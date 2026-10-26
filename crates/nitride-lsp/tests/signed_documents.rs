mod support;
use serde_json::{Value, json};
use std::path::Path;
use support::*;

const CALC: &str = "#!nitrogen grammar\nlanguage Calc { start Calc.Program; extensions \".calc\"; }\nsyntax module Calc\n{\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Number*;\n}\n";

fn signed_dir() -> std::path::PathBuf {
    repo().join("languages/signed")
}

fn hints(messages: &[Value], id: u64) -> Value {
    response(messages, id)["result"].clone()
}

#[test]
fn program_by_path() {
    let path = signed_dir().join("sample.txt");
    let text = std::fs::read_to_string(&path).unwrap();
    let messages = Session::run(
        &signed_dir(),
        vec![
            open(&path, &text),
            request(
                5,
                "textDocument/inlayHint",
                json!({"textDocument":{"uri":uri(&path)},"range":{"start":{"line":0,"character":0},"end":{"line":9,"character":0}}}),
            ),
            request(
                6,
                "textDocument/semanticTokens/full",
                json!({"textDocument":{"uri":uri(&path)}}),
            ),
            request(
                7,
                "textDocument/formatting",
                json!({"textDocument":{"uri":uri(&path)},"options":{"tabSize":2,"insertSpaces":true}}),
            ),
        ],
    );
    assert_eq!(diagnostics(&messages, &path), json!([]));
    assert_eq!(hints(&messages, 5)[0]["label"], "= 42");
    assert!(
        !response(&messages, 6)["result"]["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // The header is text to the formatter, never blank space to delete.
    let edits = response(&messages, 7)["result"].as_array().unwrap().clone();
    assert!(
        edits.iter().all(|e| e["range"]["start"]["line"] != 0),
        "{edits:?}"
    );
}

#[test]
fn program_by_name() {
    let dir = scratch("signed-by-name");
    std::fs::write(dir.join("Calc.syntax"), CALC).unwrap();
    let path = dir.join("p.txt");
    let messages = Session::run(&dir, vec![open(&path, "#!nitrogen language Calc\n1 2 3\n")]);
    assert_eq!(diagnostics(&messages, &path), json!([]));
}

#[test]
fn missing_and_ambiguous_names() {
    let dir = scratch("signed-ambiguous");
    std::fs::write(dir.join("A.syntax"), CALC).unwrap();
    std::fs::write(dir.join("B.syntax"), CALC).unwrap();
    let ambiguous = dir.join("a.txt");
    let missing = dir.join("m.txt");
    let messages = Session::run(
        &dir,
        vec![
            open(&ambiguous, "#!nitrogen language Calc\n1\n"),
            open(&missing, "#!nitrogen language Nope\n1\n"),
        ],
    );
    let a = diagnostics(&messages, &ambiguous);
    assert_eq!(a.as_array().unwrap().len(), 1, "{a}");
    assert!(
        a[0]["message"]
            .as_str()
            .unwrap()
            .starts_with("language Calc is declared by several signed grammars")
    );
    assert_eq!(a[0]["range"]["start"]["line"], 0);
    let m = diagnostics(&messages, &missing);
    assert_eq!(m[0]["message"], "no signed grammar declares language Nope");
}

#[test]
fn signed_grammar_document() {
    let path = signed_dir().join("Portable.syntax");
    let text = std::fs::read_to_string(&path).unwrap();
    let messages = Session::run(
        &signed_dir(),
        vec![
            open(&path, &text),
            request(
                5,
                "textDocument/documentSymbol",
                json!({"textDocument":{"uri":uri(&path)}}),
            ),
        ],
    );
    assert_eq!(diagnostics(&messages, &path), json!([]));
    assert_eq!(response(&messages, 5)["result"][0]["name"], "Portable");
}

#[test]
fn extension_selects_signed_language() {
    let dir = signed_dir();
    let path = dir.join("x.psig");
    let messages = Session::run(
        &dir,
        vec![
            open(&path, "let x : Scalar = 6;\nx * 7;\n"),
            request(
                5,
                "textDocument/inlayHint",
                json!({"textDocument":{"uri":uri(&path)},"range":{"start":{"line":0,"character":0},"end":{"line":9,"character":0}}}),
            ),
        ],
    );
    assert_eq!(diagnostics(&messages, &path), json!([]));
    assert_eq!(hints(&messages, 5)[0]["label"], "= 42");
}

#[test]
fn editing_signed_grammar_reloads() {
    let dir = scratch("signed-edit");
    let grammar = dir.join("Calc.syntax");
    std::fs::write(&grammar, CALC).unwrap();
    let program = dir.join("p.txt");
    let broken = CALC.replace("start Calc.Program;", "start Calc.Nope;");
    let messages = Session::run(
        &dir,
        vec![
            open(&program, "#!nitrogen language Calc\n1 2\n"),
            open(&grammar, CALC),
            marker(101),
            change(&grammar, 2, &broken),
            marker(102),
            change(&grammar, 3, CALC),
            marker(103),
        ],
    );
    let grammar_after = |id| {
        all_diagnostics(after(&messages, id), &grammar)
            .first()
            .cloned()
            .unwrap()
    };
    let program_after = |id| {
        all_diagnostics(after(&messages, id), &program)
            .first()
            .cloned()
            .unwrap()
    };
    assert!(
        grammar_after(101)
            .to_string()
            .contains("unknown syntax start rule Calc.Nope"),
        "{}",
        grammar_after(101)
    );
    assert_eq!(grammar_after(102), json!([]));
    assert_eq!(program_after(102), json!([]));
    let _ = Path::new("");
}
