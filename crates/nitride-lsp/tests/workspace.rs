mod support;
use nitride_lsp::manifest::{self, Mode};
use nitride_lsp::registry::Registry;
use serde_json::json;
use std::path::Path;
use support::*;

fn write(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

const GRAMMAR: &str =
    "syntax module M\n{\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Number*;\n}\n";

#[test]
fn prefers_nitride_json() {
    let dir = scratch("prefers");
    write(
        &dir,
        "nitride.json",
        r#"{"languages":[{"name":"A","extensions":[".a"],"grammars":["m.ngr"],"start":"M.Program"}]}"#,
    );
    write(
        &dir,
        "nitrogen.json",
        r#"{"languages":[{"name":"B","extensions":[".b"],"grammars":["m.ngr"],"start":"M.Program"}]}"#,
    );
    let (path, entries) = manifest::read(&dir);
    assert_eq!(path.unwrap(), dir.join("nitride.json"));
    assert_eq!(entries.unwrap().0[0].name, "A");
}

#[test]
fn falls_back_to_nitrogen_json() {
    let dir = scratch("fallback");
    write(
        &dir,
        "nitrogen.json",
        r#"{"languages":[{"name":"B","extensions":[".b"],"grammars":["m.ngr"],"start":"M.Program","catalog":"c.json","mode":"typed","sources":["x.cs"]}]}"#,
    );
    let (path, entries) = manifest::read(&dir);
    assert_eq!(path.unwrap(), dir.join("nitrogen.json"));
    let entry = &entries.unwrap().0[0];
    assert_eq!(entry.mode, Mode::Syntax);
    assert_eq!(entry.catalog, None);
    assert_eq!(entry.grammars, [dir.join("m.ngr")]);
}

#[test]
fn mode_defaults_from_catalog() {
    let dir = scratch("mode");
    write(
        &dir,
        "nitride.json",
        r#"{"languages":[
        {"name":"T","extensions":[".t"],"grammars":["m.ngr"],"start":"M.Program","catalog":"c.json"},
        {"name":"S","extensions":[".s"],"grammars":["m.ngr"],"start":"M.Program"},
        {"name":"E","extensions":[".e"],"grammars":["m.ngr"],"start":"M.Program","catalog":"c.json","mode":"syntax"}]}"#,
    );
    let entries = manifest::read(&dir).1.unwrap().0;
    let modes: Vec<_> = entries.iter().map(|e| e.mode).collect();
    assert_eq!(modes, [Mode::Typed, Mode::Syntax, Mode::Syntax]);
    assert_eq!(entries[0].catalog, Some(dir.join("c.json")));
}

#[test]
fn bad_manifest_reports_and_continues() {
    let bad = scratch("bad-manifest");
    let text = r#"{"languages":[],}"#;
    write(&bad, "nitride.json", text);
    let good = scratch("good-manifest");
    write(
        &good,
        "nitride.json",
        r#"{"languages":[{"name":"M","extensions":[".m"],"grammars":["m.ngr"],"start":"M.Program"}]}"#,
    );
    write(&good, "m.ngr", GRAMMAR);

    let mut registry = Registry::default();
    registry.reload(&[bad.clone(), good.clone()], &|_| None);
    assert!(
        registry
            .for_path(&good.join("x.m"))
            .unwrap()
            .current
            .is_some()
    );

    let mut init = initialize(1, &bad);
    init["params"]["workspaceFolders"] =
        json!([{"uri":uri(&bad),"name":"bad"},{"uri":uri(&good),"name":"good"}]);
    let mut input = frame(&init);
    input.extend(frame(&open(&bad.join("nitride.json"), text)));
    input.extend(frame(&request(2, "shutdown", serde_json::Value::Null)));
    input.extend(frame(&notification("exit", serde_json::Value::Null)));
    let messages = Session::raw(input);
    let diagnostics = diagnostics(&messages, &bad.join("nitride.json"));
    assert_eq!(diagnostics.as_array().unwrap().len(), 1);
    // serde_json reports the trailing comma at the closing brace.
    assert_eq!(
        diagnostics[0]["range"]["start"],
        json!({"line":0,"character":16})
    );
}

#[test]
fn missing_grammar_is_manifest_diagnostic() {
    let dir = scratch("missing-grammar");
    write(
        &dir,
        "nitride.json",
        r#"{"languages":[{"name":"M","extensions":[".m"],"grammars":["absent.ngr"],"start":"M.Program"}]}"#,
    );
    let messages = Session::run(&dir, vec![]);
    let diagnostics = diagnostics(&messages, &dir.join("nitride.json"));
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .unwrap()
            .contains("absent.ngr")
    );
}

#[test]
fn repo_manifest_loads() {
    let mut registry = Registry::default();
    registry.reload(&[repo()], &|_| None);
    assert_eq!(registry.languages().len(), 12);
    for language in registry.languages() {
        assert!(
            language.current.is_some(),
            "{}: {:?}",
            language.entry.name,
            language.errors
        );
        assert!(language.errors.is_empty(), "{}", language.entry.name);
    }
}

const XML: &str =
    r#"{"name":"Xml","extensions":[".xml"],"grammars":["Xml.ngr"],"start":"Xml.Document"}"#;

fn xml_folder(name: &str, second: &str) -> std::path::PathBuf {
    let dir = scratch(name);
    std::fs::copy(repo().join("languages/xml/Xml.ngr"), dir.join("Xml.ngr")).unwrap();
    write(
        &dir,
        "nitride.json",
        &format!(r#"{{"languages":[{XML},{second}]}}"#),
    );
    dir
}

#[test]
fn one_bad_entry_keeps_others() {
    let dir = xml_folder(
        "bad-entry",
        r#"{"name":"A","extensions":[".a"],"grammars":["Xml.ngr"]}"#,
    );
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    assert!(
        registry
            .for_path(&dir.join("t.xml"))
            .unwrap()
            .current
            .is_some()
    );
    let errors = &registry.manifests()[0].errors;
    assert!(
        errors.iter().any(|e| e.message.contains("\"start\"")),
        "{errors:?}"
    );
}

#[test]
fn invalid_manifest_buffer_keeps_languages() {
    let dir = xml_folder(
        "transient",
        r#"{"name":"B","extensions":[".b"],"grammars":["Xml.ngr"],"start":"Xml.Document"}"#,
    );
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    let manifest = dir.join("nitride.json");
    let typing = r#"{"languages":[, {"#.to_owned();
    registry.reload(std::slice::from_ref(&dir), &|p| {
        (p == manifest).then(|| typing.clone())
    });
    assert!(
        registry
            .for_path(&dir.join("t.xml"))
            .unwrap()
            .current
            .is_some()
    );
    assert_eq!(registry.manifests()[0].errors.len(), 1);
}

#[test]
fn unknown_start_rule_is_manifest_diagnostic() {
    let dir = xml_folder(
        "bad-start",
        r#"{"name":"A","extensions":[".a"],"grammars":["Xml.ngr"],"start":"Xml.Nope"}"#,
    );
    let grammar = dir.join("Xml.ngr");
    let text = std::fs::read_to_string(&grammar).unwrap();
    let messages = Session::run(&dir, vec![open(&grammar, &text)]);
    let on_manifest = diagnostics(&messages, &dir.join("nitride.json"));
    assert!(
        on_manifest
            .to_string()
            .contains("unknown syntax start rule Xml.Nope"),
        "{on_manifest}"
    );
    assert_eq!(diagnostics(&messages, &grammar), json!([]));
}
