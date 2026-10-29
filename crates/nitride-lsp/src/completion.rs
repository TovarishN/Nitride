//! Completion: names visible before the cursor, built-ins, grammar rules,
//! and the keywords the grammar accepts there.
use crate::analysis::Analysis;
use crate::navigation::GrammarAt;
use nitride_core::*;
use serde_json::{Value, json};

const VARIABLE: u32 = 6;
const FUNCTION: u32 = 3;
const CLASS: u32 = 7;
const CONSTANT: u32 = 21;
const KEYWORD: u32 = 14;

/// The start of the word (`[A-Za-z0-9_.]*`) that ends at `offset`.
fn word_start(text: &str, offset: usize) -> usize {
    let bytes = text.as_bytes();
    let mut start = offset.min(text.len());
    while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric() || matches!(bytes[start - 1], b'_' | b'.'))
    {
        start -= 1;
    }
    start
}

fn is_word(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Every word literal of the grammar. Recovery prefers deleting an
/// unfinished last statement to completing it, so the words a grammar would
/// accept at the cursor cannot be read from it; all are offered.
fn keywords(language: &Language<'_>) -> Vec<String> {
    fn walk(expr: &Expr<'_>, words: &mut Vec<String>) {
        match expr {
            Expr::Literal(text) if is_word(text) => words.push((*text).to_owned()),
            Expr::Sequence(items) | Expr::Choice(items) => {
                items.iter().for_each(|e| walk(e, words))
            }
            Expr::Label(_, inner) | Expr::Predicate { inner, .. } | Expr::Repeat { inner, .. } => {
                walk(inner, words)
            }
            _ => {}
        }
    }
    let mut words = Vec::new();
    for rule in language.rules.iter().filter(|r| !r.lexical) {
        walk(&rule.expr, &mut words);
    }
    words.sort();
    words.dedup();
    words
}

fn item(label: &str, kind: u32, detail: Option<&str>) -> Value {
    let mut item = json!({"label": label, "kind": kind});
    if let Some(detail) = detail {
        item["detail"] = json!(detail);
    }
    item
}

pub fn items(analysis: &Analysis, offset: u32, grammar: Option<GrammarAt<'_>>) -> Value {
    // Nothing completes inside a Nitrogen header.
    if analysis
        .header
        .iter()
        .any(|h| h[0] <= offset && offset <= h[1])
    {
        return Value::Array(vec![]);
    }
    let text = analysis.lines.text();
    let start = word_start(text, offset as usize);
    let typed = text[start..(offset as usize).min(text.len())].to_lowercase();
    let mut names: Vec<Value> = Vec::new();
    if let Some(GrammarAt { index, file }) = grammar {
        // Grammars name rules; catalogs also name their own members.
        let catalog = crate::grammar_index::is_catalog(&index.files[file].path);
        for rule in index.definitions().iter().filter(|d| {
            crate::grammar_index::DEFINING.contains(&d.item.kind)
                || catalog && matches!(d.item.kind, "type" | "operation" | "overload")
        }) {
            let kind = match rule.item.kind {
                "token" => CONSTANT,
                "function" | "helper" | "operation" | "overload" => FUNCTION,
                _ => CLASS,
            };
            names.push(item(&rule.item.name, kind, Some(&rule.module)));
        }
    } else {
        for definition in analysis
            .index
            .definitions
            .iter()
            .filter(|d| (d.name_span[0] as usize) < start)
        {
            let kind = if definition.kind == "function" {
                FUNCTION
            } else {
                VARIABLE
            };
            names.push(item(&definition.name, kind, definition.ty.as_deref()));
        }
        if let Some(tree) = &analysis.tree {
            for rule in tree.grammar.language().rules {
                match rule.action {
                    Action::Call { builtins, .. } => {
                        for builtin in builtins {
                            names.push(item(builtin.name, FUNCTION, Some("built-in")));
                        }
                    }
                    Action::Reference { values, .. } => {
                        for value in values {
                            names.push(item(value.name, VARIABLE, Some("built-in")));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut keywords: Vec<Value> = analysis
        .tree
        .as_ref()
        .map(|tree| keywords(tree.grammar.language()))
        .unwrap_or_default()
        .iter()
        .map(|word| item(word, KEYWORD, None))
        .collect();
    let label = |v: &Value| v["label"].as_str().unwrap_or_default().to_owned();
    let matching = |v: &Value| label(v).to_lowercase().starts_with(&typed);
    names.retain(matching);
    keywords.retain(matching);
    names.sort_by_key(label);
    keywords.sort_by_key(label);
    let mut seen = std::collections::HashSet::new();
    let all: Vec<Value> = names
        .into_iter()
        .chain(keywords)
        .filter(|v| seen.insert(label(v)))
        .collect();
    Value::Array(all)
}
