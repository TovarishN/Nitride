mod support;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

fn lang(path: &str) -> PathBuf {
    repo().join("languages").join(path)
}

fn calls_workspace(name: &str) -> PathBuf {
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

fn ask(root: &Path, path: &Path, text: &str, method: &str, extra: Value) -> Value {
    let mut params = json!({"textDocument":{"uri":uri(path)}});
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    let messages = Session::run(root, vec![open(path, text), request(5, method, params)]);
    response(&messages, 5)["result"].clone()
}

#[test]
fn program_outline() {
    let dir = calls_workspace("outline-program");
    let symbols = ask(
        &dir,
        &dir.join("x.calls"),
        "let a=1;\nlet b=2;\nsum(a,b);",
        "textDocument/documentSymbol",
        json!({}),
    );
    let symbols = symbols.as_array().unwrap();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0]["name"], "a");
    assert_eq!(symbols[0]["kind"], 13);
    assert_eq!(
        symbols[1]["selectionRange"]["start"],
        json!({"line":1,"character":4})
    );
    assert_eq!(
        symbols[1]["range"]["start"],
        json!({"line":1,"character":0})
    );
}

#[test]
fn ngr_outline() {
    let dir = scratch("outline-ngr");
    let grammar = std::fs::read_to_string(lang("calls/Calls.ngr")).unwrap();
    let symbols = ask(
        &dir,
        &dir.join("Calls.ngr"),
        &grammar,
        "textDocument/documentSymbol",
        json!({}),
    );
    assert_eq!(symbols[0]["name"], "Calls");
    assert_eq!(symbols[0]["kind"], 3);
    let children = symbols[0]["children"].as_array().unwrap();
    let expr = children.iter().find(|c| c["name"] == "Expr").unwrap();
    assert_eq!(expr["kind"], 5);
    let number = children.iter().find(|c| c["name"] == "Number").unwrap();
    assert_eq!(number["kind"], 14);
}

#[test]
fn folds_multiline_rule() {
    let dir = scratch("folds-rule");
    let grammar = std::fs::read_to_string(lang("calls/Calls.ngr")).unwrap();
    let start = grammar
        .lines()
        .position(|l| l.contains("extensible syntax Expr"))
        .unwrap() as u64;
    let end = grammar
        .lines()
        .enumerate()
        .skip(start as usize)
        .find(|(_, l)| *l == "  }")
        .unwrap()
        .0 as u64;
    let folds = ask(
        &dir,
        &dir.join("Calls.ngr"),
        &grammar,
        "textDocument/foldingRange",
        json!({}),
    );
    let fold = folds
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["startLine"] == start)
        .unwrap();
    assert_eq!(fold["endLine"], end);
}

#[test]
fn comment_block_folds() {
    let dir = calls_workspace("folds-comment");
    let folds = ask(
        &dir,
        &dir.join("x.calls"),
        "// one\n// two\n// three\nlet a=1;",
        "textDocument/foldingRange",
        json!({}),
    );
    assert!(
        folds
            .as_array()
            .unwrap()
            .contains(&json!({"startLine":0,"endLine":2,"kind":"comment"}))
    );
}

#[test]
fn selection_climbs() {
    let dir = calls_workspace("selection");
    let text = "let a=duration(4);";
    let ranges = ask(
        &dir,
        &dir.join("x.calls"),
        text,
        "textDocument/selectionRange",
        json!({"positions":[{"line":0,"character":15}]}),
    );
    let mut chain = Vec::new();
    let mut at = &ranges[0];
    while !at.is_null() {
        let (s, e) = (
            at["range"]["start"]["character"].as_u64().unwrap(),
            at["range"]["end"]["character"].as_u64().unwrap(),
        );
        chain.push(&text[s as usize..e as usize]);
        at = &at["parent"];
    }
    assert_eq!(chain.first(), Some(&"4"));
    assert!(chain.contains(&"duration(4)"), "{chain:?}");
    assert_eq!(chain.last(), Some(&text));
    let mut sorted = chain.clone();
    sorted.dedup();
    assert_eq!(sorted.len(), chain.len());
}
