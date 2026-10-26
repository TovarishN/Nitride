mod support;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

pub fn lang(path: &str) -> PathBuf {
    repo().join("languages").join(path)
}

pub fn calls(name: &str) -> PathBuf {
    let dir = scratch(name);
    manifest(
        &dir,
        "nitride.json",
        "Calls",
        ".calls",
        &[lang("calls/Calls.ngr")],
        "Calls.Program",
        Some(&lang("calls/catalog.json")),
    );
    dir
}

pub fn power(name: &str) -> PathBuf {
    let dir = scratch(name);
    manifest(
        &dir,
        "nitride.json",
        "PortablePower",
        ".powercalc",
        &[lang("portable/Portable.ngr"), lang("power/Power.ngr")],
        "Portable.Program",
        Some(&lang("power/catalog.json")),
    );
    dir
}

/// Opens `path` with `text` and sends one request at (line, character).
pub fn at(
    root: &Path,
    path: &Path,
    text: &str,
    method: &str,
    line: u32,
    character: u32,
    extra: Value,
) -> Value {
    let mut params =
        json!({"textDocument":{"uri":uri(path)},"position":{"line":line,"character":character}});
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    let messages = Session::run(root, vec![open(path, text), request(5, method, params)]);
    response(&messages, 5).clone()
}

fn hover(root: &Path, path: &Path, text: &str, line: u32, character: u32) -> Value {
    at(
        root,
        path,
        text,
        "textDocument/hover",
        line,
        character,
        json!({}),
    )["result"]
        .clone()
}

#[test]
fn hover_declaration_type() {
    let dir = calls("hover-declaration");
    let result = hover(&dir, &dir.join("x.calls"), "let a=1;\nsum(a,a);", 0, 4);
    assert_eq!(result["contents"]["value"], "`a` : Core.Scalar\n\n*value*");
    assert_eq!(
        result["range"],
        json!({"start":{"line":0,"character":4},"end":{"line":0,"character":5}})
    );
}

#[test]
fn hover_use_type() {
    let dir = calls("hover-use");
    let result = hover(&dir, &dir.join("x.calls"), "let a=1;\nsum(a,a);", 1, 4);
    assert_eq!(result["contents"]["value"], "`a` : Core.Scalar\n\n*value*");
}

#[test]
fn hover_builtin() {
    let dir = calls("hover-builtin");
    let result = hover(&dir, &dir.join("x.calls"), "sum(1,2);", 0, 1);
    assert_eq!(result["contents"]["value"], "`sum`\n\n*built-in function*");
}

#[test]
fn hover_expression_type() {
    let dir = power("hover-expression");
    let result = hover(&dir, &dir.join("x.powercalc"), "1+2;", 0, 1);
    let value = result["contents"]["value"].as_str().unwrap();
    assert!(
        value.starts_with('`') && value.contains("Scalar"),
        "{value}"
    );
}

#[test]
fn hover_rule_across_files() {
    let dir = power("hover-rule");
    let path = lang("power/Power.ngr");
    let text = std::fs::read_to_string(&path).unwrap();
    let line = text
        .lines()
        .position(|l| l.contains("Portable.Expr"))
        .unwrap() as u32;
    let character = text
        .lines()
        .nth(line as usize)
        .unwrap()
        .find("Portable.Expr")
        .unwrap() as u32
        + 10;
    let result = hover(&dir, &path, &text, line, character);
    let value = result["contents"]["value"].as_str().unwrap();
    assert!(value.contains("extensible syntax Expr"), "{value}");
    assert!(value.contains("`Portable.Expr`"), "{value}");
}

#[test]
fn nothing_under_cursor() {
    let dir = calls("hover-nothing");
    let path = dir.join("x.calls");
    let text = "// note\nlet  a=1;";
    for (line, character) in [(0, 3), (1, 0), (1, 4), (99, 0)] {
        assert_eq!(
            hover(&dir, &path, text, line, character),
            Value::Null,
            "{line}:{character}"
        );
    }
}

fn location(line: u32, start: u32, end: u32) -> Value {
    json!({"start":{"line":line,"character":start},"end":{"line":line,"character":end}})
}

const PROGRAM: &str = "let a=1;\nsum(a,a);";

#[test]
fn definition_from_use() {
    let dir = calls("definition-use");
    let path = dir.join("x.calls");
    for method in ["textDocument/definition", "textDocument/declaration"] {
        let result = at(&dir, &path, PROGRAM, method, 1, 4, json!({}))["result"].clone();
        assert_eq!(
            result,
            json!({"uri": uri(&path), "range": location(0, 4, 5)}),
            "{method}"
        );
    }
}

#[test]
fn references_with_and_without_declaration() {
    let dir = calls("references");
    let path = dir.join("x.calls");
    let count = |include: bool| {
        at(
            &dir,
            &path,
            PROGRAM,
            "textDocument/references",
            0,
            4,
            json!({"context":{"includeDeclaration":include}}),
        )["result"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!((count(true), count(false)), (3, 2));
}

#[test]
fn highlights_kinds() {
    let dir = calls("highlights");
    let path = dir.join("x.calls");
    let result = at(
        &dir,
        &path,
        PROGRAM,
        "textDocument/documentHighlight",
        1,
        6,
        json!({}),
    )["result"]
        .clone();
    assert_eq!(
        result,
        json!([{"range":location(0,4,5),"kind":3},{"range":location(1,4,5),"kind":2},{"range":location(1,6,7),"kind":2}])
    );
}

fn power_grammar_position(needle: &str, shift: u32) -> (PathBuf, String, u32, u32) {
    let path = lang("power/Power.ngr");
    let text = std::fs::read_to_string(&path).unwrap();
    let line = text.lines().position(|l| l.contains(needle)).unwrap();
    let character = text.lines().nth(line).unwrap().find(needle).unwrap() as u32 + shift;
    (path, text, line as u32, character)
}

#[test]
fn rule_definition_across_files() {
    let dir = power("rule-definition");
    let (path, text, line, character) = power_grammar_position("Portable.Expr", 10);
    let result = at(
        &dir,
        &path,
        &text,
        "textDocument/definition",
        line,
        character,
        json!({}),
    )["result"]
        .clone();
    let portable = lang("portable/Portable.ngr");
    assert_eq!(result["uri"], uri(&portable));
    let source = std::fs::read_to_string(&portable).unwrap();
    let expr_line = source
        .lines()
        .position(|l| l.contains("extensible syntax Expr"))
        .unwrap() as u64;
    assert_eq!(result["range"]["start"]["line"], expr_line);
}

#[test]
fn rule_references_across_files() {
    let dir = power("rule-references");
    let (path, text, line, character) = power_grammar_position("Portable.Expr", 10);
    let result = at(
        &dir,
        &path,
        &text,
        "textDocument/references",
        line,
        character,
        json!({"context":{"includeDeclaration":false}}),
    )["result"]
        .clone();
    let uris: Vec<_> = result
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["uri"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        uris.contains(&uri(&path)) && uris.contains(&uri(&lang("portable/Portable.ngr"))),
        "{uris:?}"
    );
}

#[test]
fn definition_of_builtin_is_null() {
    let dir = calls("definition-builtin");
    let result = at(
        &dir,
        &dir.join("x.calls"),
        "sum(1,2);",
        "textDocument/definition",
        0,
        1,
        json!({}),
    );
    assert_eq!(result["result"], Value::Null);
}

fn symbols(root: &Path, opened: Vec<Value>, query: &str) -> Vec<Value> {
    let mut messages = opened;
    messages.push(request(5, "workspace/symbol", json!({"query": query})));
    let messages = Session::run(root, messages);
    response(&messages, 5)["result"].as_array().unwrap().clone()
}

#[test]
fn workspace_symbols_find_rules() {
    let dir = power("symbols-rules");
    let found = symbols(&dir, vec![], "pow");
    let pow = found.iter().find(|s| s["name"] == "Pow").expect("Pow");
    assert_eq!(pow["containerName"], "Portable.Power");
    assert_eq!(pow["location"]["uri"], uri(&lang("power/Power.ngr")));
}

#[test]
fn workspace_symbols_find_open_definitions() {
    let dir = calls("symbols-open");
    let path = dir.join("x.calls");
    let found = symbols(&dir, vec![open(&path, "let total=1;\ntotal;")], "tot");
    let total = found.iter().find(|s| s["name"] == "total").expect("total");
    assert_eq!(total["location"]["uri"], uri(&path));
    assert_eq!(total["kind"], 13);
}
