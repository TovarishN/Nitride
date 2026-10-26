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
