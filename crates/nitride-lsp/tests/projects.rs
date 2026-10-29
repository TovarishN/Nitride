//! Project files in the editor: structure, colour, diagnostics, and navigation
//! from a `.nproj` to the files and start rule it names.
mod support;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

const DATES: &str = "// Dates.\nlanguage Dates\n{\n  extensions .dates;\n  grammar Dates.ngr;\n  start Dates.Program;\n  catalog Dates.ncatalog;\n}\n";

/// A workspace whose `dates` folder holds copies of the Dates files and `DATES`.
fn dates(name: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(name);
    std::fs::create_dir_all(dir.join("dates")).unwrap();
    for file in ["Dates.ngr", "Dates.ncatalog"] {
        std::fs::copy(
            repo().join("languages/dates").join(file),
            dir.join("dates").join(file),
        )
        .unwrap();
    }
    let file = dir.join("dates/Dates.nproj");
    std::fs::write(&file, DATES).unwrap();
    std::fs::write(dir.join("nitride.nproj"), "workspace { include dates; }\n").unwrap();
    (dir, file)
}

/// (line, character) of `needle` on the first line containing it.
fn find(text: &str, needle: &str) -> (u32, u32) {
    let n = text.lines().position(|l| l.contains(needle)).unwrap();
    let c = text.lines().nth(n).unwrap().find(needle).unwrap();
    (n as u32, c as u32)
}

fn ask(root: &Path, path: &Path, text: &str, method: &str, extra: Value) -> Value {
    let mut params = json!({"textDocument":{"uri":uri(path)}});
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    let messages = Session::run(root, vec![open(path, text), request(5, method, params)]);
    response(&messages, 5)["result"].clone()
}

fn definition(root: &Path, path: &Path, text: &str, needle: &str) -> Value {
    let (line, character) = find(text, needle);
    ask(
        root,
        path,
        text,
        "textDocument/definition",
        json!({"position":{"line":line,"character":character + 1}}),
    )
}

#[test]
fn nproj_document_symbols() {
    let (dir, file) = dates("nproj-symbols");
    let symbols = ask(&dir, &file, DATES, "textDocument/documentSymbol", json!({}));
    assert_eq!(symbols[0]["name"], "Dates", "{symbols}");
}

#[test]
fn definition_on_grammar_path_opens_file() {
    let (dir, file) = dates("nproj-path");
    let found = definition(&dir, &file, DATES, "Dates.ngr");
    assert_eq!(found["uri"], uri(&dir.join("dates/Dates.ngr")), "{found}");
    assert_eq!(found["range"]["start"], json!({"line":0,"character":0}));
    let found = definition(&dir, &file, DATES, "Dates.ncatalog");
    assert_eq!(
        found["uri"],
        uri(&dir.join("dates/Dates.ncatalog")),
        "{found}"
    );
}

#[test]
fn definition_on_include_opens_language_file() {
    let (dir, _) = dates("nproj-include");
    let root = dir.join("nitride.nproj");
    let text = std::fs::read_to_string(&root).unwrap();
    let found = definition(&dir, &root, &text, "dates;");
    assert_eq!(found["uri"], uri(&dir.join("dates/Dates.nproj")), "{found}");
}

#[test]
fn definition_on_start_goes_to_rule() {
    let (dir, file) = dates("nproj-start");
    let found = definition(&dir, &file, DATES, "Dates.Program");
    assert_eq!(found["uri"], uri(&dir.join("dates/Dates.ngr")), "{found}");
    let grammar = std::fs::read_to_string(dir.join("dates/Dates.ngr")).unwrap();
    let line = grammar
        .lines()
        .position(|l| l.contains("syntax Program"))
        .unwrap();
    assert_eq!(found["range"]["start"]["line"], line as u64, "{found}");
}

#[test]
fn unlisted_nproj_is_checked_alone() {
    let dir = scratch("nproj-alone");
    let file = dir.join("Lone.nproj");
    let text = "language Lone { grammar Gone.ngr; start Lone.S; }";
    let messages = Session::run(&dir, vec![open(&file, text)]);
    let found = diagnostics(&messages, &file);
    assert!(found.to_string().contains("file not found"), "{found}");
    assert_eq!(found[0]["range"]["start"], json!({"line":0,"character":24}));
}

#[test]
fn listed_nproj_shows_workspace_problems_once() {
    let (dir, file) = dates("nproj-listed");
    let text = DATES.replace("start Dates.Program;", "start Dates.Nope;");
    std::fs::write(&file, &text).unwrap();
    let messages = Session::run(&dir, vec![open(&file, &text)]);
    let found = diagnostics(&messages, &file);
    assert_eq!(found.as_array().unwrap().len(), 1, "{found}");
    assert!(found.to_string().contains("start rule"), "{found}");
}

#[test]
fn nproj_semantic_tokens() {
    let (dir, file) = dates("nproj-tokens");
    let tokens = ask(
        &dir,
        &file,
        DATES,
        "textDocument/semanticTokens/full",
        json!({}),
    );
    let data: Vec<u64> = tokens["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    // `language` opens line 1 (after the comment line, if comments are tokens).
    let starts: Vec<_> = data.chunks(5).map(|t| t[2]).collect();
    assert!(starts.contains(&8), "{data:?}");
    assert!(data.len() >= 5 * 5, "{data:?}");
}
