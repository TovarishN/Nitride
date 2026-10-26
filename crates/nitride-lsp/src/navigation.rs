//! Hover, definitions, references, highlights, rename and workspace symbols
//! over a document's `SymbolIndex`, or a grammar set's `GrammarIndex`.
use crate::analysis::Analysis;
use crate::grammar_index::{GrammarIndex, RuleDef};
use crate::index::Target;
use crate::server::range;
use serde_json::{Value, json};

/// A grammar file within the index of its language's grammars.
pub struct GrammarAt<'a> {
    pub index: &'a GrammarIndex,
    pub file: usize,
}

fn rule_word(kind: &str) -> &'static str {
    match kind {
        "syntax" => "syntax rule",
        "token" => "token",
        "extensible" => "extensible syntax",
        "alternative" => "alternative",
        "function" => "function",
        "helper" => "helper",
        _ => "rule",
    }
}

/// Whether the cursor touches a non-space character (the one after or before it).
fn on_text(analysis: &Analysis, offset: u32) -> bool {
    let text = analysis.lines.text().as_bytes();
    let visible = |i: usize| text.get(i).is_some_and(|b| !b.is_ascii_whitespace());
    visible(offset as usize) || (offset > 0 && visible(offset as usize - 1))
}

pub fn hover(analysis: &Analysis, offset: u32, grammar: Option<GrammarAt<'_>>) -> Value {
    if !on_text(analysis, offset) {
        return Value::Null;
    }
    if let Some(GrammarAt { index, file }) = grammar {
        return match rule_at(index, file, offset) {
            Some((rule, span)) => {
                let text = &index.files[rule.file].text;
                let start = rule.item.span[0];
                let line = text[start..].lines().next().unwrap_or_default().trim();
                let line: String = line.chars().take(200).collect();
                json!({
                    "contents": {"kind": "markdown", "value": format!(
                        "```ngr\n{line}\n```\n*{}* `{}.{}`",
                        rule_word(rule.item.kind), rule.module, rule.item.name)},
                    "range": range(&analysis.lines, span),
                })
            }
            None => Value::Null,
        };
    }
    let index = &analysis.index;
    match index.at(offset) {
        Some(Target::Definition(i)) => {
            let definition = &index.definitions[i];
            let span = index
                .occurrences(i)
                .into_iter()
                .map(|(span, _)| span)
                .find(|s| s[0] <= offset && offset <= s[1])
                .unwrap_or(definition.name_span);
            let head = match &definition.ty {
                Some(ty) => format!("`{}` : {ty}", definition.name),
                None => format!("`{}`", definition.name),
            };
            json!({
                "contents": {"kind": "markdown", "value": format!("{head}\n\n*{}*", definition.kind)},
                "range": range(&analysis.lines, span),
            })
        }
        Some(Target::Builtin(i)) => {
            let builtin = &index.builtins[i];
            json!({
                "contents": {"kind": "markdown",
                    "value": format!("`{}`\n\n*built-in {}*", builtin.name, builtin.kind)},
                "range": range(&analysis.lines, builtin.span),
            })
        }
        None => index
            .types
            .iter()
            .filter(|(s, _)| s[0] <= offset && offset < s[1])
            .min_by_key(|(s, _)| s[1] - s[0])
            .map_or(Value::Null, |(span, ty)| {
                json!({
                    "contents": {"kind": "markdown", "value": format!("`{ty}`")},
                    "range": range(&analysis.lines, *span),
                })
            }),
    }
}

/// The rule at `offset` in a grammar file, and the span of its name there.
pub fn rule_at(index: &GrammarIndex, file: usize, offset: u32) -> Option<(RuleDef, [u32; 2])> {
    let rule = index.at(file, offset)?;
    let inside = |s: [u32; 2]| s[0] <= offset && offset <= s[1];
    let name = rule.item.name_span.map(|b| b as u32);
    let span = if rule.file == file && inside(name) {
        name
    } else {
        index.files[file]
            .references
            .iter()
            .map(|r| r.span.map(|b| b as u32))
            .find(|s| inside(*s))?
    };
    Some((rule, span))
}
