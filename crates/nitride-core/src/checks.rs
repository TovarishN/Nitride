//! Rule checks: diagnostics a syntax rule declares over its own nodes.
use crate::quote_match::{Budget, Flavor, MAX_WORK_STEPS, Patterns, Quote};
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
    for check in language.rule_checks {
        let quote = match &check.test {
            RuleTest::Match {
                captures, pattern, ..
            } => match (
                &patterns,
                Quote::compile(language, check.rule, captures, pattern, Flavor::Check),
            ) {
                (Some(_), Ok(quote)) => Some(quote),
                _ => continue,
            },
            RuleTest::Compare { .. } => None,
        };
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
                    let mut bindings = Vec::new();
                    let mut budget = Budget::new(steps);
                    match quote.matches(patterns, tree, id, &mut budget, &mut bindings) {
                        Ok(true) => bindings,
                        Ok(false) => continue,
                        Err(_) => {
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
            captures, pattern, ..
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
        }
    }
    Ok(())
}
