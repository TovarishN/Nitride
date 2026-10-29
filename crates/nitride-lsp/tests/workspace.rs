mod support;
use nitride_lsp::manifest::{self, Mode};
use nitride_lsp::registry::Registry;
use serde_json::json;
use std::path::{Path, PathBuf};
use support::*;

fn write(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

const GRAMMAR: &str =
    "syntax module M\n{\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Number*;\n}\n";

/// `dir/nitride.nproj` including `folders`.
fn workspace(dir: &Path, folders: &[&str]) -> PathBuf {
    let includes: String = folders.iter().map(|f| format!("  include {f};\n")).collect();
    write(dir, "nitride.nproj", &format!("workspace\n{{\n{includes}}}\n"));
    dir.join("nitride.nproj")
}

/// `dir/<folder>/<name>.nproj` holding `text`.
fn language(dir: &Path, folder: &str, name: &str, text: &str) -> PathBuf {
    let path = format!("{folder}/{name}.nproj");
    write(dir, &path, text);
    dir.join(path)
}

fn no_buffers(_: &Path) -> Option<String> {
    None
}

#[test]
fn nproj_wins_over_nitrogen_json() {
    let dir = scratch("prefers");
    write(&dir, "a/m.ngr", GRAMMAR);
    language(&dir, "a", "A", "language A { extensions .a; grammar m.ngr; start M.Program; }");
    let root = workspace(&dir, &["a"]);
    write(
        &dir,
        "nitrogen.json",
        r#"{"languages":[{"name":"B","extensions":[".b"],"grammars":["a/m.ngr"],"start":"M.Program"}]}"#,
    );
    let read = manifest::read(&dir, &no_buffers);
    let paths: Vec<_> = read.iter().map(|(p, _)| p.clone()).collect();
    assert_eq!(paths, [root, dir.join("a/A.nproj")]);
    let names: Vec<_> = read
        .into_iter()
        .flat_map(|(_, parsed)| parsed.unwrap().0)
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["A"]);
}

#[test]
fn falls_back_to_nitrogen_json() {
    let dir = scratch("fallback");
    write(
        &dir,
        "nitrogen.json",
        r#"{"languages":[{"name":"B","extensions":[".b"],"grammars":["m.ngr"],"start":"M.Program","catalog":"c.json","mode":"typed","sources":["x.cs"]}]}"#,
    );
    let read = manifest::read(&dir, &no_buffers);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].0, dir.join("nitrogen.json"));
    let entry = &read[0].1.as_ref().unwrap().0[0];
    assert_eq!(entry.mode, Mode::Syntax);
    assert!(entry.catalogs.is_empty());
    assert_eq!(entry.grammars, [dir.join("m.ngr")]);
}

#[test]
fn no_manifest_reads_nothing() {
    assert!(manifest::read(&scratch("empty"), &no_buffers).is_empty());
}

#[test]
fn typed_follows_catalog() {
    let dir = scratch("mode");
    write(&dir, "m.ngr", GRAMMAR);
    write(&dir, "c.ncatalog", "");
    language(&dir, "t", "T", "language T { extensions .t; grammar ../m.ngr; start M.Program; catalog ../c.ncatalog; }");
    language(&dir, "s", "S", "language S { extensions .s; grammar ../m.ngr; start M.Program; }");
    workspace(&dir, &["t", "s"]);
    let entries: Vec<_> = manifest::read(&dir, &no_buffers)
        .into_iter()
        .flat_map(|(_, parsed)| parsed.unwrap().0)
        .collect();
    let modes: Vec<_> = entries.iter().map(|e| e.mode).collect();
    assert_eq!(modes, [Mode::Typed, Mode::Syntax]);
    assert_eq!(entries[0].catalogs, [dir.join("t/../c.ncatalog")]);
}

#[test]
fn bad_manifest_reports_and_continues() {
    let bad = scratch("bad-manifest");
    let text = "workspace { include ";
    write(&bad, "nitride.nproj", text);
    let good = scratch("good-manifest");
    write(&good, "m/m.ngr", GRAMMAR);
    language(&good, "m", "M", "language M { extensions .m; grammar m.ngr; start M.Program; }");
    workspace(&good, &["m"]);

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
    input.extend(frame(&open(&bad.join("nitride.nproj"), text)));
    input.extend(frame(&request(2, "shutdown", serde_json::Value::Null)));
    input.extend(frame(&notification("exit", serde_json::Value::Null)));
    let messages = Session::raw(input);
    let diagnostics = diagnostics(&messages, &bad.join("nitride.nproj"));
    assert_eq!(diagnostics.as_array().unwrap().len(), 1, "{diagnostics}");
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 0);
}

#[test]
fn missing_grammar_is_reported_on_its_nproj() {
    let dir = scratch("missing-grammar");
    let file = language(&dir, "m", "M", "language M { extensions .m; grammar absent.ngr; start M.Program; }");
    workspace(&dir, &["m"]);
    let messages = Session::run(&dir, vec![]);
    let diagnostics = diagnostics(&messages, &file);
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .unwrap()
            .contains("absent.ngr"),
        "{diagnostics}"
    );
    assert_eq!(
        diagnostics[0]["range"]["start"],
        json!({"line":0,"character":36})
    );
}

#[test]
fn repo_manifest_loads() {
    let mut registry = Registry::default();
    registry.reload(&[repo()], &|_| None);
    // The 12 nitride.nproj languages, and languages/signed's signed Portable.
    assert_eq!(registry.languages().len(), 13);
    assert_eq!(registry.languages().iter().filter(|l| l.signed).count(), 1);
    for language in registry.languages() {
        assert!(
            language.current.is_some(),
            "{}: {:?}",
            language.entry.name,
            language.errors
        );
        assert!(language.errors.is_empty(), "{}", language.entry.name);
    }
    for manifest in registry.manifests() {
        assert!(manifest.errors.is_empty(), "{}: {:?}", manifest.path.display(), manifest.errors);
    }
}

const XML: &str = "language Xml { extensions .xml; grammar ../Xml.ngr; start Xml.Document; }";

/// A workspace of Xml and a second language `second` (folder `b`).
fn xml_folder(name: &str, second: &str) -> PathBuf {
    let dir = scratch(name);
    std::fs::copy(repo().join("languages/xml/Xml.ngr"), dir.join("Xml.ngr")).unwrap();
    language(&dir, "xml", "Xml", XML);
    language(&dir, "b", "B", second);
    workspace(&dir, &["xml", "b"]);
    dir
}

#[test]
fn one_bad_language_keeps_others() {
    let dir = xml_folder("bad-entry", "language B { extensions .b; grammar ../Xml.ngr; }");
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    assert!(
        registry
            .for_path(&dir.join("t.xml"))
            .unwrap()
            .current
            .is_some()
    );
    let b = registry
        .manifests()
        .iter()
        .find(|m| m.path == dir.join("b/B.nproj"))
        .unwrap();
    assert!(
        b.errors.iter().any(|e| e.message.contains("start rule")),
        "{:?}",
        b.errors
    );
}

#[test]
fn broken_nproj_keeps_previous_language() {
    let dir = xml_folder(
        "transient",
        "language B { extensions .b; grammar ../Xml.ngr; start Xml.Document; }",
    );
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    let file = dir.join("b/B.nproj");
    let typing = "language B {".to_owned();
    registry.reload(std::slice::from_ref(&dir), &|p| {
        (p == file).then(|| typing.clone())
    });
    assert!(
        registry
            .for_path(&dir.join("t.b"))
            .unwrap()
            .current
            .is_some()
    );
    let with_errors: Vec<_> = registry
        .manifests()
        .iter()
        .filter(|m| !m.errors.is_empty())
        .collect();
    assert_eq!(with_errors.len(), 1);
    assert_eq!(with_errors[0].path, file);
    assert_eq!(with_errors[0].errors.len(), 1);
}

#[test]
fn bad_include_reports_on_workspace_file() {
    let dir = scratch("bad-include");
    std::fs::copy(repo().join("languages/xml/Xml.ngr"), dir.join("Xml.ngr")).unwrap();
    language(&dir, "xml", "Xml", XML);
    let root = workspace(&dir, &["gone", "xml"]);
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    assert!(registry.for_path(&dir.join("t.xml")).unwrap().current.is_some());
    let errors = &registry
        .manifests()
        .iter()
        .find(|m| m.path == root)
        .unwrap()
        .errors;
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("no .nproj in gone"));
    assert_eq!(errors[0].span, [22, 26]);
}

#[test]
fn unknown_start_rule_is_reported_at_start() {
    let text = "language B { extensions .b; grammar ../Xml.ngr; start Xml.Nope; }";
    let dir = xml_folder("bad-start", text);
    let grammar = dir.join("Xml.ngr");
    let grammar_text = std::fs::read_to_string(&grammar).unwrap();
    let messages = Session::run(&dir, vec![open(&grammar, &grammar_text)]);
    let on_nproj = diagnostics(&messages, &dir.join("b/B.nproj"));
    assert!(
        on_nproj
            .to_string()
            .contains("unknown syntax start rule Xml.Nope"),
        "{on_nproj}"
    );
    let at = text.find("Xml.Nope").unwrap() as u64;
    assert_eq!(on_nproj[0]["range"]["start"], json!({"line":0,"character":at}));
    assert_eq!(on_nproj[0]["range"]["end"], json!({"line":0,"character":at + 8}));
    assert_eq!(diagnostics(&messages, &grammar), json!([]));
}

#[test]
fn language_extensions_lists_manifest_and_signed() {
    let dir = scratch("language-extensions");
    write(&dir, "m/m.ngr", GRAMMAR);
    language(&dir, "m", "M", "language M { extensions .m; grammar m.ngr; start M.Program; }");
    workspace(&dir, &["m"]);
    write(
        &dir,
        "Signed.syntax",
        "#!nitrogen grammar\nlanguage S { extensions \".sx\"; start M.Program; }\nsyntax module M\n{\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Number*;\n}\n",
    );
    let messages = Session::run(
        &dir,
        vec![request(5, "nitride/languageExtensions", json!({}))],
    );
    assert_eq!(response(&messages, 5)["result"], json!([".m", ".sx"]));
}
