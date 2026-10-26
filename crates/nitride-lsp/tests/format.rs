mod support;
use nitride_lsp::lines::LineIndex;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

/// Applies LSP text edits (non-overlapping) to `text`.
fn apply(text: &str, edits: &Value) -> String {
    let lines = LineIndex::new(text);
    let mut spans: Vec<(u32, u32, String)> = edits
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let at = |p: &Value| {
                lines.offset(
                    p["line"].as_u64().unwrap() as u32,
                    p["character"].as_u64().unwrap() as u32,
                )
            };
            (
                at(&e["range"]["start"]),
                at(&e["range"]["end"]),
                e["newText"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    spans.sort_by_key(|s| std::cmp::Reverse(s.0));
    let mut out = text.to_owned();
    let mut last = u32::MAX;
    for (start, end, new) in spans {
        assert!(end <= last, "overlapping edits");
        out.replace_range(start as usize..end as usize, &new);
        last = start;
    }
    out
}

fn options(tab_size: u32, spaces: bool) -> Value {
    json!({"tabSize": tab_size, "insertSpaces": spaces})
}

/// Formats each (path, text) in one session; returns the edits per file.
fn format_all(root: &Path, files: &[(PathBuf, String)], method: &str, extra: Value) -> Vec<Value> {
    let mut messages: Vec<Value> = files.iter().map(|(p, t)| open(p, t)).collect();
    for (i, (path, _)) in files.iter().enumerate() {
        let mut params = json!({"textDocument":{"uri":uri(path)},"options":options(2, true)});
        for (k, v) in extra.as_object().unwrap() {
            params[k] = v.clone();
        }
        messages.push(request(100 + i as u64, method, params));
    }
    let messages = Session::run(root, messages);
    (0..files.len())
        .map(|i| response(&messages, 100 + i as u64)["result"].clone())
        .collect()
}

fn format_one(text: &str, extra: Value) -> String {
    let dir = scratch(&format!("format-{}", text.len()));
    let path = dir.join("M.ngr");
    let edits = &format_all(
        &dir,
        &[(path, text.to_owned())],
        "textDocument/formatting",
        extra,
    )[0];
    apply(text, edits)
}

const FLAT: &str =
    "syntax module M\n{\ntoken Number = ['0'..'9']+;\n      syntax Program = Items:Number*;\n}\n";

#[test]
fn indents_blocks() {
    assert_eq!(
        format_one(FLAT, json!({})),
        "syntax module M\n{\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Number*;\n}\n"
    );
}

#[test]
fn uses_tabs_when_asked() {
    let formatted = format_one(FLAT, json!({"options": options(4, false)}));
    assert!(formatted.contains("\n\ttoken Number"), "{formatted:?}");
}

#[test]
fn trims_and_collapses() {
    let text = "syntax module M\n{\n  token A = \"a\";   \n\n\n\n  syntax P = A*;\n}\n\n\n";
    assert_eq!(
        format_one(text, json!({})),
        "syntax module M\n{\n  token A = \"a\";\n\n  syntax P = A*;\n}\n"
    );
}

#[test]
fn format_keeps_crlf() {
    let text = FLAT.replace('\n', "\r\n");
    let formatted = format_one(&text, json!({}));
    assert_eq!(
        formatted,
        "syntax module M\r\n{\r\n  token Number = ['0'..'9']+;\r\n  syntax Program = Items:Number*;\r\n}\r\n"
    );
}

#[test]
fn range_only_touches_range() {
    let dir = scratch("format-range");
    let path = dir.join("M.ngr");
    let range = json!({"range":{"start":{"line":2,"character":0},"end":{"line":2,"character":5}}});
    let edits = &format_all(
        &dir,
        &[(path, FLAT.to_owned())],
        "textDocument/rangeFormatting",
        range,
    )[0];
    let formatted = apply(FLAT, edits);
    assert!(formatted.contains("\n  token Number"), "{formatted:?}");
    assert!(
        formatted.contains("\n      syntax Program"),
        "{formatted:?}"
    );
}

#[test]
fn on_type_dedents_closing_brace() {
    let dir = scratch("format-on-type");
    let path = dir.join("M.ngr");
    let text = "syntax module M\n{\n  token A = \"a\";\n    }";
    let at = json!({"position":{"line":3,"character":5},"ch":"}"});
    let edits = &format_all(
        &dir,
        &[(path, text.to_owned())],
        "textDocument/onTypeFormatting",
        at,
    )[0];
    assert_eq!(
        apply(text, edits),
        "syntax module M\n{\n  token A = \"a\";\n}"
    );
}

fn squeeze(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn format_only_whitespace() {
    let root = repo();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join("languages")).unwrap() {
        let dir = entry.unwrap().path();
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        if let Ok(grammars) = std::fs::read_dir(dir.join("Grammars")) {
            paths.extend(grammars.map(|e| e.unwrap().path()));
        }
        for path in paths {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.ends_with(".ngr") || name.starts_with("sample.") {
                files.push((path.clone(), std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    assert!(files.len() > 20);
    let edits = format_all(&root, &files, "textDocument/formatting", json!({}));
    let formatted: Vec<(PathBuf, String)> = files
        .iter()
        .zip(&edits)
        .map(|((path, text), edits)| {
            let out = apply(text, edits);
            assert_eq!(squeeze(&out), squeeze(text), "{}", path.display());
            (path.clone(), out)
        })
        .collect();
    // Formatting a grammar needs only its syntax tree; outside the repository's
    // manifest, edited grammars do not re-prepare their languages.
    let (grammars, samples): (Vec<_>, Vec<_>) = formatted
        .into_iter()
        .partition(|(p, _)| p.extension().is_some_and(|e| e == "ngr"));
    let again = format_all(
        &scratch("format-again"),
        &grammars,
        "textDocument/formatting",
        json!({}),
    )
    .into_iter()
    .zip(&grammars)
    .chain(
        format_all(&root, &samples, "textDocument/formatting", json!({}))
            .into_iter()
            .zip(&samples),
    );
    for (edits, (path, _)) in again {
        assert_eq!(edits, json!([]), "not idempotent: {}", path.display());
    }
}

/// Trailing spaces inside a multi-line token (a C# block) belong to it.
#[test]
fn token_content_untouched() {
    let text = "syntax module M\n{\n  token A = \"a\";\n  syntax P = A* { var s = 1;   \n    s = 2; }\n}\n";
    let formatted = format_one(text, json!({}));
    assert!(formatted.contains("var s = 1;   \n"), "{formatted:?}");
}
