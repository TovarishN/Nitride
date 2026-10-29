//! Catalog files in the editor: their own structure and diagnostics, and
//! navigation between catalogs and the grammars that lower to them.
mod support;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

fn lang(path: &str) -> PathBuf {
    repo().join("languages").join(path)
}

/// A workspace of the Dates language over copies of its files, so a test may
/// edit them.
fn dates(name: &str) -> PathBuf {
    let dir = scratch(name);
    for file in ["Dates.ngr", "Dates.ncatalog"] {
        std::fs::copy(lang("dates").join(file), dir.join(file)).unwrap();
    }
    manifest(
        &dir,
        "nitride.json",
        "Dates",
        ".dates",
        &[dir.join("Dates.ngr")],
        "Dates.Program",
        Some(&dir.join("Dates.ncatalog")),
    );
    dir
}

/// (line, character) of `needle` on the first line containing `line`.
fn find(text: &str, line: &str, needle: &str) -> (u32, u32) {
    let n = text.lines().position(|l| l.contains(line)).unwrap();
    let c = text.lines().nth(n).unwrap().find(needle).unwrap();
    (n as u32, c as u32)
}

fn request_at(dir: &Path, path: &Path, method: &str, (line, character): (u32, u32)) -> Value {
    let text = std::fs::read_to_string(path).unwrap();
    let params = json!({"textDocument":{"uri":uri(path)},"position":{"line":line,"character":character},
        "context":{"includeDeclaration":false}});
    let messages = Session::run(dir, vec![open(path, &text), request(5, method, params)]);
    response(&messages, 5).clone()
}

/// The file name and text a location in `dir` points at.
fn target(dir: &Path, location: &Value) -> (String, String) {
    if location.is_null() {
        return ("none".into(), String::new());
    }
    let name = location["uri"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned();
    let text = std::fs::read_to_string(dir.join(&name)).unwrap();
    let line = location["range"]["start"]["line"].as_u64().unwrap() as usize;
    let (start, end) = (
        location["range"]["start"]["character"].as_u64().unwrap() as usize,
        location["range"]["end"]["character"].as_u64().unwrap() as usize,
    );
    (name, text.lines().nth(line).unwrap()[start..end].to_owned())
}

#[test]
fn lowering_targets_go_to_catalog_members() {
    let dir = dates("catalog-lowers");
    let grammar = dir.join("Dates.ngr");
    let text = std::fs::read_to_string(&grammar).unwrap();
    for (line, needle, file, name) in [
        ("lowers Dates.Plus", "Plus", "Dates.ncatalog", "Plus"),
        ("lowers Dates.Days", "Days(", "Dates.ncatalog", "Days"),
        ("lowers Dates.Plus", "Dates.Plus", "Dates.ncatalog", "Dates"),
        ("builtin value", "today", "Dates.ncatalog", "today"),
    ] {
        let result = request_at(
            &dir,
            &grammar,
            "textDocument/definition",
            find(&text, line, needle),
        );
        assert_eq!(
            target(&dir, &result["result"]),
            (file.into(), name.into()),
            "{line} {needle}"
        );
    }
}

#[test]
fn catalog_names_go_to_their_definitions() {
    let dir = dates("catalog-definitions");
    let catalog = dir.join("Dates.ncatalog");
    let text = std::fs::read_to_string(&catalog).unwrap();
    for (line, needle, file, name) in [
        // Members of the catalog itself.
        ("overload Plus", "Later", "Dates.ncatalog", "Later"),
        ("operation Later", "Duration)", "Dates.ncatalog", "Duration"),
        ("builtin value today", "Date =", "Dates.ncatalog", "Date"),
        // Grammar rules and their fields.
        ("let Let(Value)", "Let(", "Dates.ngr", "Let"),
        ("let Let(Value)", "Value", "Dates.ngr", "Value"),
        ("check Day.Value", "Day", "Dates.ngr", "Day"),
        ("check Day.Value", "Value", "Dates.ngr", "Value"),
    ] {
        let result = request_at(
            &dir,
            &catalog,
            "textDocument/definition",
            find(&text, line, needle),
        );
        assert_eq!(
            target(&dir, &result["result"]),
            (file.into(), name.into()),
            "{line} {needle}"
        );
    }
    // A core type has no definition.
    let result = request_at(
        &dir,
        &catalog,
        "textDocument/definition",
        find(&text, "operation Days", "Scalar"),
    );
    assert!(result["result"].is_null(), "{result}");
}

#[test]
fn catalog_members_find_their_grammar_uses() {
    let dir = dates("catalog-references");
    let catalog = dir.join("Dates.ncatalog");
    let text = std::fs::read_to_string(&catalog).unwrap();
    let uses = |at| {
        request_at(&dir, &catalog, "textDocument/references", at)["result"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| target(&dir, l))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        uses(find(&text, "overload Plus", "Plus")),
        [("Dates.ngr".to_owned(), "Dates.Plus".to_owned())]
    );
    // An operation is used by the overload and by its grammar lowering.
    let mut found = uses(find(&text, "operation Days", "Days"));
    found.sort();
    assert_eq!(found, [("Dates.ngr".to_owned(), "Dates.Days".to_owned())]);
    let found = uses(find(&text, "operation Later", "Later"));
    assert_eq!(found, [("Dates.ncatalog".to_owned(), "Later".to_owned())]);
    // A built-in value is used where the grammar declares the name.
    assert_eq!(
        uses(find(&text, "builtin value pi", "pi")),
        [("Dates.ngr".to_owned(), "pi".to_owned())]
    );
}

#[test]
fn hover_and_rename_over_catalog_members() {
    let dir = dates("catalog-hover");
    let catalog = dir.join("Dates.ncatalog");
    let text = std::fs::read_to_string(&catalog).unwrap();
    let hover = request_at(
        &dir,
        &catalog,
        "textDocument/hover",
        find(&text, "overload Plus", "Later"),
    );
    assert_eq!(
        hover["result"]["contents"]["value"],
        "```ngr\noperation Later(Date, Duration) : Date = host add;\n```\n*operation* `Dates.Later`"
    );
    // A rename of an operation edits its overload and its definition.
    let params = json!({"textDocument":{"uri":uri(&catalog)},"position":{"line":find(&text, "operation Later", "Later").0,"character":find(&text, "operation Later", "Later").1},"newName":"After"});
    let messages = Session::run(
        &dir,
        vec![
            open(&catalog, &text),
            request(5, "textDocument/rename", params),
        ],
    );
    let edits = &response(&messages, 5)["result"]["changes"][uri(&catalog)];
    assert_eq!(edits.as_array().unwrap().len(), 2, "{edits}");
    for (line, needle) in [("let Let(Value)", "Value"), ("builtin value pi", "pi")] {
        let at = find(&text, line, needle);
        let result = request_at(&dir, &catalog, "textDocument/prepareRename", at);
        assert!(result["error"].is_object(), "{line}: {result}");
    }
}

#[test]
fn catalog_structure_and_diagnostics() {
    let dir = dates("catalog-structure");
    let catalog = dir.join("Dates.ncatalog");
    let text = std::fs::read_to_string(&catalog).unwrap();
    let params = json!({"textDocument":{"uri":uri(&catalog)}});
    let messages = Session::run(
        &dir,
        vec![
            open(&catalog, &text),
            request(5, "textDocument/documentSymbol", params),
        ],
    );
    let symbols = &response(&messages, 5)["result"];
    assert_eq!(symbols[0]["name"], "Dates");
    assert_eq!(symbols[0]["kind"], 3);
    let names: Vec<_> = symbols[0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(&names[..3], ["Date", "Duration", "ParseDate"]);
    assert!(names.contains(&"Plus".to_owned()) && names.contains(&"today".to_owned()));
    assert_eq!(diagnostics(&messages, &catalog), json!([]));

    // A field the grammar rule lacks is reported where the catalog names it.
    let broken = text.replace("show Show(Value)", "show Show(Missing)");
    let messages = Session::run(&dir, vec![open(&catalog, &broken)]);
    let problems = diagnostics(&messages, &catalog);
    assert_eq!(
        problems[0]["message"],
        "'Dates.Show' has no field 'Missing'"
    );
    let (line, character) = find(&broken, "show Show", "Missing");
    assert_eq!(
        problems[0]["range"]["start"],
        json!({"line":line,"character":character})
    );

    // A catalog no language lists is still checked alone.
    let lone = dir.join("Lone.ncatalog");
    let lone_text = "catalog module Lone { operation F(Missing) : Scalar = host identity; }";
    std::fs::write(&lone, lone_text).unwrap();
    let messages = Session::run(&dir, vec![open(&lone, lone_text)]);
    assert_eq!(
        diagnostics(&messages, &lone)[0]["message"],
        "unknown type 'Missing'"
    );
}

#[test]
fn catalog_tokens_colour_names() {
    let dir = dates("catalog-tokens");
    let catalog = dir.join("Dates.ncatalog");
    let text = std::fs::read_to_string(&catalog).unwrap();
    let params = json!({"textDocument":{"uri":uri(&catalog)}});
    let messages = Session::run(
        &dir,
        vec![
            open(&catalog, &text),
            request(5, "textDocument/semanticTokens/full", params),
        ],
    );
    let data = response(&messages, 5)["result"]["data"]
        .as_array()
        .unwrap()
        .clone();
    assert!(data.len() > 100, "{}", data.len());
}
