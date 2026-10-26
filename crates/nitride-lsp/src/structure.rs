//! Outline, folding and expand-selection, from declarations and the tree.
use crate::analysis::Analysis;
use crate::server::range;
use nitride_prepare::GrammarItem;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Hierarchical `DocumentSymbol`s: a grammar's modules and members, or a
/// program's declarations nested under the nearest one containing them.
pub fn symbols(analysis: &Analysis) -> Value {
    let lines = &analysis.lines;
    if !analysis.outline.is_empty() {
        return Value::Array(
            analysis
                .outline
                .iter()
                .map(|item| grammar_symbol(item, analysis))
                .collect(),
        );
    }
    let mut declarations: Vec<_> = analysis.declarations.iter().collect();
    declarations.sort_by_key(|d| (d.node_span[0], std::cmp::Reverse(d.node_span[1])));
    // (span, symbol) of the declarations still open around the current one.
    let mut open: Vec<([u32; 2], Value)> = Vec::new();
    let mut roots = Vec::new();
    let close = |open: &mut Vec<([u32; 2], Value)>, roots: &mut Vec<Value>| {
        let (_, symbol) = open.pop().unwrap();
        match open.last_mut() {
            Some((_, parent)) => parent["children"].as_array_mut().unwrap().push(symbol),
            None => roots.push(symbol),
        }
    };
    for declaration in declarations {
        let span = declaration.node_span;
        while open
            .last()
            .is_some_and(|(outer, _)| !(outer[0] <= span[0] && span[1] <= outer[1]))
        {
            close(&mut open, &mut roots);
        }
        let symbol = json!({
            "name": declaration.name,
            "kind": if declaration.kind == "function" { 12 } else { 13 },
            "range": range(lines, span),
            "selectionRange": range(lines, declaration.name_span),
            "children": [],
        });
        open.push((span, symbol));
    }
    while !open.is_empty() {
        close(&mut open, &mut roots);
    }
    Value::Array(roots)
}

fn grammar_symbol(item: &GrammarItem, analysis: &Analysis) -> Value {
    let kind = match item.kind {
        "module" => 3,
        "token" => 14,
        "function" | "helper" => 12,
        _ => 5,
    };
    json!({
        "name": item.name,
        "kind": kind,
        "range": range(&analysis.lines, item.span.map(|b| b as u32)),
        "selectionRange": range(&analysis.lines, item.name_span.map(|b| b as u32)),
        "children": item.children.iter().map(|c| grammar_symbol(c, analysis)).collect::<Vec<_>>(),
    })
}

/// Nodes spanning two or more lines fold (the outermost per start line), as
/// do runs of two or more `//` comment lines and multi-line block comments.
pub fn folds(analysis: &Analysis) -> Value {
    let lines = &analysis.lines;
    let mut by_start = BTreeMap::new();
    if let Some(tree) = &analysis.tree {
        for (id, node) in tree.nodes.iter().enumerate() {
            if id as u32 == tree.root {
                continue;
            }
            let start = lines.position(node.span[0]).0;
            let end = lines.position(node.span[1]).0;
            if end > start {
                let longest = by_start.entry(start).or_insert(end);
                *longest = (*longest).max(end);
            }
        }
    }
    let mut folds: Vec<Value> = by_start
        .into_iter()
        .map(|(start, end)| json!({"startLine": start, "endLine": end}))
        .collect();
    let text = lines.text();
    let mut run: Option<(u32, u32)> = None;
    fn comment(run: &mut Option<(u32, u32)>, folds: &mut Vec<Value>) {
        if let Some((start, end)) = run.take().filter(|(s, e)| e > s) {
            folds.push(json!({"startLine": start, "endLine": end, "kind": "comment"}));
        }
    }
    let mut offset = 0;
    for (line, content) in text.split_inclusive('\n').enumerate() {
        let line = line as u32;
        let trimmed = content.trim_start();
        if trimmed.starts_with("//") {
            run = Some(run.map_or((line, line), |(s, _)| (s, line)));
        } else {
            comment(&mut run, &mut folds);
            if let Some(at) = trimmed.find("/*").filter(|&at| at == 0) {
                let begin = offset + (content.len() - trimmed.len()) + at;
                if let Some(close) = text[begin..].find("*/") {
                    let end = lines.position((begin + close) as u32).0;
                    if end > line {
                        folds.push(json!({"startLine": line, "endLine": end, "kind": "comment"}));
                    }
                }
            }
        }
        offset += content.len();
    }
    comment(&mut run, &mut folds);
    Value::Array(folds)
}

/// Ranges in one selection chain at most.
const MAX_SELECTION_DEPTH: usize = 64;

/// For each position, the chain of enclosing node ranges, innermost first,
/// ending with the whole document.
pub fn selections(analysis: &Analysis, positions: &[Value]) -> Value {
    let lines = &analysis.lines;
    let whole = [0, lines.text().len() as u32];
    let mut results = Vec::new();
    for position in positions {
        let offset = lines.offset(
            position["line"].as_u64().unwrap_or(0) as u32,
            position["character"].as_u64().unwrap_or(0) as u32,
        );
        let mut spans: Vec<[u32; 2]> = analysis
            .tree
            .iter()
            .flat_map(|t| t.nodes.iter().map(|n| n.span))
            .filter(|s| s[0] <= offset && offset <= s[1] && s[0] < s[1])
            .collect();
        spans.sort_by_key(|s| (s[1] - s[0], std::cmp::Reverse(s[0])));
        spans.dedup();
        // Deep nesting would make a deeply nested JSON value (slow to build,
        // recursive to write and drop): keep the innermost levels, then the document.
        spans.truncate(MAX_SELECTION_DEPTH - 1);
        if spans.last() != Some(&whole) {
            spans.push(whole);
        }
        // Innermost last, so each range nests in the one built before it.
        let mut selection = Value::Null;
        for span in spans.iter().rev() {
            let mut range_value = json!({ "range": range(lines, *span) });
            if !selection.is_null() {
                range_value["parent"] = selection; // a move: no deep copy per level
            }
            selection = range_value;
        }
        results.push(selection);
    }
    Value::Array(results)
}
