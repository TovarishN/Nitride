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

/// A `Location` in a grammar file of the index.
fn grammar_location(index: &GrammarIndex, file: usize, span: [u32; 2]) -> Value {
    let grammar = &index.files[file];
    let lines = crate::lines::LineIndex::new(&grammar.text);
    json!({"uri": crate::uri::from_path(&grammar.path), "range": range(&lines, span)})
}

/// Every occurrence of the symbol at `offset` as (uri, span, is declaration),
/// or `None` when there is no renameable symbol there.
fn occurrences(
    analysis: &Analysis,
    uri: &str,
    offset: u32,
    grammar: Option<&GrammarAt<'_>>,
) -> Option<Vec<(Value, bool)>> {
    if let Some(GrammarAt { index, file }) = grammar {
        let (rule, _) = rule_at(index, *file, offset)?;
        let mut found = vec![(
            grammar_location(index, rule.file, rule.item.name_span.map(|b| b as u32)),
            true,
        )];
        found.extend(
            index
                .uses(&rule)
                .into_iter()
                .map(|(file, span)| (grammar_location(index, file, span), false)),
        );
        return Some(found);
    }
    let Some(Target::Definition(definition)) = analysis.index.at(offset) else {
        return None;
    };
    Some(
        analysis
            .index
            .occurrences(definition)
            .into_iter()
            .map(|(span, declaration)| {
                (
                    json!({"uri": uri, "range": range(&analysis.lines, span)}),
                    declaration,
                )
            })
            .collect(),
    )
}

pub fn definition(
    analysis: &Analysis,
    uri: &str,
    offset: u32,
    grammar: Option<GrammarAt<'_>>,
) -> Value {
    if !on_text(analysis, offset) {
        return Value::Null;
    }
    occurrences(analysis, uri, offset, grammar.as_ref())
        .and_then(|all| all.into_iter().find(|(_, declaration)| *declaration))
        .map_or(Value::Null, |(location, _)| location)
}

pub fn references(
    analysis: &Analysis,
    uri: &str,
    offset: u32,
    grammar: Option<GrammarAt<'_>>,
    include_declaration: bool,
) -> Value {
    if !on_text(analysis, offset) {
        return json!([]);
    }
    let mut found: Vec<Value> = occurrences(analysis, uri, offset, grammar.as_ref())
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, declaration)| include_declaration || !declaration)
        .map(|(location, _)| location)
        .collect();
    found.sort_by_key(|l| {
        let start = &l["range"]["start"];
        (
            l["uri"].as_str().unwrap_or_default().to_owned(),
            start["line"].as_u64(),
            start["character"].as_u64(),
        )
    });
    Value::Array(found)
}

/// Occurrences in this document: the declaration is a write (3), uses reads (2).
pub fn highlights(
    analysis: &Analysis,
    uri: &str,
    offset: u32,
    grammar: Option<GrammarAt<'_>>,
) -> Value {
    if !on_text(analysis, offset) {
        return json!([]);
    }
    let this = grammar
        .as_ref()
        .map(|g| crate::uri::from_path(&g.index.files[g.file].path));
    let mut found: Vec<Value> = occurrences(analysis, uri, offset, grammar.as_ref())
        .unwrap_or_default()
        .into_iter()
        .filter(|(l, _)| this.as_deref().is_none_or(|this| l["uri"] == this))
        .map(|(l, declaration)| json!({"range": l["range"], "kind": if declaration { 3 } else { 2 }}))
        .collect();
    found.sort_by_key(|h| {
        (
            h["range"]["start"]["line"].as_u64(),
            h["range"]["start"]["character"].as_u64(),
        )
    });
    Value::Array(found)
}
