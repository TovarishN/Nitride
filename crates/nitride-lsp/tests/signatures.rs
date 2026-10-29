mod support;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

fn calls(name: &str) -> PathBuf {
    let dir = scratch(name);
    let lang = |p: &str| repo().join("languages").join(p);
    manifest(
        &dir,
        "nitride.json",
        "Calls",
        ".calls",
        &[lang("calls/Calls.ngr")],
        "Calls.Program",
        Some(&lang("calls/Calls.ncatalog")),
    );
    dir
}

/// Signature help at the end of a one-line `text`.
fn help(root: &Path, text: &str) -> Value {
    let path = root.join("x.calls");
    let character = text.encode_utf16().count();
    let messages = Session::run(
        root,
        vec![
            open(&path, text),
            request(
                5,
                "textDocument/signatureHelp",
                json!({"textDocument":{"uri":uri(&path)},"position":{"line":0,"character":character}}),
            ),
        ],
    );
    response(&messages, 5)["result"].clone()
}

fn active(result: &Value) -> (String, u64) {
    let signature = &result["signatures"][result["activeSignature"].as_u64().unwrap() as usize];
    (
        signature["label"].as_str().unwrap().to_owned(),
        result["activeParameter"].as_u64().unwrap(),
    )
}

#[test]
fn builtin_signature() {
    let dir = calls("signature-builtin");
    let result = help(&dir, "sum(1,");
    assert_eq!(
        active(&result),
        ("sum(Core.Scalar, Core.Scalar) : Core.Scalar".into(), 1)
    );
    let parameter = &result["signatures"][0]["parameters"][1]["label"];
    let label = result["signatures"][0]["label"].as_str().unwrap();
    let span = [
        parameter[0].as_u64().unwrap() as usize,
        parameter[1].as_u64().unwrap() as usize,
    ];
    assert_eq!(&label[span[0]..span[1]], "Core.Scalar");
}

#[test]
fn nested_call_active_parameter() {
    let dir = calls("signature-nested");
    assert_eq!(
        active(&help(&dir, "sum(neg(1), ")).0,
        "sum(Core.Scalar, Core.Scalar) : Core.Scalar"
    );
    assert_eq!(active(&help(&dir, "sum(neg(1), ")).1, 1);
    assert_eq!(
        active(&help(&dir, "sum(neg(")),
        ("neg(Core.Scalar) : Core.Scalar".into(), 0)
    );
}

#[test]
fn outside_call_is_null() {
    let dir = calls("signature-outside");
    assert_eq!(help(&dir, "sum(1,2);"), Value::Null);
    assert_eq!(help(&dir, "let a=1"), Value::Null);
}

/// `(` triggers signature help as the user types; non-ASCII text before it
/// must not crash the server.
#[test]
fn non_ascii_before_paren() {
    let dir = calls("signature-unicode");
    for text in ["é(", "let b = ü(", "→(", "😀(", "sum(é("] {
        let result = help(&dir, text);
        assert!(result.is_null() || result.is_object(), "{text}: {result}");
    }
}
