//! Code actions. Quick fixes come from the parser's own recovery repairs
//! and are offered only when the repaired text parses with fewer errors;
//! grammars can drop the C# semantic members Nitride skips.
use crate::analysis::{Analysis, Fix};
use crate::server::range;
use nitride_core::*;
use serde_json::{Value, json};

pub const SKIPPED_MEMBER: &str = "C# semantic member is not admitted";

/// Whether `after` parses with fewer recovery diagnostics than `before`.
pub fn helps(language: &Language<'_>, before: &str, after: &str) -> bool {
    let errors = |text: &str| {
        parse_recovering(
            language,
            text,
            Limits::default(),
            RecoveryOptions::default(),
        )
        .diagnostics
        .len()
    };
    errors(after) < errors(before)
}

fn edit(analysis: &Analysis, uri: &str, edits: &[([u32; 2], String)]) -> Value {
    let edits: Vec<Value> = edits
        .iter()
        .map(|(span, text)| json!({"range": range(&analysis.lines, *span), "newText": text}))
        .collect();
    json!({"changes": {uri: edits}})
}

/// The span to delete for a skipped member: with the indentation before it
/// and, when only whitespace follows, the rest of its line and line ending.
fn member_span(text: &str, span: [u32; 2]) -> [u32; 2] {
    let [mut start, mut end] = span.map(|b| (b as usize).min(text.len()));
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    if text[line_start..start].trim().is_empty() {
        start = line_start;
    }
    let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
    if text[end..line_end].trim().is_empty() {
        end = line_end;
    }
    [start as u32, end as u32]
}

fn wanted(only: &[String], kind: &str) -> bool {
    only.is_empty()
        || only
            .iter()
            .any(|o| kind == o || kind.starts_with(&format!("{o}.")))
}

pub fn actions(analysis: &Analysis, uri: &str, start: u32, end: u32, only: &[String]) -> Value {
    let text = analysis.lines.text();
    let mut found = Vec::new();
    if wanted(only, "quickfix") {
        if let Some(tree) = analysis
            .tree
            .as_ref()
            .filter(|_| !analysis.repairs.is_empty())
        {
            let language = tree.grammar.language();
            for fix in &analysis.repairs {
                let (span, new, title) = match fix {
                    Fix::Insert { at, text: inserted } => {
                        // Insert after the previous token, not after the line break.
                        let at = text[..(*at as usize).min(text.len())].trim_end().len() as u32;
                        ([at, at], inserted.clone(), format!("Insert `{inserted}`"))
                    }
                    Fix::Delete { span } => {
                        let removed: String = text
                            .get(span[0] as usize..span[1] as usize)
                            .unwrap_or_default()
                            .chars()
                            .take(20)
                            .collect();
                        (*span, String::new(), format!("Remove `{removed}`"))
                    }
                };
                if span[1] < start || span[0] > end {
                    continue;
                }
                let on_boundaries = span[0] <= span[1]
                    && text.is_char_boundary(span[0] as usize)
                    && text.is_char_boundary(span[1] as usize);
                if !on_boundaries {
                    continue;
                }
                let mut after = text.to_owned();
                after.replace_range(span[0] as usize..span[1] as usize, &new);
                if helps(language, text, &after) {
                    found.push(json!({"title": title, "kind": "quickfix",
                        "edit": edit(analysis, uri, &[(span, new)])}));
                }
            }
            if found.len() == 1 {
                found[0]["isPreferred"] = json!(true);
            }
        }
        for problem in analysis
            .problems
            .iter()
            .filter(|p| p.message == SKIPPED_MEMBER)
        {
            if problem.span[1] < start || problem.span[0] > end {
                continue;
            }
            let span = member_span(text, problem.span);
            found.push(
                json!({"title": "Remove C# semantic member", "kind": "quickfix",
                "edit": edit(analysis, uri, &[(span, String::new())])}),
            );
        }
    }
    if wanted(only, "source.fixAll") {
        let all: Vec<([u32; 2], String)> = analysis
            .problems
            .iter()
            .filter(|p| p.message == SKIPPED_MEMBER)
            .map(|p| (member_span(text, p.span), String::new()))
            .collect();
        if !all.is_empty() {
            found.push(
                json!({"title": "Remove all C# semantic members", "kind": "source.fixAll",
                "edit": edit(analysis, uri, &all)}),
            );
        }
    }
    Value::Array(found)
}
