//! Structural quote matching for rule checks, named patterns and function
//! arms, as Nitrogen PR 92 defines it. A quote is parsed once as its
//! category; `$name` holes bind nodes, `..$name` spreads (checks only) take
//! runs of siblings, a capture naming a pattern must match it, and a
//! `..Pattern` capture searches at or below its hole. All work is charged to
//! a [`Budget`], so no pattern runs unbounded or exhausts the stack.
use crate::parser::parse_quote;
use crate::syntax::access::SyntaxAccess;
use crate::*;

/// Steps one check may take: its own match, every alternative tried, every
/// descendant and ancestor visited (Nitrogen's `QuoteMatchOptions.MaxWorkSteps`).
pub const MAX_WORK_STEPS: u32 = 1_000_000;

/// Stack a match may use below where its budget began. Deep recursion through
/// named patterns ends here as a spent budget rather than a stack overflow.
const STACK_BYTES: usize = 512 * 1024;

/// A match ran out of work steps (or stack): Nitrogen's `NQP0006`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitExceeded;

#[derive(Debug)]
pub struct Budget {
    remaining: u32,
    base: usize,
}

impl Budget {
    pub fn new(steps: u32) -> Self {
        Self {
            remaining: steps,
            base: stack_position(),
        }
    }

    pub fn step(&mut self) -> Result<(), LimitExceeded> {
        if self.remaining == 0 || self.base.saturating_sub(stack_position()) > STACK_BYTES {
            return Err(LimitExceeded);
        }
        self.remaining -= 1;
        Ok(())
    }
}

#[inline(never)]
fn stack_position() -> usize {
    let marker = 0u8;
    std::hint::black_box(std::ptr::addr_of!(marker) as usize)
}

/// Which matcher's rules a quote keeps. Checks take `..$x` spreads, accept a
/// node whose rule derives from the hole's category through aliases, and
/// compare a repeated capture by text. Function arms accept the category or a
/// rule it owns and compare a repeated capture by structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Check,
    Arm,
}

#[derive(Debug, Clone, Copy)]
struct QuoteHole<'a> {
    name: &'a str,
    category: u16,
    spread: bool,
    pattern: Option<u16>,
    descendant: bool,
}

/// A compiled quote: its template tree and the hole each hole node stands for.
pub struct Quote<'a> {
    language: &'a Language<'a>,
    flavor: Flavor,
    tree: SyntaxTree<'a>,
    holes: Vec<(u32, QuoteHole<'a>)>,
}

/// A language's named patterns, compiled: each pattern's category and
/// alternatives, by index in `Language::patterns`.
pub struct Patterns<'a> {
    compiled: Vec<(u16, Vec<Quote<'a>>)>,
}

impl<'a> Patterns<'a> {
    pub fn compile(language: &'a Language<'a>) -> Result<Self, Diagnostic> {
        let mut compiled = Vec::with_capacity(language.patterns.len());
        for pattern in language.patterns {
            let mut alternatives = Vec::with_capacity(pattern.alternatives.len());
            for (index, alternative) in pattern.alternatives.iter().enumerate() {
                let quote = Quote::compile(
                    language,
                    pattern.category,
                    alternative.captures,
                    alternative.pattern,
                    Flavor::Check,
                )
                .map_err(|e| {
                    Diagnostic::new(
                        "check",
                        [0, 0],
                        format!(
                            "alternative {} of pattern {} does not parse as {}: {}",
                            index + 1,
                            pattern.name,
                            language.rules[pattern.category as usize].name,
                            e.message
                        ),
                    )
                })?;
                alternatives.push(quote);
            }
            compiled.push((pattern.category, alternatives));
        }
        Ok(Self { compiled })
    }

    /// Whether named pattern `index` matches node `t`: its alternatives in
    /// order, the first success committed, each with captures of its own.
    fn matches_at(
        &self,
        index: u16,
        tree: &dyn SyntaxAccess<'_>,
        t: u32,
        budget: &mut Budget,
    ) -> Result<bool, LimitExceeded> {
        let Some((category, alternatives)) = self.compiled.get(index as usize) else {
            return Ok(false);
        };
        let Some(first) = alternatives.first() else {
            return Ok(false);
        };
        if !stands_for(first.language, *category, tree.nodes()[t as usize].rule) {
            return Ok(false);
        }
        for alternative in alternatives {
            budget.step()?;
            let mut private = Vec::new();
            if alternative.node(self, tree, alternative.tree.root, t, budget, &mut private)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl<'a> Quote<'a> {
    pub fn compile(
        language: &'a Language<'a>,
        category: u16,
        captures: &'a [Hole<'a>],
        text: &'a str,
        flavor: Flavor,
    ) -> Result<Self, Diagnostic> {
        let (text, spreads) = match flavor {
            Flavor::Check => spreads(text),
            Flavor::Arm => (text.to_owned(), Vec::new()),
        };
        let every = |name: &'a str, pattern: Option<u16>, descendant: bool| {
            language
                .rules
                .iter()
                .enumerate()
                .filter(|(_, r)| !r.lexical)
                .map(move |(category, _)| Hole {
                    name,
                    category: category as u16,
                    pattern,
                    descendant,
                })
        };
        // A spread or a descendant capture accepts any node at its site.
        let mut holes: Vec<Hole<'a>> = Vec::new();
        for capture in captures {
            if capture.descendant {
                holes.extend(every(capture.name, capture.pattern, true));
            } else {
                holes.push(Hole {
                    name: capture.name,
                    category: capture.category,
                    pattern: capture.pattern,
                    descendant: false,
                });
            }
        }
        for &name in &spreads {
            holes.extend(every(name, None, false));
        }
        let refs: Vec<&Hole<'_>> = holes.iter().collect();
        let (tree, found) = parse_quote(language, &text, category, &refs, Limits::default())?;
        if !stands_for(language, category, tree.nodes[tree.root as usize].rule) {
            return Err(Diagnostic::new(
                "check",
                [0, 0],
                "the pattern is another rule",
            ));
        }
        let holes: Vec<(u32, QuoteHole<'a>)> = found
            .into_iter()
            .map(|(id, index)| {
                let hole = &holes[index];
                (
                    id,
                    QuoteHole {
                        name: hole.name,
                        category: hole.category,
                        spread: spreads.contains(&hole.name),
                        pattern: hole.pattern,
                        descendant: hole.descendant,
                    },
                )
            })
            .collect();
        for capture in captures.iter().filter(|c| c.descendant) {
            if holes.iter().filter(|(_, h)| h.name == capture.name).count() > 1 {
                return Err(Diagnostic::new(
                    "check",
                    [0, 0],
                    "NQP0009: A descendant capture may occur only once in a pattern.",
                ));
            }
        }
        Ok(Self {
            language,
            flavor,
            tree,
            holes,
        })
    }

    /// Whether the quote matches node `node` of `tree`, appending captures to
    /// `bindings` (left as they were when it does not match).
    pub fn matches(
        &self,
        patterns: &Patterns<'a>,
        tree: &SyntaxTree<'_>,
        node: u32,
        budget: &mut Budget,
        bindings: &mut Vec<(&'a str, u32)>,
    ) -> Result<bool, LimitExceeded> {
        self.matches_in(patterns, tree, node, budget, bindings)
    }

    pub(crate) fn matches_in(
        &self,
        patterns: &Patterns<'a>,
        tree: &dyn SyntaxAccess<'_>,
        node: u32,
        budget: &mut Budget,
        bindings: &mut Vec<(&'a str, u32)>,
    ) -> Result<bool, LimitExceeded> {
        let kept = bindings.len();
        let matched = self.node(patterns, tree, self.tree.root, node, budget, bindings)?;
        if !matched {
            bindings.truncate(kept);
        }
        Ok(matched)
    }

    /// Whether a hole of the quote binds `name`.
    pub(crate) fn binds(&self, name: &str) -> bool {
        self.holes.iter().any(|(_, h)| h.name == name)
    }

    fn hole(&self, id: u32) -> Option<QuoteHole<'a>> {
        self.holes.iter().find(|h| h.0 == id).map(|h| h.1)
    }

    /// Whether a node of `rule` may fill a hole of `category`.
    fn accepts(&self, category: u16, rule: u16) -> bool {
        match self.flavor {
            Flavor::Check => stands_for(self.language, category, rule),
            Flavor::Arm => {
                rule == category || self.language.rules[rule as usize].owner == Some(category)
            }
        }
    }

    /// Whether `a` and `b`, captured under one name, count as the same.
    fn same(&self, tree: &dyn SyntaxAccess<'_>, a: u32, b: u32) -> bool {
        match self.flavor {
            Flavor::Check => tree.text(a) == tree.text(b),
            Flavor::Arm => same_syntax(self.language, tree, a, b),
        }
    }

    /// Whether pattern node `p` matches node `t` of `tree`, binding captures.
    fn node(
        &self,
        patterns: &Patterns<'a>,
        tree: &dyn SyntaxAccess<'_>,
        p: u32,
        t: u32,
        budget: &mut Budget,
        bindings: &mut Vec<(&'a str, u32)>,
    ) -> Result<bool, LimitExceeded> {
        budget.step()?;
        let target = tree.nodes()[t as usize];
        if let Some(hole) = self.hole(p) {
            if hole.spread {
                return Ok(true);
            }
            if hole.descendant {
                // Preorder, document order, from the hole's own node.
                let Some(pattern) = hole.pattern else {
                    return Ok(false);
                };
                let mut pending = vec![t];
                while let Some(candidate) = pending.pop() {
                    budget.step()?;
                    if patterns.matches_at(pattern, tree, candidate, budget)? {
                        bindings.push((hole.name, candidate));
                        return Ok(true);
                    }
                    pending.extend(tree.children(candidate).iter().rev().map(|e| e.node));
                }
                return Ok(false);
            }
            if !self.accepts(hole.category, target.rule) {
                return Ok(false);
            }
            if let Some(pattern) = hole.pattern
                && !patterns.matches_at(pattern, tree, t, budget)?
            {
                return Ok(false);
            }
            if let Some(&(_, bound)) = bindings.iter().find(|(n, _)| *n == hole.name) {
                return Ok(self.same(tree, bound, t));
            }
            bindings.push((hole.name, t));
            return Ok(true);
        }
        let node = self.tree.nodes[p as usize];
        if node.rule != target.rule {
            return Ok(false);
        }
        let (ps, ts) = (self.tree.children(p), tree.children(t));
        match self.flavor {
            Flavor::Check => {
                if ps.is_empty() && ts.is_empty() {
                    let spelling = |text: Option<&str>| {
                        text.unwrap_or_default()
                            .split_whitespace()
                            .collect::<String>()
                    };
                    return Ok(spelling(self.tree.text(p)) == spelling(tree.text(t)));
                }
                self.list(patterns, tree, ps, ts, budget, bindings)
            }
            Flavor::Arm => {
                if self.language.rules[node.rule as usize].lexical {
                    return Ok(self.tree.text(p) == tree.text(t));
                }
                if ps.len() != ts.len() {
                    return Ok(false);
                }
                for (l, r) in ps.iter().zip(ts) {
                    if self.tree.label(l) != tree.label(r)
                        || !self.node(patterns, tree, l.node, r.node, budget, bindings)?
                    {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
        }
    }

    /// Pattern children `ps` against target children `ts`, spreads taking runs.
    fn list(
        &self,
        patterns: &Patterns<'a>,
        tree: &dyn SyntaxAccess<'_>,
        ps: &[Edge],
        ts: &[Edge],
        budget: &mut Budget,
        bindings: &mut Vec<(&'a str, u32)>,
    ) -> Result<bool, LimitExceeded> {
        let Some((first, rest)) = ps.split_first() else {
            return Ok(ts.is_empty());
        };
        let label = self.tree.label(first);
        if self.hole(first.node).is_some_and(|h| h.spread) {
            // A spread takes any run of siblings under its own field label.
            let run = ts.iter().take_while(|e| tree.label(e) == label).count();
            for taken in 0..=run {
                budget.step()?;
                let kept = bindings.len();
                if self.list(patterns, tree, rest, &ts[taken..], budget, bindings)? {
                    return Ok(true);
                }
                bindings.truncate(kept);
            }
            return Ok(false);
        }
        let Some((next, more)) = ts.split_first() else {
            return Ok(false);
        };
        let kept = bindings.len();
        let matched = tree.label(next) == label
            && self.node(patterns, tree, first.node, next.node, budget, bindings)?
            && self.list(patterns, tree, rest, more, budget, bindings)?;
        if !matched {
            bindings.truncate(kept);
        }
        Ok(matched)
    }
}

/// A pattern's text with each `..$name` spread written `$name`, and the spread names.
pub(crate) fn spreads(pattern: &str) -> (String, Vec<&str>) {
    let mut names = Vec::new();
    let mut text = String::with_capacity(pattern.len());
    let mut rest = pattern;
    while let Some(at) = rest.find("..$") {
        text.push_str(&rest[..at]);
        let start = at + 3;
        let end = rest[start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map_or(rest.len(), |e| start + e);
        names.push(&rest[start..end]);
        text.push('$');
        text.push_str(&rest[start..end]);
        rest = &rest[end..];
    }
    text.push_str(rest);
    (text, names)
}

/// Structural syntax equality: rules, labels and children; token text for lexical nodes.
fn same_syntax(language: &Language<'_>, tree: &dyn SyntaxAccess<'_>, x: u32, y: u32) -> bool {
    let (p, q) = (tree.nodes()[x as usize], tree.nodes()[y as usize]);
    if p.rule != q.rule {
        return false;
    }
    if language.rules[p.rule as usize].lexical {
        return tree.text(x) == tree.text(y);
    }
    let (left, right) = (tree.children(x), tree.children(y));
    left.len() == right.len()
        && left.iter().zip(right).all(|(l, r)| {
            tree.label(l) == tree.label(r) && same_syntax(language, tree, l.node, r.node)
        })
}

/// Whether a node of `rule` can fill a check's `category`: one `derives` it,
/// or an alternative of the extensible category (its `owner`).
pub(crate) fn stands_for(language: &Language<'_>, category: u16, rule: u16) -> bool {
    derives(language, category, rule, &mut Vec::new())
        || language.rules.get(rule as usize).and_then(|r| r.owner) == Some(category)
}

/// Whether a node of `rule` can stand for `category`: the same rule, or one
/// an alias category refers to, as an alias's one child stands in for it.
fn derives(language: &Language<'_>, category: u16, rule: u16, seen: &mut Vec<u16>) -> bool {
    if category == rule {
        return true;
    }
    let Some(alias) = language.rules.get(category as usize).filter(|r| r.alias) else {
        return false;
    };
    if seen.contains(&category) {
        return false;
    }
    seen.push(category);
    let mut refs = Vec::new();
    references(&alias.expr, &mut refs);
    refs.into_iter().any(|r| derives(language, r, rule, seen))
}

fn references(expr: &Expr<'_>, out: &mut Vec<u16>) {
    match expr {
        Expr::Ref(id) => out.push(*id),
        Expr::Choice(items) | Expr::Sequence(items) => {
            items.iter().for_each(|e| references(e, out));
        }
        Expr::Label(_, inner) => references(inner, out),
        _ => {}
    }
}
