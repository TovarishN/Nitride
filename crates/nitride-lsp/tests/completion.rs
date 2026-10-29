mod support;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

fn lang(path: &str) -> PathBuf {
    repo().join("languages").join(path)
}

fn calls(name: &str) -> PathBuf {
    let dir = scratch(name);
    manifest(
        &dir,
        "nitride.nproj",
        "Calls",
        ".calls",
        &[lang("calls/Calls.ngr")],
        "Calls.Program",
        Some(&lang("calls/Calls.ncatalog")),
    );
    dir
}

/// Completion at the end of `text`.
fn complete(root: &Path, path: &Path, text: &str) -> Value {
    let line = text.lines().count().max(1) as u32 - 1;
    let character = text.lines().last().unwrap_or("").encode_utf16().count() as u32;
    let line = if text.ends_with('\n') { line + 1 } else { line };
    let character = if text.ends_with('\n') { 0 } else { character };
    let messages = Session::run(
        root,
        vec![
            open(path, text),
            request(
                5,
                "textDocument/completion",
                json!({"textDocument":{"uri":uri(path)},"position":{"line":line,"character":character}}),
            ),
        ],
    );
    response(&messages, 5).clone()
}

fn labels(result: &Value) -> Vec<String> {
    result["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn names_before_cursor() {
    let dir = calls("complete-names");
    let found = labels(&complete(
        &dir,
        &dir.join("x.calls"),
        "let alpha=1;\nsum(al",
    ));
    assert!(found.contains(&"alpha".to_owned()), "{found:?}");
    let found = labels(&complete(
        &dir,
        &dir.join("x.calls"),
        "let alpha=1;\nsum(be",
    ));
    assert!(!found.contains(&"beta".to_owned()), "{found:?}");
}

#[test]
fn builtins_offered() {
    let dir = calls("complete-builtins");
    let result = complete(&dir, &dir.join("x.calls"), "su");
    let sum = result["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "sum")
        .expect("sum");
    assert_eq!(sum["detail"], "built-in");
}

#[test]
fn keywords_at_statement_start() {
    let dir = calls("complete-keywords");
    let found = labels(&complete(&dir, &dir.join("x.calls"), "let a=1;\nle"));
    assert!(found.contains(&"let".to_owned()), "{found:?}");
}

#[test]
fn completion_mid_edit() {
    let dir = calls("complete-mid-edit");
    for text in ["sum(", "let a=1;\nsu", "let ", ""] {
        let result = complete(&dir, &dir.join("x.calls"), text);
        assert!(result["result"].is_array(), "{text:?}: {result}");
    }
}

#[test]
fn grammar_rules_offered() {
    let dir = scratch("complete-rules");
    manifest(
        &dir,
        "nitride.nproj",
        "PortablePower",
        ".powercalc",
        &[lang("portable/Portable.ngr"), lang("power/Power.ngr")],
        "Portable.Program",
        Some(&lang("power/Power.ncatalog")),
    );
    let text =
        "syntax module Portable.Power\n{\n  extend syntax Portable.Expr\n  {\n    | Pow = Left:Ex";
    let found = labels(&complete(&dir, &lang("power/Power.ngr"), text));
    assert!(found.contains(&"Expr".to_owned()), "{found:?}");
}
