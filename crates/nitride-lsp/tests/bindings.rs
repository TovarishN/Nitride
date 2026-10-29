//! Binding files in the editor: checked against the catalogs of the language
//! that lists them, with an outline.
mod support;
use serde_json::json;
use std::path::PathBuf;
use support::*;

/// A workspace holding a copy of the Calendar language folder.
fn calendar(name: &str) -> PathBuf {
    let dir = scratch(name);
    let home = dir.join("calendar");
    std::fs::create_dir_all(&home).unwrap();
    for file in [
        "Calendar.ngr",
        "Calendar.ncatalog",
        "Calendar.nproj",
        "gregorian.nbind",
    ] {
        std::fs::copy(
            repo().join("languages/calendar").join(file),
            home.join(file),
        )
        .unwrap();
    }
    std::fs::write(
        dir.join("nitride.nproj"),
        "workspace\n{\n  include calendar;\n}\n",
    )
    .unwrap();
    dir
}

#[test]
fn bindings_are_checked_against_their_language_catalog() {
    let dir = calendar("binding-diagnostics");
    let binding = dir.join("calendar/gregorian.nbind");
    let text = std::fs::read_to_string(&binding).unwrap();
    let params = json!({"textDocument":{"uri":uri(&binding)}});
    let messages = Session::run(
        &dir,
        vec![
            open(&binding, &text),
            request(5, "textDocument/documentSymbol", params),
        ],
    );
    assert_eq!(diagnostics(&messages, &binding), json!([]));
    let symbols = &response(&messages, 5)["result"];
    assert_eq!(symbols[0]["name"], "Calendar.Gregorian");

    // A stale version is reported where the binding writes it.
    let stale = text.replace("version 3", "version 2");
    let messages = Session::run(&dir, vec![open(&binding, &stale)]);
    let problems = diagnostics(&messages, &binding);
    assert_eq!(
        problems[0]["message"],
        "'Calendar.Gregorian' is version 3 in the catalog"
    );
    let line = stale.lines().position(|l| l.contains("version 2")).unwrap();
    let character = stale.lines().nth(line).unwrap().find('2').unwrap();
    assert_eq!(
        problems[0]["range"]["start"],
        json!({"line":line,"character":character})
    );

    // A binding no language lists is parsed, and says so.
    let lone = dir.join("calendar/lone.nbind");
    let lone_text = "binding X.Y version 1 for wasm component \"x.wasm\";";
    std::fs::write(&lone, lone_text).unwrap();
    let messages = Session::run(&dir, vec![open(&lone, lone_text)]);
    let problems = diagnostics(&messages, &lone);
    assert_eq!(problems[0]["message"], "no language lists this binding");
    assert_eq!(problems[0]["severity"], 2);

    let broken = "binding X.Y version for";
    let messages = Session::run(&dir, vec![open(&lone, broken)]);
    assert_eq!(diagnostics(&messages, &lone)[0]["severity"], 1);
}
