use crate::*;

pub fn parse(
    language: &'static Language,
    source: &str,
    limits: Limits,
) -> Result<SyntaxTree, Diagnostic> {
    parse_with_options(language, source, limits, ParseOptions::default())
}

/// Dialect-specific trivia policy. Ordinary language parsing retains Nitrogen's
/// permissive trailing-comment behavior; grammar authoring opts into strictness.
#[derive(Debug, Default, Clone, Copy)]
pub struct ParseOptions {
    pub strict_comments: bool,
    /// Keep nodes for rules matched inside a token. Tokens are leaves by
    /// default, as in Nitrogen; the grammar frontend inspects their insides.
    pub token_children: bool,
    /// Keep nodes for alias rules. Aliases build no node by default, as in
    /// Nitrogen; the grammar frontend decodes by rule kind.
    pub alias_nodes: bool,
}

/// A rule matched inside a token: it consumed input but left no node.
const NO_NODE: u32 = u32::MAX;

pub fn parse_with_options(
    language: &'static Language,
    source: &str,
    limits: Limits,
    options: ParseOptions,
) -> Result<SyntaxTree, Diagnostic> {
    parse_from(language, source, limits, options, language.start, &[])
}

/// Template hole nodes and the holes they stand for.
pub type TemplateHoles = Vec<(u32, &'static Hole)>;

/// A quote template parsed as `category`, with `$Name` holes for `holes`.
/// The text is trimmed first, so spans are relative to the trimmed template.
/// Each returned hole is a childless node of the hole's category.
pub fn parse_template(
    language: &'static Language,
    text: &str,
    category: u16,
    holes: &[&'static Hole],
    limits: Limits,
) -> Result<(SyntaxTree, TemplateHoles), Diagnostic> {
    let tree = parse_from(
        language,
        text.trim(),
        limits,
        ParseOptions::default(),
        category,
        holes,
    )?;
    let found = (0..tree.nodes.len() as u32)
        .filter_map(|id| {
            let node = tree.nodes[id as usize];
            let name = tree.text(id)?.strip_prefix('$')?;
            (node.child_count == 0 && !language.rules[node.rule as usize].lexical)
                .then(|| {
                    holes
                        .iter()
                        .find(|h| h.name == name && h.category == node.rule)
                })
                .flatten()
                .map(|hole| (id, *hole))
        })
        .collect();
    Ok((tree, found))
}

const STACK_EXHAUSTED: &str = "parser stack limit exceeded";

/// The address of a local: the current stack position.
#[inline(never)]
fn stack_position() -> usize {
    let marker = 0u8;
    std::hint::black_box(std::ptr::addr_of!(marker) as usize)
}

/// Parses within a measured stack budget on the caller's thread. Deep input
/// reruns once on a dedicated thread with a large stack, as Roslyn moves deep
/// work off small stacks; beyond that it is a structured `limit` diagnostic.
fn parse_from(
    language: &'static Language,
    source: &str,
    limits: Limits,
    options: ParseOptions,
    start: u16,
    holes: &[&'static Hole],
) -> Result<SyntaxTree, Diagnostic> {
    match parse_on_stack(
        language,
        source,
        limits,
        options,
        start,
        holes,
        limits.parser_stack_bytes,
    ) {
        Err(e)
            if e.message == STACK_EXHAUSTED
                && limits.parser_spill_stack_bytes > limits.parser_stack_bytes =>
        {
            // Leave headroom on the spill thread for frames between checks.
            let budget = limits.parser_spill_stack_bytes - limits.parser_spill_stack_bytes / 8;
            std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .name("nitride-parse".into())
                    .stack_size(limits.parser_spill_stack_bytes)
                    .spawn_scoped(scope, || {
                        parse_on_stack(language, source, limits, options, start, holes, budget)
                    })
                    .map_err(|_| {
                        Diagnostic::new("limit", [0, 0], "cannot start a deep-parse thread")
                    })?
                    .join()
                    .unwrap_or_else(|_| Err(Diagnostic::new("limit", [0, 0], "deep parse failed")))
            })
        }
        result => result,
    }
}

fn parse_on_stack(
    language: &'static Language,
    source: &str,
    limits: Limits,
    options: ParseOptions,
    start: u16,
    holes: &[&'static Hole],
    stack_budget: usize,
) -> Result<SyntaxTree, Diagnostic> {
    if source.len() > u32::MAX as usize {
        return Err(Diagnostic::new(
            "limit",
            [0, 0],
            "source exceeds 32-bit span capacity",
        ));
    }
    let mut parser = Parser::new(language, source, limits, options, holes, stack_budget);
    let root = parser.rule(start, 0, 0)?;
    parser.trivia()?;
    if root.is_none() || parser.position != source.len() {
        let at = parser.furthest.max(parser.position) as u32;
        return Err(Diagnostic::new("syntax", [at, at], "invalid source"));
    }
    // Parsing is finished: do not overlap scratch high-water storage with the
    // returned tree's owned source copy.
    drop(parser.edge_buffers);
    drop(parser.node_buffers);
    drop(parser.memo);
    // A tree is kept (an editor holds one per document): no growth slack.
    parser.nodes.shrink_to_fit();
    parser.edges.shrink_to_fit();
    Ok(SyntaxTree {
        language,
        source: source.into(),
        nodes: parser.nodes,
        edges: parser.edges,
        labels: parser.labels,
        root: root.unwrap(),
        recovery: None,
    })
}

#[derive(Clone, Copy)]
struct Mark {
    position: usize,
    nodes: usize,
    edges: usize,
}
struct Winner {
    end: usize,
    precedence: u32,
    node: u32,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    tied: bool,
}
struct Parser<'a> {
    language: &'static Language,
    labels: Vec<&'static str>,
    label_ids: std::collections::HashMap<(usize, usize), u16>,
    source: &'a str,
    options: ParseOptions,
    limits: Limits,
    position: usize,
    furthest: usize,
    work: u64,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    // Free lists belong to this parse only. Recursive frames own taken buffers.
    edge_buffers: Vec<Vec<Edge>>,
    node_buffers: Vec<Vec<Node>>,
    // Template mode only: `$Name` stands for one node of the hole's category.
    holes: &'a [&'static Hole],
    in_token: u32,
    work_budget: u64,
    // Extension-point results by (point, position, minimum precedence).
    // Alternatives sharing a prefix reparse the same operand; replaying the
    // stored subtree keeps nesting linear, as Nitrogen's memoization does.
    // Two generations bound the cache: when the current one fills, the older is
    // dropped. Eviction only costs reparsing; it never changes a result.
    memo: [MemoGeneration; 2],
    memo_capacity: usize,
    stack_base: usize,
    stack_budget: usize,
}

/// Memoized subtrees live in shared arenas, so entries allocate nothing.
#[derive(Default)]
struct MemoGeneration {
    entries: std::collections::HashMap<(u16, u32, u32), Memo>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

/// Larger subtrees (whole members, classes) are not memoized: repeated
/// reparsing comes from operands that several alternatives share.
const MEMO_SUBTREE_LIMIT: usize = 8192;

/// A memoized extension-point parse, relative to its own first node and edge.
enum Memo {
    Fail {
        furthest: usize,
    },
    Hit {
        end: usize,
        furthest: usize,
        root: u32,
        nodes: std::ops::Range<u32>,
        edges: std::ops::Range<u32>,
    },
}

impl<'a> Parser<'a> {
    fn new(
        language: &'static Language,
        source: &'a str,
        limits: Limits,
        options: ParseOptions,
        holes: &'a [&'static Hole],
        stack_budget: usize,
    ) -> Self {
        Self {
            language,
            source,
            options,
            limits,
            position: 0,
            furthest: 0,
            work: 0,
            nodes: Vec::new(),
            edges: Vec::new(),
            edge_buffers: Vec::new(),
            node_buffers: Vec::new(),
            holes,
            labels: vec![""],
            label_ids: Default::default(),
            in_token: 0,
            work_budget: limits
                .work
                .max((source.len() as u64).saturating_mul(limits.parser_work_per_byte)),
            memo: Default::default(),
            // Reparsing revisits recent positions, so a window suffices: an eighth
            // of the input, between 4,096 and about a million nodes per generation.
            memo_capacity: (source.len() / 8).clamp(4096, 1 << 20),
            stack_base: stack_position(),
            stack_budget,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProbeHit {
    pub root: u32,
    pub end: usize,
    pub furthest: usize,
}

/// Strict prefix recognizer and its immutable successful subtree arena. Recovery
/// uses original-source offsets; probes never edit or synthesize token text.
pub(crate) struct StrictProbe<'a> {
    parser: Parser<'a>,
    cache: std::collections::HashMap<(u16, usize, u32), Option<ProbeHit>>,
    committed_nodes: usize,
    committed_edges: usize,
}
impl<'a> StrictProbe<'a> {
    pub fn new(language: &'static Language, source: &'a str, limits: Limits) -> Self {
        Self {
            parser: Parser::new(
                language,
                source,
                limits,
                ParseOptions {
                    strict_comments: true,
                    ..Default::default()
                },
                &[],
                limits.parser_stack_bytes,
            ),
            cache: Default::default(),
            committed_nodes: 0,
            committed_edges: 0,
        }
    }
    pub fn rule(&mut self, id: u16, at: usize, power: u32) -> Result<Option<ProbeHit>, Diagnostic> {
        if let Some(hit) = self.cache.get(&(id, at, power)) {
            return Ok(*hit);
        }
        self.parser.position = at;
        self.parser.furthest = at;
        let root = self.parser.rule(id, power, 0)?;
        let hit = root.map(|root| ProbeHit {
            root,
            end: self.parser.position,
            furthest: self.parser.furthest,
        });
        self.cache.insert((id, at, power), hit);
        Ok(hit)
    }
    pub fn predicate(&mut self, expr: &'static Expr, at: usize) -> Result<bool, Diagnostic> {
        self.parser.position = at;
        let mark = self.parser.mark();
        let mut out = Vec::new();
        let result = self.parser.expression(expr, false, None, 0, 0, &mut out);
        self.parser.restore(mark);
        result
    }
    /// Pack only newly accepted subtrees after a stable continuation. Previously
    /// committed identities remain valid; speculative probes are discarded.
    pub fn commit(&mut self, roots: &mut [u32], keep_from: usize) {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut mapped = std::collections::HashMap::new();
        for root in roots {
            if *root == NO_NODE || (*root as usize) < self.committed_nodes {
                continue;
            }
            let mut pending = vec![(*root, false)];
            while let Some((old, visited)) = pending.pop() {
                if (old as usize) < self.committed_nodes || mapped.contains_key(&old) {
                    continue;
                }
                let n = self.parser.nodes[old as usize];
                let children = &self.parser.edges
                    [n.first_child as usize..(n.first_child + n.child_count) as usize];
                if !visited {
                    pending.push((old, true));
                    pending.extend(children.iter().rev().map(|e| (e.node, false)));
                } else {
                    let id = (self.committed_nodes + nodes.len()) as u32;
                    nodes.push(Node {
                        first_child: (self.committed_edges + edges.len()) as u32,
                        ..n
                    });
                    edges.extend(children.iter().map(|e| Edge {
                        node: if (e.node as usize) < self.committed_nodes {
                            e.node
                        } else {
                            mapped[&e.node]
                        },
                        ..*e
                    }));
                    mapped.insert(old, id);
                }
            }
            *root = mapped[root];
        }
        self.parser.nodes.truncate(self.committed_nodes);
        self.parser.edges.truncate(self.committed_edges);
        self.parser.nodes.extend(nodes);
        self.parser.edges.extend(edges);
        self.cache.retain(|(_, at, _), hit| {
            if *at < keep_from {
                return false;
            }
            match hit {
                None => true,
                Some(hit) if hit.root == NO_NODE || (hit.root as usize) < self.committed_nodes => {
                    true
                }
                Some(hit) => {
                    if let Some(root) = mapped.get(&hit.root) {
                        hit.root = *root;
                        true
                    } else {
                        false
                    }
                }
            }
        });
        self.committed_nodes = self.parser.nodes.len();
        self.committed_edges = self.parser.edges.len();
    }
    pub fn nodes(&self) -> &[Node] {
        &self.parser.nodes
    }
    pub fn edges(&self) -> &[Edge] {
        &self.parser.edges
    }
    pub fn labels(&self) -> &[&'static str] {
        &self.parser.labels
    }
    pub fn label(&self, edge: &Edge) -> &'static str {
        self.parser.labels[edge.label as usize]
    }
    pub fn work(&self) -> u64 {
        self.parser.work
    }
}

impl Parser<'_> {
    fn recycle<T>(pool: &mut Vec<Vec<T>>, mut buffer: Vec<T>) {
        if buffer.capacity() != 0 {
            buffer.clear();
            pool.push(buffer);
        }
    }
    fn consider(&mut self, mark: Mark, node: u32, precedence: u32, winner: &mut Option<Winner>) {
        let rank = (self.position, precedence);
        if winner.as_ref().is_none_or(|w| rank > (w.end, w.precedence)) {
            let best = winner.get_or_insert_with(|| Winner {
                end: 0,
                precedence: 0,
                node: 0,
                nodes: self.node_buffers.pop().unwrap_or_default(),
                edges: self.edge_buffers.pop().unwrap_or_default(),
                tied: false,
            });
            best.end = self.position;
            best.precedence = precedence;
            best.node = node;
            best.nodes.clear();
            best.nodes.extend_from_slice(&self.nodes[mark.nodes..]);
            best.edges.clear();
            best.edges.extend_from_slice(&self.edges[mark.edges..]);
            best.tied = false;
        } else if let Some(best) = winner.as_mut() {
            if rank == (best.end, best.precedence) {
                best.tied = true;
            }
        }
    }
    fn accept_winner(
        &mut self,
        mark: Mark,
        winner: Option<Winner>,
    ) -> Result<Option<u32>, Diagnostic> {
        self.restore(mark);
        let Some(winner) = winner else {
            return Ok(None);
        };
        if winner.tied {
            return Err(Diagnostic::new(
                "ambiguity",
                [mark.position as u32, winner.end as u32],
                "equally ranked extensible alternatives are outside current admission",
            ));
        }
        self.position = winner.end;
        self.nodes.extend_from_slice(&winner.nodes);
        self.edges.extend_from_slice(&winner.edges);
        Self::recycle(&mut self.node_buffers, winner.nodes);
        Self::recycle(&mut self.edge_buffers, winner.edges);
        Ok(Some(winner.node))
    }
    /// This tree's index for a label; labels are static grammar strings, so
    /// they are keyed by address (a repeated spelling only costs a slot).
    fn intern(&mut self, label: &'static str) -> Result<u16, Diagnostic> {
        if label.is_empty() {
            return Ok(0);
        }
        let key = (label.as_ptr() as usize, label.len());
        if let Some(&id) = self.label_ids.get(&key) {
            return Ok(id);
        }
        let id = u16::try_from(self.labels.len()).map_err(|_| {
            Diagnostic::new(
                "limit",
                [self.position as u32; 2],
                "too many distinct labels",
            )
        })?;
        self.labels.push(label);
        self.label_ids.insert(key, id);
        Ok(id)
    }
    fn mark(&self) -> Mark {
        Mark {
            position: self.position,
            nodes: self.nodes.len(),
            edges: self.edges.len(),
        }
    }
    fn restore(&mut self, mark: Mark) {
        self.position = mark.position;
        self.nodes.truncate(mark.nodes);
        self.edges.truncate(mark.edges);
    }
    fn tick(&mut self, depth: u32) -> Result<(), Diagnostic> {
        self.work += 1;
        if stack_position().abs_diff(self.stack_base) > self.stack_budget {
            return Err(Diagnostic::new(
                "limit",
                [self.position as u32; 2],
                STACK_EXHAUSTED,
            ));
        }
        if depth > self.limits.parser_frames || self.work > self.work_budget {
            return Err(Diagnostic::new(
                "limit",
                [self.position as u32; 2],
                "parser depth/work limit exceeded",
            ));
        }
        Ok(())
    }
    fn trivia(&mut self) -> Result<(), Diagnostic> {
        loop {
            self.tick(0)?;
            let rest = &self.source[self.position..];
            if let Some(c) = rest.chars().next().filter(|c| c.is_whitespace()) {
                self.position += c.len_utf8();
            } else if rest.starts_with("//") {
                self.position += rest.find('\n').unwrap_or(rest.len());
            } else if let Some(after_open) = rest.strip_prefix("/*") {
                if let Some(end) = after_open.find("*/") {
                    self.position += end + 4;
                } else if self.options.strict_comments {
                    return Err(Diagnostic::new(
                        "syntax",
                        [self.position as u32, self.source.len() as u32],
                        "unterminated comment",
                    ));
                } else {
                    self.position = self.source.len();
                }
            } else {
                return Ok(());
            }
        }
    }
    fn add_node(&mut self, rule: u16, start: usize, children: &[Edge]) -> Result<u32, Diagnostic> {
        if self.nodes.len() >= u32::MAX as usize
            || self.edges.len() + children.len() > u32::MAX as usize
        {
            return Err(Diagnostic::new(
                "limit",
                [start as u32; 2],
                "syntax storage capacity exceeded",
            ));
        }
        let id = self.nodes.len() as u32;
        let node = Node {
            rule,
            span: [start as u32, self.position as u32],
            first_child: self.edges.len() as u32,
            child_count: children.len() as u32,
        };
        self.edges.extend_from_slice(children);
        self.nodes.push(node);
        Ok(id)
    }
    /// The length of a `$Name` hole of `category` at the current position.
    fn hole(&self, category: u16) -> Option<usize> {
        if self.holes.is_empty() {
            return None;
        }
        let rest = self.source[self.position..].strip_prefix('$')?;
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        (name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && self
                .holes
                .iter()
                .any(|h| h.name == name && h.category == category))
        .then_some(end + 1)
    }
    fn rule(&mut self, id: u16, minimum: u32, depth: u32) -> Result<Option<u32>, Diagnostic> {
        let lexical = self
            .language
            .rules
            .get(id as usize)
            .is_some_and(|r| r.lexical);
        if !lexical || self.options.token_children {
            return self.match_rule(id, minimum, depth);
        }
        self.in_token += 1;
        let result = self.match_rule(id, minimum, depth);
        self.in_token -= 1;
        match result {
            // A token inside a token leaves no node: the outermost token is a leaf.
            Ok(Some(node)) if self.in_token > 0 && node != NO_NODE => {
                let first_child = self.nodes[node as usize].first_child;
                self.nodes.pop();
                self.edges.truncate(first_child as usize);
                Ok(Some(NO_NODE))
            }
            other => other,
        }
    }
    fn match_rule(&mut self, id: u16, minimum: u32, depth: u32) -> Result<Option<u32>, Diagnostic> {
        self.tick(depth)?;
        let rule = self
            .language
            .rules
            .get(id as usize)
            .ok_or_else(|| Diagnostic::new("grammar", [0, 0], "invalid rule identity"))?;
        let mark = self.mark();
        if !rule.lexical {
            self.trivia()?;
        }
        let start = self.position;
        if let Expr::Pratt {
            prefixes,
            infixes,
            memoize,
        } = rule.expr
        {
            return if memoize {
                self.pratt(id, prefixes, infixes, minimum, depth + 1)
            } else {
                self.pratt_uncached(id, prefixes, infixes, minimum, depth + 1)
            };
        }
        if !rule.lexical {
            if let Some(length) = self.hole(id) {
                self.position += length;
                return self.add_node(id, start, &[]).map(Some);
            }
        }
        let mut children = self.edge_buffers.pop().unwrap_or_default();
        let lexical_failure_base = self.furthest;
        // Only a direct final reference to the extensible point is an operand.
        // Enclosed expressions start a fresh precedence context, as in Nitrogen.
        let matched = if rule.owner.is_some() {
            let parts = match &rule.expr {
                Expr::Sequence(parts) => *parts,
                expr => std::slice::from_ref(expr),
            };
            let mut matched = true;
            for (index, part) in parts.iter().enumerate() {
                let owner = if index + 1 == parts.len() {
                    rule.owner
                } else {
                    None
                };
                if !self.expression(part, rule.lexical, owner, minimum, depth + 1, &mut children)? {
                    matched = false;
                    break;
                }
            }
            matched
        } else {
            self.expression(&rule.expr, rule.lexical, None, 0, depth + 1, &mut children)?
        };
        if rule.lexical {
            self.furthest = lexical_failure_base;
        }
        if !matched || rule.except.contains(&&self.source[start..self.position]) {
            self.furthest = self
                .furthest
                .max(if rule.lexical { start } else { self.position });
            Self::recycle(&mut self.edge_buffers, children);
            self.restore(mark);
            return Ok(None);
        }
        // An alias builds no node: its one child stands in, labeled by the
        // reference to the alias. With no child, nothing remains, except for
        // an extensible alternative, which must yield an operand node.
        if rule.alias
            && !self.options.alias_nodes
            && (children.len() == 1 || (children.is_empty() && rule.owner.is_none()))
        {
            let node = children.first().map_or(NO_NODE, |edge| edge.node);
            Self::recycle(&mut self.edge_buffers, children);
            return Ok(Some(node));
        }
        let result = self.add_node(id, start, &children).map(Some);
        Self::recycle(&mut self.edge_buffers, children);
        result
    }
    /// A transparent alternative's one element node stands in for it. The
    /// alternative's own node is the last one added, so it is simply removed.
    /// With no element node (a literal), it stays and observers hide it.
    fn stand_in(&mut self, node: u32) -> u32 {
        let current = self.nodes[node as usize];
        if !self.language.rules[current.rule as usize].transparent
            || current.child_count != 1
            || node as usize + 1 != self.nodes.len()
        {
            return node;
        }
        let child = self.edges[current.first_child as usize].node;
        self.nodes.pop();
        self.edges.truncate(current.first_child as usize);
        child
    }
    fn pratt(
        &mut self,
        point: u16,
        prefixes: &'static [u16],
        infixes: &'static [u16],
        minimum: u32,
        depth: u32,
    ) -> Result<Option<u32>, Diagnostic> {
        let key = (point, self.position as u32, minimum);
        let found = (0..2).find_map(|g| self.memo[g].entries.get(&key).map(|e| (g, e)));
        if let Some((generation, entry)) = found {
            return match entry {
                Memo::Fail { furthest } => {
                    self.furthest = self.furthest.max(*furthest);
                    Ok(None)
                }
                Memo::Hit {
                    end,
                    furthest,
                    root,
                    nodes,
                    edges,
                } => {
                    let (end, furthest, root) = (*end, *furthest, *root);
                    let memo = &self.memo[generation];
                    let nodes = &memo.nodes[nodes.start as usize..nodes.end as usize];
                    let edges = &memo.edges[edges.start as usize..edges.end as usize];
                    let (node_base, edge_base) = (self.nodes.len() as u32, self.edges.len() as u32);
                    if node_base as usize + nodes.len() >= u32::MAX as usize
                        || edge_base as usize + edges.len() > u32::MAX as usize
                    {
                        return Err(Diagnostic::new(
                            "limit",
                            [self.position as u32; 2],
                            "syntax storage capacity exceeded",
                        ));
                    }
                    self.nodes.extend(nodes.iter().map(|n| Node {
                        first_child: n.first_child + edge_base,
                        ..*n
                    }));
                    self.edges.extend(edges.iter().map(|e| Edge {
                        node: e.node + node_base,
                        ..*e
                    }));
                    self.position = end;
                    self.furthest = self.furthest.max(furthest);
                    Ok(Some(root + node_base))
                }
            };
        }
        let mark = self.mark();
        let result = self.pratt_uncached(point, prefixes, infixes, minimum, depth)?;
        if result.is_some_and(|_| self.nodes.len() - mark.nodes > MEMO_SUBTREE_LIMIT) {
            return Ok(result);
        }
        if self.memo[0].nodes.len() + self.memo[0].entries.len() > self.memo_capacity {
            self.memo.swap(0, 1);
            let current = &mut self.memo[0];
            current.entries.clear();
            current.nodes.clear();
            current.edges.clear();
        }
        let furthest = self.furthest;
        let position = self.position;
        let current = &mut self.memo[0];
        let entry = match result {
            None => Memo::Fail { furthest },
            Some(root) => {
                let node_start = current.nodes.len() as u32;
                let edge_start = current.edges.len() as u32;
                current
                    .nodes
                    .extend(self.nodes[mark.nodes..].iter().map(|n| Node {
                        first_child: n.first_child - mark.edges as u32,
                        ..*n
                    }));
                current
                    .edges
                    .extend(self.edges[mark.edges..].iter().map(|e| Edge {
                        node: e.node - mark.nodes as u32,
                        ..*e
                    }));
                Memo::Hit {
                    end: position,
                    furthest,
                    root: root - mark.nodes as u32,
                    nodes: node_start..current.nodes.len() as u32,
                    edges: edge_start..current.edges.len() as u32,
                }
            }
        };
        current.entries.insert(key, entry);
        Ok(result)
    }

    fn pratt_uncached(
        &mut self,
        point: u16,
        prefixes: &'static [u16],
        infixes: &'static [u16],
        minimum: u32,
        depth: u32,
    ) -> Result<Option<u32>, Diagnostic> {
        self.tick(depth)?;
        let prefix_mark = self.mark();
        let mut winner = None;
        // A hole is one operand node; operators may still follow it.
        let hole = self.hole(point);
        for id in prefixes.iter().filter(|_| hole.is_none()) {
            self.restore(prefix_mark);
            let rule =
                self.language.rules.get(*id as usize).ok_or_else(|| {
                    Diagnostic::new("grammar", [0, 0], "invalid Pratt rule identity")
                })?;
            if let Some(node) = self.rule(
                *id,
                rule.precedence
                    .checked_add(1)
                    .ok_or_else(|| Diagnostic::new("grammar", [0, 0], "precedence overflow"))?,
                depth + 1,
            )? {
                if self.position > prefix_mark.position {
                    self.consider(prefix_mark, node, rule.precedence, &mut winner);
                }
            }
        }
        let left = if let Some(length) = hole {
            let start = self.position;
            self.position += length;
            Some(self.add_node(point, start, &[])?)
        } else {
            self.accept_winner(prefix_mark, winner)?
                .map(|node| self.stand_in(node))
        };
        let Some(mut left) = left else {
            return Ok(None);
        };
        loop {
            self.tick(depth)?;
            let infix_mark = self.mark();
            let mut winner = None;
            for id in infixes {
                self.restore(infix_mark);
                let rule = self.language.rules.get(*id as usize).ok_or_else(|| {
                    Diagnostic::new("grammar", [0, 0], "invalid Pratt rule identity")
                })?;
                if rule.precedence < minimum {
                    continue;
                }
                let Expr::Sequence(parts) = &rule.expr else {
                    return Err(Diagnostic::new(
                        "grammar",
                        [0, 0],
                        "infix must be a sequence",
                    ));
                };
                let label = match parts.first() {
                    Some(Expr::Label(label, _)) => *label,
                    Some(Expr::Ref(_)) => "",
                    _ => {
                        return Err(Diagnostic::new(
                            "grammar",
                            [0, 0],
                            "invalid infix left operand",
                        ));
                    }
                };
                let mark = self.mark();
                let start = self.nodes[left as usize].span[0] as usize;
                let mut children = self.edge_buffers.pop().unwrap_or_default();
                let label = self.intern(label)?;
                children.push(Edge { label, node: left });
                let power = rule
                    .precedence
                    .checked_add(u32::from(!rule.right_associative))
                    .ok_or_else(|| Diagnostic::new("grammar", [0, 0], "precedence overflow"))?;
                let mut matched = true;
                for (index, part) in parts.iter().enumerate().skip(1) {
                    let owner = if index + 1 == parts.len() {
                        rule.owner
                    } else {
                        None
                    };
                    if !self.expression(part, false, owner, power, depth + 1, &mut children)? {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    if self.position == mark.position {
                        return Err(Diagnostic::new(
                            "grammar",
                            [start as u32; 2],
                            "zero-width infix",
                        ));
                    }
                    let node = self.add_node(*id, start, &children)?;
                    self.consider(infix_mark, node, rule.precedence, &mut winner);
                }
                Self::recycle(&mut self.edge_buffers, children);
                self.restore(mark);
            }
            if let Some(node) = self.accept_winner(infix_mark, winner)? {
                left = node;
            } else {
                return Ok(Some(left));
            }
        }
    }
    fn expression(
        &mut self,
        expr: &'static Expr,
        lexical: bool,
        owner: Option<u16>,
        power: u32,
        depth: u32,
        out: &mut Vec<Edge>,
    ) -> Result<bool, Diagnostic> {
        self.tick(depth)?;
        if !lexical
            && matches!(
                expr,
                Expr::Literal(_) | Expr::Class(_) | Expr::NotClass(_) | Expr::Any | Expr::Ref(_)
            )
        {
            self.trivia()?;
        }
        let result = match expr {
            Expr::Any => {
                if let Some(c) = self.source[self.position..].chars().next() {
                    self.position += c.len_utf8();
                    true
                } else {
                    false
                }
            }
            Expr::Predicate { positive, inner } => {
                let mark = self.mark();
                let furthest = self.furthest;
                let len = out.len();
                let matched = self.expression(inner, lexical, None, 0, depth + 1, out);
                self.restore(mark);
                self.furthest = furthest;
                out.truncate(len);
                matched? == *positive
            }
            Expr::Literal(text) => {
                let keyword = !lexical
                    && text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_')
                    && text.chars().all(|c| c.is_alphanumeric() || c == '_');
                let end = self.position + text.len();
                let boundary = !keyword
                    || self
                        .source
                        .get(end..)
                        .and_then(|s| s.chars().next())
                        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                if self.source[self.position..].starts_with(text) && boundary {
                    self.position += text.len();
                    true
                } else {
                    false
                }
            }
            Expr::Class(ranges) | Expr::NotClass(ranges) => {
                if let Some(c) = self.source[self.position..].chars().next().filter(|c| {
                    ranges.iter().any(|(a, b)| a <= c && c <= b)
                        != matches!(expr, Expr::NotClass(_))
                }) {
                    self.position += c.len_utf8();
                    true
                } else {
                    false
                }
            }
            Expr::Ref(id) => {
                if let Some(node) =
                    self.rule(*id, if Some(*id) == owner { power } else { 0 }, depth + 1)?
                {
                    if node != NO_NODE {
                        out.push(Edge { label: 0, node });
                    }
                    true
                } else {
                    false
                }
            }
            Expr::Label(label, inner) => {
                let from = out.len();
                let matched = self.expression(inner, lexical, owner, power, depth + 1, out)?;
                if out.len() > from {
                    let label = self.intern(label)?;
                    for edge in &mut out[from..] {
                        edge.label = label;
                    }
                }
                matched
            }
            Expr::Sequence(parts) => {
                let mut matched = true;
                for part in *parts {
                    if !self.expression(part, lexical, None, 0, depth + 1, out)? {
                        matched = false;
                        break;
                    }
                }
                matched
            }
            Expr::Choice(parts) => {
                let mut matched = false;
                for part in *parts {
                    let mark = self.mark();
                    let len = out.len();
                    if self.expression(part, lexical, None, 0, depth + 1, out)? {
                        matched = true;
                        break;
                    }
                    self.restore(mark);
                    out.truncate(len);
                }
                matched
            }
            Expr::Repeat { inner, min, max } => {
                let mut count = 0;
                while count < *max {
                    let mark = self.mark();
                    let len = out.len();
                    if !self.expression(inner, lexical, None, 0, depth + 1, out)? {
                        self.restore(mark);
                        out.truncate(len);
                        break;
                    }
                    if self.position == mark.position && *max > 1 {
                        return Err(Diagnostic::new(
                            "grammar",
                            [self.position as u32; 2],
                            "zero-width repetition",
                        ));
                    }
                    count += 1;
                }
                count >= *min
            }
            Expr::Pratt { .. } => {
                return Err(Diagnostic::new(
                    "grammar",
                    [0, 0],
                    "Pratt expressions must be rule roots",
                ));
            }
        };
        if !result {
            self.furthest = self.furthest.max(self.position);
        }
        Ok(result)
    }
}
