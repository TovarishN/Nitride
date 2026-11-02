//! Rule checks: diagnostics a syntax rule declares over its own nodes.
use crate::quote_match::{
    Budget, Flavor, LimitExceeded, MAX_WORK_STEPS, Patterns, Quote, stands_for,
};
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
    rule_diagnostics_with(language, tree, MAX_WORK_STEPS)
}

/// [`rule_diagnostics`] with `steps` work steps per check and node. A check
/// that runs out reports Nitrogen's `NS0002` failure on the node instead.
pub fn rule_diagnostics_with(
    language: &Language<'_>,
    tree: &SyntaxTree<'_>,
    steps: u32,
) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    // `validate_rule_checks` reports patterns that do not compile when the
    // language loads; here their checks are skipped.
    let patterns = Patterns::compile(language).ok();
    let mut ancestry: Option<Ancestry> = None;
    for check in language.rule_checks {
        let (quote, context) = match &check.test {
            RuleTest::Match {
                captures,
                pattern,
                context,
            } => {
                let quote = Quote::compile(language, check.rule, captures, pattern, Flavor::Check);
                let context = context.as_ref().map(|c| {
                    Quote::compile(language, c.category, c.captures, c.pattern, Flavor::Check)
                        .map(|quote| (quote, c))
                });
                match (&patterns, quote, context.transpose()) {
                    (Some(_), Ok(quote), Ok(context)) => (Some(quote), context),
                    _ => continue,
                }
            }
            RuleTest::Compare { .. } => (None, None),
        };
        if context.is_some() && ancestry.is_none() {
            ancestry = Some(Ancestry::new(tree));
        }
        for id in 0..tree.nodes.len() as u32 {
            if tree.nodes[id as usize].rule != check.rule {
                continue;
            }
            let text = |field: &str| tree.field(id, field).and_then(|n| tree.text(n));
            let bindings = match (&check.test, &quote, &patterns) {
                (RuleTest::Compare { left, right, equal }, ..) => {
                    let (Some(left), Some(right)) = (text(left), text(right)) else {
                        continue;
                    };
                    if (left == right) == *equal {
                        continue;
                    }
                    Vec::new()
                }
                (RuleTest::Match { .. }, Some(quote), Some(patterns)) => {
                    let mut budget = Budget::new(steps);
                    let context = context.as_ref().zip(ancestry.as_ref());
                    match matched(language, patterns, tree, id, quote, context, &mut budget) {
                        Ok(Some(bindings)) => bindings,
                        Ok(None) => continue,
                        Err(LimitExceeded) => {
                            found.push(failure(check, tree.nodes[id as usize].span));
                            continue;
                        }
                    }
                }
                _ => continue,
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

/// Each node's parent, and whether its subtree holds recovered syntax.
struct Ancestry {
    parents: Vec<Option<u32>>,
    broken: Vec<bool>,
}

impl Ancestry {
    fn new(tree: &SyntaxTree<'_>) -> Self {
        let mut parents = vec![None; tree.nodes.len()];
        for id in 0..tree.nodes.len() as u32 {
            for edge in tree.children(id) {
                parents[edge.node as usize] = Some(id);
            }
        }
        let mut broken = vec![false; tree.nodes.len()];
        for &damaged in tree.recovery().map_or(&[][..], |r| &r.damaged_nodes) {
            if let Some(flag) = broken.get_mut(damaged as usize) {
                *flag = true;
            }
        }
        // Postorder ids: every child comes before its parent.
        for id in 0..tree.nodes.len() {
            if broken[id]
                && let Some(parent) = parents[id]
            {
                broken[parent as usize] = true;
            }
        }
        Self { parents, broken }
    }
}

/// The captures of a match of `quote` at `id`, or none. With a context, an
/// ancestor (the nearest first; only the parent for `parent`) must also
/// match it, and the nearest that does adds its captures. Ancestors of
/// another category, or whose syntax was recovered, are passed over.
fn matched<'a>(
    language: &Language<'_>,
    patterns: &Patterns<'a>,
    tree: &SyntaxTree<'_>,
    id: u32,
    quote: &Quote<'a>,
    context: Option<(&(Quote<'a>, &MatchContext<'_>), &Ancestry)>,
    budget: &mut Budget,
) -> Result<Option<Vec<(&'a str, u32)>>, LimitExceeded> {
    let mut bindings = Vec::new();
    if !quote.matches(patterns, tree, id, budget, &mut bindings)? {
        return Ok(None);
    }
    let Some(((quote, context), ancestry)) = context else {
        return Ok(Some(bindings));
    };
    let mut ancestor = ancestry.parents[id as usize];
    while let Some(at) = ancestor {
        budget.step()?;
        let rule = tree.nodes[at as usize].rule;
        if stands_for(language, context.category, rule)
            && !ancestry.broken[at as usize]
            && quote.matches(patterns, tree, at, budget, &mut bindings)?
        {
            return Ok(Some(bindings));
        }
        if context.parent_only {
            break;
        }
        ancestor = ancestry.parents[at as usize];
    }
    Ok(None)
}

/// A check that ran out of work, reported as Nitrogen reports it: `NS0002`
/// on the checked node, an error.
fn failure(check: &RuleCheck<'_>, span: [u32; 2]) -> Diagnostic {
    Diagnostic {
        category: "check",
        span,
        message: format!(
            "check {} failed: InvalidOperationException: NQP0006: The syntax-pattern match exceeded its work limit.",
            check.code
        ),
        code: Some("NS0002".into()),
        severity: Severity::Error,
    }
}

/// Each named pattern and rule check's pattern parses as its category, or
/// the first that does not.
pub fn validate_rule_checks(language: &Language<'_>) -> Result<(), String> {
    Patterns::compile(language).map_err(|e| e.message)?;
    for check in language.rule_checks {
        if let RuleTest::Match {
            captures,
            pattern,
            context,
        } = &check.test
        {
            Quote::compile(language, check.rule, captures, pattern, Flavor::Check).map_err(
                |e| {
                    format!(
                        "the pattern of check {} does not parse as {}: {}",
                        check.code, language.rules[check.rule as usize].name, e.message
                    )
                },
            )?;
            if let Some(context) = context {
                Quote::compile(
                    language,
                    context.category,
                    context.captures,
                    context.pattern,
                    Flavor::Check,
                )
                .map_err(|e| {
                    format!(
                        "the context of check {} does not parse as {}: {}",
                        check.code, language.rules[context.category as usize].name, e.message
                    )
                })?;
            }
        }
    }
    Ok(())
}
