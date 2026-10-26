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

/// Why a rename cannot happen, as an LSP error (invalid params).
pub type Refusal = (i64, String);

fn refuse(message: &str) -> Refusal {
    (crate::server::INVALID_PARAMS, message.to_owned())
}

/// The name range under the cursor and its current text, or `null`.
pub fn prepare_rename(
    analysis: &Analysis,
    offset: u32,
    grammar: Option<GrammarAt<'_>>,
) -> Result<Value, Refusal> {
    if !on_text(analysis, offset) {
        return Ok(Value::Null);
    }
    if let Some(GrammarAt { index, file }) = grammar {
        return Ok(match rule_at(index, file, offset) {
            Some((rule, span)) => {
                // Only the last segment of a qualified use is the rule's name.
                let text = &index.files[file].text[span[0] as usize..span[1] as usize];
                let start = span[0] + text.rfind('.').map_or(0, |d| d as u32 + 1);
                json!({"range": range(&analysis.lines, [start, span[1]]), "placeholder": rule.item.name})
            }
            None => Value::Null,
        });
    }
    let index = &analysis.index;
    match index.at(offset) {
        Some(Target::Builtin(_)) => Err(refuse("built-in names cannot be renamed")),
        Some(Target::Definition(i)) => {
            let span = index
                .occurrences(i)
                .into_iter()
                .map(|(s, _)| s)
                .find(|s| s[0] <= offset && offset <= s[1])
                .unwrap_or(index.definitions[i].name_span);
            Ok(
                json!({"range": range(&analysis.lines, span), "placeholder": index.definitions[i].name}),
            )
        }
        None => Ok(Value::Null),
    }
}

fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A `WorkspaceEdit` renaming every occurrence of the symbol at `offset`.
pub fn rename(
    analysis: &Analysis,
    uri: &str,
    offset: u32,
    grammar: Option<GrammarAt<'_>>,
    new_name: &str,
) -> Result<Value, Refusal> {
    if !identifier(new_name) {
        return Err(refuse("not an identifier"));
    }
    if prepare_rename(
        analysis,
        offset,
        grammar.as_ref().map(|g| GrammarAt {
            index: g.index,
            file: g.file,
        }),
    )?
    .is_null()
    {
        return Ok(Value::Null);
    }
    let mut changes = serde_json::Map::new();
    let mut edit = |uri: String, lines: &crate::lines::LineIndex, span: [u32; 2]| {
        let edits = changes.entry(uri).or_insert_with(|| json!([]));
        edits
            .as_array_mut()
            .expect("edits are an array")
            .push(json!({"range": range(lines, span), "newText": new_name}));
    };
    if let Some(GrammarAt { index, file }) = grammar {
        let Some((rule, _)) = rule_at(index, file, offset) else {
            return Ok(Value::Null);
        };
        let mut spans = vec![(rule.file, rule.item.name_span.map(|b| b as u32))];
        spans.extend(index.uses(&rule));
        for (file, span) in spans {
            let grammar = &index.files[file];
            let text = &grammar.text[span[0] as usize..span[1] as usize];
            let start = span[0] + text.rfind('.').map_or(0, |d| d as u32 + 1);
            let lines = crate::lines::LineIndex::new(&grammar.text);
            edit(
                crate::uri::from_path(&grammar.path),
                &lines,
                [start, span[1]],
            );
        }
    } else if let Some(Target::Definition(i)) = analysis.index.at(offset) {
        for (span, _) in analysis.index.occurrences(i) {
            edit(uri.to_owned(), &analysis.lines, span);
        }
    }
    Ok(json!({ "changes": changes }))
}

/// Whether `query`'s characters appear in `name` in order, ignoring case.
fn matches(name: &str, query: &str) -> bool {
    let mut name = name.chars().flat_map(char::to_lowercase);
    query
        .chars()
        .flat_map(char::to_lowercase)
        .all(|q| name.any(|n| n == q))
}

const MAX_WORKSPACE_SYMBOLS: usize = 500;

/// Grammar rules of every workspace language, then the definitions of open
/// documents, whose names match `query`.
pub fn workspace_symbols<'a>(
    grammars: &GrammarIndex,
    open: impl Iterator<Item = (&'a str, &'a Analysis)>,
    query: &str,
) -> Value {
    let mut found = Vec::new();
    for rule in grammars
        .definitions()
        .iter()
        .filter(|r| matches(&r.item.name, query))
    {
        let kind = match rule.item.kind {
            "token" => 14,
            "function" | "helper" => 12,
            _ => 5,
        };
        found.push(json!({
            "name": rule.item.name,
            "kind": kind,
            "location": grammar_location(grammars, rule.file, rule.item.name_span.map(|b| b as u32)),
            "containerName": rule.module,
        }));
    }
    for (uri, analysis) in open {
        for definition in analysis
            .index
            .definitions
            .iter()
            .filter(|d| matches(&d.name, query))
        {
            found.push(json!({
                "name": definition.name,
                "kind": if definition.kind == "function" { 12 } else { 13 },
                "location": {"uri": uri, "range": range(&analysis.lines, definition.name_span)},
            }));
        }
    }
    found.truncate(MAX_WORKSPACE_SYMBOLS);
    Value::Array(found)
}
