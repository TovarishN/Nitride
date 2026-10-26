mod support;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

pub fn lang(path: &str) -> PathBuf {
    repo().join("languages").join(path)
}

const STANDARD: [&str; 12] = [
    "namespace",
    "type",
    "function",
    "variable",
    "parameter",
    "property",
    "keyword",
    "operator",
    "number",
    "string",
    "comment",
    "enumMember",
];

fn package_types() -> Vec<String> {
    let package: Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("editors/vscode/package.json")).unwrap(),
    )
    .unwrap();
    package["contributes"]["semanticTokenTypes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_owned())
        .collect()
}

/// Initializes (with or without palette tokens), opens each file and asks for its tokens.
pub fn session(
    root: &Path,
    palette: bool,
    files: &[(PathBuf, String)],
) -> (Vec<String>, Vec<Value>) {
    let mut init = initialize(1, root);
    if palette {
        init["params"]["initializationOptions"] = json!({"paletteTokens": true});
    }
    let mut input = frame(&init);
    for (i, (path, text)) in files.iter().enumerate() {
        input.extend(frame(&open(path, text)));
        input.extend(frame(&request(
            100 + i as u64,
            "textDocument/semanticTokens/full",
            json!({"textDocument":{"uri":uri(path)}}),
        )));
    }
    input.extend(frame(&request(9999, "shutdown", Value::Null)));
    input.extend(frame(&notification("exit", Value::Null)));
    let messages = Session::raw(input);
    let legend = response(&messages, 1)["result"]["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_owned())
        .collect();
    let data = (0..files.len())
        .map(|i| response(&messages, 100 + i as u64)["result"]["data"].clone())
        .collect();
    (legend, data)
}

/// (line, column, text, type name) per token; checks order, overlap and legend bounds.
pub fn decode(data: &Value, text: &str, legend: &[String]) -> Vec<(u32, u32, String, String)> {
    let lines: Vec<Vec<u16>> = text
        .split('\n')
        .map(|l| l.trim_end_matches('\r').encode_utf16().collect())
        .collect();
    let numbers: Vec<u64> = data
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap())
        .collect();
    let (mut line, mut column, mut end) = (0u32, 0u32, 0u32);
    let mut out = Vec::new();
    for t in numbers.chunks(5) {
        if t[0] > 0 {
            column = 0;
            end = 0;
        }
        line += t[0] as u32;
        column += t[1] as u32;
        assert!(column >= end, "overlapping tokens at {line}:{column}");
        end = column + t[2] as u32;
        assert!(
            (t[3] as usize) < legend.len(),
            "type {} outside legend",
            t[3]
        );
        let units = &lines[line as usize][column as usize..end as usize];
        out.push((
            line,
            column,
            String::from_utf16(units).unwrap(),
            legend[t[3] as usize].clone(),
        ));
    }
    out
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

fn kinds(root: &Path, file: &str, text: &str, palette: bool) -> Vec<(String, String)> {
    let path = root.join(file);
    let (legend, data) = session(root, palette, &[(path, text.to_owned())]);
    decode(&data[0], text, &legend)
        .into_iter()
        .map(|(_, _, t, k)| (t, k))
        .collect()
}

#[test]
fn palette_legend_when_asked() {
    let dir = calls("palette-legend");
    let (legend, _) = session(&dir, true, &[]);
    let mut expected: Vec<String> = STANDARD.iter().map(|s| s.to_string()).collect();
    expected.extend(package_types());
    assert_eq!(legend, expected);
    let (plain, _) = session(&dir, false, &[]);
    assert_eq!(plain, STANDARD);
}

#[test]
fn let_is_declaration_keyword_token() {
    let dir = calls("palette-let");
    let found = kinds(&dir, "x.calls", "let a=1;", true);
    assert!(
        found.contains(&("let".into(), "declarationKeyword".into())),
        "{found:?}"
    );
    assert!(
        found.contains(&(";".into(), "terminator".into())),
        "{found:?}"
    );
}

#[test]
fn duration_is_operation_word_token() {
    let dir = scratch("palette-duration");
    manifest(
        &dir,
        "nitride.json",
        "Portable",
        ".portable",
        &[lang("portable/Portable.ngr")],
        "Portable.Program",
        Some(&lang("portable/catalog.json")),
    );
    let found = kinds(&dir, "x.portable", "duration(3);", true);
    assert!(
        found.contains(&("duration".into(), "operationWord".into())),
        "{found:?}"
    );
}

#[test]
fn fallback_legend_is_valid() {
    let dir = calls("palette-fallback");
    let text = std::fs::read_to_string(lang("calls/sample.calls")).unwrap();
    let found = kinds(&dir, "x.calls", &text, false);
    assert!(
        found.iter().all(|(t, _)| t != "(" && t != ")" && t != ";"),
        "{found:?}"
    );
    assert!(
        found.contains(&("let".into(), "keyword".into())),
        "{found:?}"
    );
}

#[test]
fn longest_literal_wins() {
    let dir = scratch("palette-longest");
    let grammar = dir.join("A.ngr");
    std::fs::write(&grammar, "syntax module A\n{\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Pair*;\n  syntax Pair = Key:Number \"=>\" Value:Number \";\";\n}\n").unwrap();
    manifest(
        &dir,
        "nitride.json",
        "A",
        ".a",
        &[grammar],
        "A.Program",
        None,
    );
    let found = kinds(&dir, "x.a", "1 => 2;", true);
    assert!(
        found.contains(&("=>".into(), "operator".into())),
        "{found:?}"
    );
}

#[test]
fn roles_in_damaged_program() {
    let dir = calls("palette-damaged");
    for text in ["let =", "sum(1,", "let a=1;\nsum(a,"] {
        let found = kinds(&dir, "x.calls", text, true);
        assert!(!found.is_empty(), "{text:?}");
    }
}
