mod support;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

const TYPES: [&str; 11] = [
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
];

/// (line, column, text, type, modifier bits) per token.
fn decode(data: &Value, text: &str) -> Vec<(u32, u32, String, &'static str, u64)> {
    let lines: Vec<Vec<u16>> = text
        .split('\n')
        .map(|l| l.encode_utf16().collect())
        .collect();
    let numbers: Vec<u64> = data
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap())
        .collect();
    let (mut line, mut column) = (0u32, 0u32);
    numbers
        .chunks(5)
        .map(|t| {
            if t[0] > 0 {
                column = 0;
            }
            line += t[0] as u32;
            column += t[1] as u32;
            let units = &lines[line as usize][column as usize..(column + t[2] as u32) as usize];
            (
                line,
                column,
                String::from_utf16(units).unwrap(),
                TYPES[t[3] as usize],
                t[4],
            )
        })
        .collect()
}

fn tokens(root: &Path, path: &Path, text: &str) -> Vec<(u32, u32, String, &'static str, u64)> {
    let messages = Session::run(
        root,
        vec![
            open(path, text),
            request(
                5,
                "textDocument/semanticTokens/full",
                json!({"textDocument":{"uri":uri(path)}}),
            ),
        ],
    );
    let init = response(&messages, 1);
    assert_eq!(
        init["result"]["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"],
        json!(TYPES)
    );
    decode(&response(&messages, 5)["result"]["data"], text)
}

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

#[test]
fn calls_sample_tokens() {
    let dir = calls_workspace("tokens-calls");
    let found: Vec<_> = tokens(&dir, &dir.join("x.calls"), "let x=duration(4);")
        .into_iter()
        .map(|(_, _, text, ty, mods)| (text, ty, mods))
        .collect();
    let expected = [
        ("let", "keyword", 0),
        ("x", "variable", 1),
        ("=", "operator", 0),
        ("(", "operator", 0),
        ("4", "number", 0),
        (")", "operator", 0),
        (";", "operator", 0),
    ];
    let expected: Vec<_> = expected
        .iter()
        .map(|(t, ty, m)| (t.to_string(), *ty, *m as u64))
        .collect();
    assert_eq!(found, expected);
}

#[test]
fn ngr_tokens() {
    let dir = scratch("tokens-ngr");
    let path = dir.join("M.ngr");
    let text = "// comment\nsyntax module M\n{\n  token Number = ['0'..'9']+;\n  syntax Program = \"let\" Items:Number*;\n}\n";
    let found = tokens(&dir, &path, text);
    let has = |t: &str, ty: &str| {
        found
            .iter()
            .any(|(_, _, text, kind, _)| text == t && *kind == ty)
    };
    assert!(has("// comment", "comment"), "{found:?}");
    assert!(has("syntax", "keyword"), "{found:?}");
    assert!(has("Program", "type"), "{found:?}");
    assert!(has("\"let\"", "string"), "{found:?}");
    assert!(has("M", "namespace"), "{found:?}");
}

#[test]
fn tokens_after_emoji() {
    let dir = calls_workspace("tokens-emoji");
    let found = tokens(&dir, &dir.join("x.calls"), "/* 😀 */ let y=1;");
    let first = &found[0];
    assert_eq!(
        (first.0, first.1, first.2.as_str(), first.3),
        (0, 0, "/* 😀 */", "comment")
    );
    let keyword = found.iter().find(|t| t.2 == "let").unwrap();
    assert_eq!((keyword.0, keyword.1), (0, 9));
}

#[test]
fn tokens_without_language_are_empty() {
    let dir = scratch("tokens-none");
    let path = dir.join("notes.txt");
    let messages = Session::run(
        &dir,
        vec![
            open(&path, "hello"),
            request(
                5,
                "textDocument/semanticTokens/full",
                json!({"textDocument":{"uri":uri(&path)}}),
            ),
        ],
    );
    assert_eq!(response(&messages, 5)["result"], json!({"data": []}));
}

/// A request that arrives before the debounce elapses still sees the
/// workspace languages and the latest text.
#[test]
fn tokens_before_debounce_elapses() {
    let dir = calls_workspace("tokens-debounce");
    let path = dir.join("x.calls");
    let mut input = frame(&initialize(1, &dir));
    input.extend(frame(&open(&path, "let x=1;")));
    input.extend(frame(&request(
        5,
        "textDocument/semanticTokens/full",
        json!({"textDocument":{"uri":uri(&path)}}),
    )));
    input.extend(frame(&request(6, "shutdown", Value::Null)));
    input.extend(frame(&notification("exit", Value::Null)));
    let messages = Session::raw_with(input, std::time::Duration::from_secs(5));
    assert!(
        !response(&messages, 5)["result"]["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
