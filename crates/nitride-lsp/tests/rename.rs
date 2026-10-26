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
        "nitride.json",
        "Calls",
        ".calls",
        &[lang("calls/Calls.ngr")],
        "Calls.Program",
        Some(&lang("calls/catalog.json")),
    );
    dir
}

fn send(
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

#[test]
fn rename_program_symbol() {
    let dir = calls("rename-program");
    let path = dir.join("x.calls");
    let result = send(
        &dir,
        &path,
        "let a=1;\nsum(a,a);",
        "textDocument/rename",
        1,
        4,
        json!({"newName":"total"}),
    );
    let edits = result["result"]["changes"][uri(&path)]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(edits.len(), 3);
    assert!(edits.iter().all(|e| e["newText"] == "total"));
}

#[test]
fn prepare_rename_refuses_builtin() {
    let dir = calls("rename-builtin");
    let path = dir.join("x.calls");
    let result = send(
        &dir,
        &path,
        "sum(1,2);",
        "textDocument/prepareRename",
        0,
        1,
        json!({}),
    );
    assert_eq!(result["error"]["code"], -32602);
    assert_eq!(
        result["error"]["message"],
        "built-in names cannot be renamed"
    );
    let ok = send(
        &dir,
        &path,
        "let a=1;\na;",
        "textDocument/prepareRename",
        1,
        0,
        json!({}),
    );
    assert_eq!(ok["result"]["placeholder"], "a");
}

#[test]
fn rename_rejects_non_identifier() {
    let dir = calls("rename-invalid");
    let path = dir.join("x.calls");
    let result = send(
        &dir,
        &path,
        "let a=1;\na;",
        "textDocument/rename",
        1,
        0,
        json!({"newName":"1x"}),
    );
    assert_eq!(result["error"]["code"], -32602);
    assert_eq!(result["error"]["message"], "not an identifier");
}

#[test]
fn rename_rule_across_files() {
    let dir = scratch("rename-rule");
    for file in ["portable/Portable.ngr", "power/Power.ngr"] {
        std::fs::copy(lang(file), dir.join(Path::new(file).file_name().unwrap())).unwrap();
    }
    let (portable, power) = (dir.join("Portable.ngr"), dir.join("Power.ngr"));
    manifest(
        &dir,
        "nitride.json",
        "PortablePower",
        ".powercalc",
        &[portable.clone(), power.clone()],
        "Portable.Program",
        Some(&lang("power/catalog.json")),
    );
    let text = std::fs::read_to_string(&power).unwrap();
    let line = text
        .lines()
        .position(|l| l.contains("Portable.Expr"))
        .unwrap() as u32;
    let character = text
        .lines()
        .nth(line as usize)
        .unwrap()
        .find("Portable.Expr")
        .unwrap() as u32;
    let result = send(
        &dir,
        &power,
        &text,
        "textDocument/rename",
        line,
        character + 10,
        json!({"newName":"Term"}),
    );
    let changes = &result["result"]["changes"];
    let in_power = changes[uri(&power)].as_array().unwrap();
    assert!(changes[uri(&portable)].as_array().unwrap().len() > 1);
    let extend = in_power
        .iter()
        .find(|e| e["range"]["start"]["line"] == line)
        .unwrap();
    assert_eq!(extend["range"]["start"]["character"], character + 9);
    assert_eq!(extend["newText"], "Term");
}

#[test]
fn rename_after_emoji() {
    let dir = calls("rename-emoji");
    let path = dir.join("x.calls");
    let result = send(
        &dir,
        &path,
        "/* 😀 */ let a=1;",
        "textDocument/rename",
        0,
        13,
        json!({"newName":"b"}),
    );
    let edits = &result["result"]["changes"][uri(&path)];
    // "/* 😀 */ " is 9 UTF-16 units (the emoji is a surrogate pair), "let " 4 more.
    assert_eq!(edits[0]["range"]["start"]["character"], 13);
    assert_eq!(edits[0]["range"]["end"]["character"], 14);
}

#[test]
fn rename_refuses_when_sibling_damaged() {
    let dir = scratch("rename-damaged");
    for file in ["portable/Portable.ngr", "power/Power.ngr"] {
        std::fs::copy(lang(file), dir.join(Path::new(file).file_name().unwrap())).unwrap();
    }
    let (portable, power) = (dir.join("Portable.ngr"), dir.join("Power.ngr"));
    std::fs::write(
        &power,
        std::fs::read_to_string(&power)
            .unwrap()
            .replace("precedence 25", "precedence ((("),
    )
    .unwrap();
    manifest(
        &dir,
        "nitride.json",
        "PortablePower",
        ".powercalc",
        &[portable.clone(), power.clone()],
        "Portable.Program",
        Some(&lang("power/catalog.json")),
    );
    let text = std::fs::read_to_string(&portable).unwrap();
    let line = text
        .lines()
        .position(|l| l.contains("extensible syntax Expr"))
        .unwrap() as u32;
    let result = send(
        &dir,
        &portable,
        &text,
        "textDocument/rename",
        line,
        22,
        json!({"newName":"Term"}),
    );
    assert_eq!(result["error"]["code"], -32602, "{result}");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Power.ngr"),
        "{result}"
    );
}
