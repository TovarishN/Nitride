//! Rule checks: diagnostics a syntax rule declares over its own nodes.
use crate::*;

/// The `{Name}` placeholders of a check message, in order.
pub fn placeholders(message: &str) -> impl Iterator<Item = &str> {
    message
        .split('{')
        .skip(1)
        .filter_map(|part| part.split_once('}').map(|(name, _)| name.trim()))
}

/// The diagnostics of `language`'s rule checks over `tree`.
pub fn rule_diagnostics(language: &Language<'_>, tree: &SyntaxTree<'_>) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    for check in language.rule_checks {
        let (text, names) = match &check.test {
            RuleTest::Match { pattern, .. } => spreads(pattern),
            RuleTest::Compare { .. } => Default::default(),
        };
        let holes = spread_holes(language, &names);
        let pattern = match &check.test {
            RuleTest::Match { captures, .. } => {
                match Pattern::parse(language, check.rule, captures, &text, &names, &holes) {
                    Ok(pattern) => Some(pattern),
                    // `validate_rule_checks` reports this when the language loads.
                    Err(_) => continue,
                }
            }
            RuleTest::Compare { .. } => None,
        };
        for id in 0..tree.nodes.len() as u32 {
            if tree.nodes[id as usize].rule != check.rule {
                continue;
            }
            let text = |field: &str| tree.field(id, field).and_then(|n| tree.text(n));
            let bindings = match (&check.test, &pattern) {
                (RuleTest::Compare { left, right, equal }, _) => {
                    let (Some(left), Some(right)) = (text(left), text(right)) else {
                        continue;
                    };
                    if (left == right) == *equal {
                        continue;
                    }
                    Vec::new()
                }
                (RuleTest::Match { .. }, Some(pattern)) => {
                    let mut bindings = Vec::new();
                    if !pattern.node(pattern.tree.root, tree, id, &mut bindings) {
                        continue;
                    }
                    bindings
                }
                (RuleTest::Match { .. }, None) => continue,
            };
            let mut message = String::new();
            let mut rest = check.message;
            while let Some((before, after)) = rest.split_once('{') {
                let Some((name, tail)) = after.split_once('}') else {
                    break;
                };
                message.push_str(before);
                let name = name.trim();
                let value = match name.strip_suffix(".Text") {
                    Some(field) => text(field),
                    None => bindings
                        .iter()
                        .find(|(n, _)| *n == name)
                        .and_then(|(_, node)| tree.text(*node)),
                };
                message.push_str(value.unwrap_or_default());
                rest = tail;
            }
            message.push_str(rest);
            let span = check
                .at
                .and_then(|field| tree.field(id, field))
                .unwrap_or(id);
            found.push(Diagnostic {
                category: "check",
                span: tree.nodes[span as usize].span,
                message,
                code: Some(check.code.into()),
                severity: check.severity,
            });
        }
    }
    found
}

/// Each rule check's pattern parses as its rule, or the first that does not.
pub fn validate_rule_checks(language: &Language<'_>) -> Result<(), String> {
    for check in language.rule_checks {
        if let RuleTest::Match { captures, pattern } = &check.test {
            let (text, names) = spreads(pattern);
            let holes = spread_holes(language, &names);
            Pattern::parse(language, check.rule, captures, &text, &names, &holes).map_err(|e| {
                format!(
                    "the pattern of check {} does not parse as {}: {}",
                    check.code, language.rules[check.rule as usize].name, e.message
                )
            })?;
        }
    }
    Ok(())
}

struct Pattern<'a> {
    language: &'a Language<'a>,
    tree: SyntaxTree<'a>,
    /// Each hole node: its name, category and whether it is a `..` spread.
    holes: Vec<(u32, &'a str, u16, bool)>,
}

/// A pattern's text with each `..$name` spread written `$name`, and the spread names.
fn spreads(pattern: &str) -> (String, Vec<&str>) {
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

/// A spread is a hole of every syntax category.
fn spread_holes<'n>(language: &Language<'_>, names: &[&'n str]) -> Vec<Hole<'n>> {
    names
        .iter()
        .flat_map(|&name| {
            language
                .rules
                .iter()
                .enumerate()
                .filter(|(_, r)| !r.lexical)
                .map(move |(category, _)| Hole {
                    name,
                    category: category as u16,
                })
        })
        .collect()
}

impl<'a> Pattern<'a> {
    fn parse(
        language: &'a Language<'a>,
        rule: u16,
        captures: &'a [Hole<'a>],
        text: &str,
        spreads: &[&'a str],
        spread_holes: &'a [Hole<'a>],
    ) -> Result<Self, Diagnostic> {
        let holes: Vec<&Hole<'_>> = captures.iter().chain(spread_holes).collect();
        let (tree, found) = parse_template(language, text, rule, &holes, Limits::default())?;
        if tree.nodes[tree.root as usize].rule != rule {
            return Err(Diagnostic::new(
                "check",
                [0, 0],
                "the pattern is another rule",
            ));
        }
        let holes = found
            .into_iter()
            .map(|(id, hole)| (id, hole.name, hole.category, spreads.contains(&hole.name)))
            .collect();
        Ok(Self {
            language,
            tree,
            holes,
        })
    }

    fn hole(&self, id: u32) -> Option<&(u32, &'a str, u16, bool)> {
        self.holes.iter().find(|h| h.0 == id)
    }

    /// Whether pattern node `p` matches node `t` of `tree`, binding captures.
    fn node<'t>(
        &self,
        p: u32,
        tree: &'t SyntaxTree<'_>,
        t: u32,
        bindings: &mut Vec<(&'a str, u32)>,
    ) -> bool {
        let target = tree.nodes[t as usize];
        if let Some(&(_, name, category, spread)) = self.hole(p) {
            if spread {
                return true;
            }
            if !derives(self.language, category, target.rule, &mut Vec::new()) {
                return false;
            }
            if let Some(&(_, bound)) = bindings.iter().find(|(n, _)| *n == name) {
                return tree.text(bound) == tree.text(t);
            }
            bindings.push((name, t));
            return true;
        }
        let node = self.tree.nodes[p as usize];
        if node.rule != target.rule {
            return false;
        }
        let (ps, ts) = (self.tree.children(p), tree.children(t));
        if ps.is_empty() && ts.is_empty() {
            let spelling = |text: Option<&str>| {
                text.unwrap_or_default()
                    .split_whitespace()
                    .collect::<String>()
            };
            return spelling(self.tree.text(p)) == spelling(tree.text(t));
        }
        self.list(ps, tree, ts, bindings)
    }

    fn list(
        &self,
        ps: &[Edge],
        tree: &SyntaxTree<'_>,
        ts: &[Edge],
        bindings: &mut Vec<(&'a str, u32)>,
    ) -> bool {
        let Some((first, rest)) = ps.split_first() else {
            return ts.is_empty();
        };
        let label = self.tree.label(first);
        if self.hole(first.node).is_some_and(|h| h.3) {
            // A spread takes any run of siblings under its own field label.
            let run = ts.iter().take_while(|e| tree.label(e) == label).count();
            return (0..=run).any(|taken| {
                let kept = bindings.len();
                let matched = self.list(rest, tree, &ts[taken..], bindings);
                if !matched {
                    bindings.truncate(kept);
                }
                matched
            });
        }
        let Some((next, more)) = ts.split_first() else {
            return false;
        };
        let kept = bindings.len();
        let matched = tree.label(next) == label
            && self.node(first.node, tree, next.node, bindings)
            && self.list(rest, tree, more, bindings);
        if !matched {
            bindings.truncate(kept);
        }
        matched
    }
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
