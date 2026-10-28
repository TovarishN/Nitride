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

#[test]
fn grammar_with_syntax_error_still_navigates() {
    let dir = scratch("broken-grammar-nav");
    std::fs::copy(lang("portable/Portable.ngr"), dir.join("Portable.ngr")).unwrap();
    let path = dir.join("Portable.ngr");
    let text = std::fs::read_to_string(&path).unwrap() + "\nsyntax ???\n";
    let line = text
        .lines()
        .position(|l| l.contains("extensible syntax Expr"))
        .unwrap() as u32;
    let result = hover(&dir, &path, &text, line, 22);
    assert!(
        result["contents"]["value"]
            .as_str()
            .unwrap_or_default()
            .contains("Expr"),
        "{result}"
    );
}

#[test]
fn locations_use_canonical_paths() {
    let dir = scratch("canonical-paths");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::create_dir_all(dir.join("g")).unwrap();
    std::fs::copy(lang("portable/Portable.ngr"), dir.join("g/Portable.ngr")).unwrap();
    manifest(
        &dir,
        "nitride.json",
        "Portable",
        ".portable",
        &[dir.join("sub/../g/Portable.ngr")],
        "Portable.Program",
        Some(&lang("portable/catalog.json")),
    );
    let path = dir.join("g/Portable.ngr");
    let text = std::fs::read_to_string(&path).unwrap();
    let line = text.lines().position(|l| l.contains("Value:Expr")).unwrap() as u32;
    let character = text
        .lines()
        .nth(line as usize)
        .unwrap()
        .find("Value:Expr")
        .unwrap() as u32
        + 7;
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
    assert_eq!(result["uri"], uri(&path), "{result}");
}

#[test]
fn syntax_only_call_heads_and_kinds() {
    let dir = scratch("syntax-kinds");
    manifest(
        &dir,
        "nitrogen.json",
        "Calls",
        ".calls",
        &[lang("calls/Calls.ngr")],
        "Calls.Program",
        None,
    );
    let path = dir.join("x.calls");
    let text = "function f = 1;\nf(2);\nlet g = 3;\nfunction g = 4;\ng(g);";
    let definition = at(
        &dir,
        &path,
        text,
        "textDocument/definition",
        1,
        0,
        json!({}),
    )["result"]
        .clone();
    assert_eq!(
        definition["range"]["start"],
        json!({"line":0,"character":9}),
        "call head f"
    );
    let references = at(
        &dir,
        &path,
        text,
        "textDocument/references",
        2,
        4,
        json!({"context":{"includeDeclaration":true}}),
    )["result"]
        .clone();
    let lines: Vec<_> = references
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["range"]["start"]["line"].as_u64().unwrap())
        .collect();
    assert!(
        !lines.contains(&3),
        "value g must not include function g's declaration: {lines:?}"
    );
}

#[test]
fn empty_name_is_not_a_definition() {
    let dir = calls("empty-name");
    let result = hover(&dir, &dir.join("x.calls"), "let =1;", 0, 4);
    assert!(!result.to_string().contains("``"), "{result}");
}

pub fn rewrites(name: &str) -> PathBuf {
    let dir = scratch(name);
    manifest(
        &dir,
        "nitride.json",
        "Rewrites",
        ".rewrites",
        &[lang("dates/Dates.ngr"), lang("rewrites/Rewrites.ngr")],
        "Dates.Program",
        Some(&lang("dates/catalog.json")),
    );
    dir
}

#[test]
fn quote_placeholders_navigate_to_their_binders() {
    let dir = rewrites("quote-placeholders");
    let path = lang("rewrites/Rewrites.ngr");
    let text = std::fs::read_to_string(&path).unwrap();
    // (line, character) of the first `needle` in the first line containing `line`.
    let find = |line: &str, needle: &str| {
        let n = text.lines().position(|l| l.contains(line)).unwrap();
        let c = text.lines().nth(n).unwrap().find(needle).unwrap();
        (n as u32, c as u32)
    };
    for (use_line, use_text, def_line, def_text) in [
        (
            "$Count * 14",
            "Count *",
            "FortnightDuration(Count:",
            "Count:",
        ),
        ("_ => <[ $Value", "Value ]", "SwapAddition(Value:", "Value:"),
        (
            "<[ $Right + $Left ]>",
            "Right + $Left ]",
            "Right: Expr)",
            "Right:",
        ),
        ("| Soon", "Amount days", "| Soon", "Amount:"),
        // Clicking the `$` of a placeholder.
        ("| Soon", "$Amount days", "| Soon", "Amount:"),
    ] {
        let (line, character) = find(use_line, use_text);
        let result = at(
            &dir,
            &path,
            &text,
            "textDocument/definition",
            line,
            character,
            json!({}),
        );
        let (line, start) = find(def_line, def_text);
        let end = start + def_text.len() as u32 - 1;
        assert_eq!(
            result["result"]["range"],
            location(line, start, end),
            "{use_text}"
        );
    }
    let (line, character) = find("| Twice", "Inner:");
    let result = at(
        &dir,
        &path,
        &text,
        "textDocument/references",
        line,
        character,
        json!({"context":{"includeDeclaration":true}}),
    );
    assert_eq!(result["result"].as_array().unwrap().len(), 3);
    let result = hover(&dir, &path, &text, line, character);
    assert_eq!(
        result["contents"]["value"],
        "```ngr\nInner:Expr\n```\n*field*"
    );
}
