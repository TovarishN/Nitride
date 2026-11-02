mod support;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

fn lang(path: &str) -> PathBuf {
    repo().join("languages").join(path)
}

fn workspace(
    name: &str,
    language: &str,
    extension: &str,
    grammars: &[&str],
    start: &str,
    catalog: &str,
) -> PathBuf {
    let dir = scratch(name);
    let grammars: Vec<_> = grammars.iter().map(|g| lang(g)).collect();
    manifest(
        &dir,
        "nitride.nproj",
        language,
        extension,
        &grammars,
        start,
        Some(&lang(catalog)),
    );
    dir
}

fn hints(root: &Path, path: &Path, text: &str, range: Value) -> Value {
    let messages = Session::run(
        root,
        vec![
            open(path, text),
            request(
                5,
                "textDocument/inlayHint",
                json!({"textDocument":{"uri":uri(path)},"range":range}),
            ),
        ],
    );
    response(&messages, 5)["result"].clone()
}

fn everything() -> Value {
    json!({"start":{"line":0,"character":0},"end":{"line":99,"character":0}})
}

fn portable(name: &str) -> PathBuf {
    workspace(
        name,
        "Portable",
        ".portable",
        &["portable/Portable.ngr"],
        "Portable.Program",
        "portable/Portable.ncatalog",
    )
}

#[test]
fn scalar_value_hint() {
    let dir = portable("hints-scalar");
    let result = hints(
        &dir,
        &dir.join("x.portable"),
        "let x : Scalar = 6;\nx * 7;",
        everything(),
    );
    assert_eq!(
        result,
        json!([{"position":{"line":1,"character":5},"label":"= 42","paddingLeft":true}])
    );
}

#[test]
fn date_value_hint() {
    let dir = workspace(
        "hints-date",
        "Dates",
        ".dates",
        &["dates/Dates.ngr"],
        "Dates.Program",
        "dates/Dates.ncatalog",
    );
    let result = hints(
        &dir,
        &dir.join("x.dates"),
        "2026-10-05 + 2 days;",
        everything(),
    );
    assert_eq!(result[0]["label"], "= 2026-10-07");
}

#[test]
fn range_filters_hints() {
    let dir = portable("hints-range");
    let text = "1 + 1;\n2 + 2;";
    let result = hints(
        &dir,
        &dir.join("x.portable"),
        text,
        json!({"start":{"line":1,"character":0},"end":{"line":1,"character":6}}),
    );
    assert_eq!(result.as_array().unwrap().len(), 1);
    assert_eq!(result[0]["label"], "= 4");
}

#[test]
fn failed_execution_has_no_hints() {
    let dir = portable("hints-failed");
    assert_eq!(
        hints(&dir, &dir.join("x.portable"), "1 / 0;", everything()),
        json!([])
    );
    let syntax = scratch("hints-syntax");
    manifest(
        &syntax,
        "nitride.nproj",
        "Xml",
        ".xml",
        &[lang("xml/Xml.ngr")],
        "Xml.Document",
        None,
    );
    assert_eq!(
        hints(&syntax, &syntax.join("x.xml"), "<a/>", everything()),
        json!([])
    );
}

/// A workspace whose `calendar` folder copies the Calendar language: its
/// operations are bound to a WebAssembly component and a Python process.
fn calendar(name: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(name);
    let folder = dir.join("calendar");
    std::fs::create_dir_all(&folder).unwrap();
    for file in [
        "Calendar.ngr",
        "Calendar.ncatalog",
        "Calendar.nproj",
        "gregorian.nbind",
        "gregorian.wasm",
        "clock.nbind",
        "clock.py",
    ] {
        std::fs::copy(lang("calendar").join(file), folder.join(file)).unwrap();
    }
    std::fs::write(
        dir.join("nitride.nproj"),
        "workspace { include calendar; }\n",
    )
    .unwrap();
    (dir, folder.join("sample.calendar"))
}

fn labels(hints: &Value) -> Vec<(u64, String)> {
    hints
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            let line = h["position"]["line"].as_u64().unwrap();
            (line, h["label"].as_str().unwrap().to_owned())
        })
        .collect()
}

#[cfg(feature = "components")]
#[test]
fn component_bound_operations_have_hints() {
    let (dir, file) = calendar("component-hints");
    let text = "let kickoff = 2026-10-05;\nlet sprint = 14 days;\nkickoff + sprint;\nweekday(kickoff + sprint);\nleap(2028-01-01);\n";
    let found = labels(&hints(&dir, &file, text, everything()));
    assert_eq!(
        found,
        [
            (2, "= 2026-10-19".to_owned()),
            (3, "= \"Monday\"".to_owned()),
            (4, "= true".to_owned())
        ],
        "{found:?}"
    );
}

/// Opening a file never starts a binding's process, so what needs one has no hints.
#[test]
fn process_bound_operations_have_no_hints() {
    let (dir, file) = calendar("process-hints");
    let found = labels(&hints(&dir, &file, "today;\n", everything()));
    assert!(found.is_empty(), "{found:?}");
}
