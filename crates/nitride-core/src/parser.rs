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
}

pub fn parse_with_options(
    language: &'static Language,
    source: &str,
    limits: Limits,
    options: ParseOptions,
) -> Result<SyntaxTree, Diagnostic> {
    if source.len() > u32::MAX as usize {
        return Err(Diagnostic::new(
            "limit",
            [0, 0],
            "source exceeds 32-bit span capacity",
        ));
    }
    let mut parser = Parser {
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
    };
    let root = parser.rule(language.start, 0, 0)?;
    parser.trivia()?;
    if root.is_none() || parser.position != source.len() {
        let at = parser.furthest.max(parser.position) as u32;
        return Err(Diagnostic::new("syntax", [at, at], "invalid source"));
    }
    // Parsing is finished: do not overlap scratch high-water storage with the
    // returned tree's owned source copy.
    drop(parser.edge_buffers);
    drop(parser.node_buffers);
    Ok(SyntaxTree {
        language,
        source: source.into(),
        nodes: parser.nodes,
        edges: parser.edges,
        root: root.unwrap(),
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
        if depth > self.limits.depth || self.work > self.limits.work {
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
    fn rule(&mut self, id: u16, minimum: u32, depth: u32) -> Result<Option<u32>, Diagnostic> {
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
        if let Expr::Pratt { prefixes, infixes } = rule.expr {
            return self.pratt(prefixes, infixes, minimum, depth + 1);
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
        let result = self.add_node(id, start, &children).map(Some);
        Self::recycle(&mut self.edge_buffers, children);
        result
    }
    fn pratt(
        &mut self,
        prefixes: &'static [u16],
        infixes: &'static [u16],
        minimum: u32,
        depth: u32,
    ) -> Result<Option<u32>, Diagnostic> {
        self.tick(depth)?;
        let prefix_mark = self.mark();
        let mut winner = None;
        for id in prefixes {
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
        let Some(mut left) = self.accept_winner(prefix_mark, winner)? else {
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
                    out.push(Edge { label: "", node });
                    true
                } else {
                    false
                }
            }
            Expr::Label(label, inner) => {
                let from = out.len();
                let matched = self.expression(inner, lexical, owner, power, depth + 1, out)?;
                for edge in &mut out[from..] {
                    edge.label = label;
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
