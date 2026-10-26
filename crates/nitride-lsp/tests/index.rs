mod support;
use nitride_lsp::analysis::{Analysis, analyze};
use nitride_lsp::registry::Registry;
use std::path::{Path, PathBuf};
use support::*;

fn lang(path: &str) -> PathBuf {
    repo().join("languages").join(path)
}

fn analysis(dir: &Path, file: &str, text: &str) -> Analysis {
    let mut registry = Registry::default();
    registry.reload(&[dir.to_owned()], &|_| None);
    analyze(&registry, &dir.join(file), text, &|_| None).unwrap()
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

/// (use text, target definition's name start) for every use.
fn uses(analysis: &Analysis, text: &str) -> Vec<(String, u32)> {
    let index = &analysis.index;
    index
        .references
        .iter()
        .map(|u| {
            (
                text[u.span[0] as usize..u.span[1] as usize].to_owned(),
                index.definitions[u.target].name_span[0],
            )
        })
        .collect()
}

#[test]
fn typed_uses_resolve() {
    let text = "let a=1;\nsum(a,a);";
    let analysis = analysis(&calls("index-typed"), "x.calls", text);
    assert_eq!(uses(&analysis, text), [("a".into(), 4), ("a".into(), 4)]);
    assert_eq!(
        analysis.index.definitions[0].ty.as_deref(),
        Some("Core.Scalar")
    );
}

#[test]
fn redeclaration_follows_engine() {
    let text = "let x=1;\nlet x=x;\nx;";
    let analysis = analysis(&calls("index-redeclare"), "x.calls", text);
    assert_eq!(uses(&analysis, text), [("x".into(), 4), ("x".into(), 13)]);
}

#[test]
fn builtins_recorded() {
    let analysis = analysis(&calls("index-builtins"), "x.calls", "sum(1,2);");
    let builtin = &analysis.index.builtins[0];
    assert_eq!(
        (builtin.name.as_str(), builtin.kind.as_str(), builtin.span),
        ("sum", "function", [0, 3])
    );
}

#[test]
fn fallback_binds_damaged_program() {
    let text = "let a=1;\nlet b=;\nsum(a,a);";
    let analysis = analysis(&calls("index-damaged"), "x.calls", text);
    assert_eq!(uses(&analysis, text), [("a".into(), 4), ("a".into(), 4)]);
}

#[test]
fn syntax_only_binds_by_name() {
    let dir = scratch("index-syntax");
    manifest(
        &dir,
        "nitrogen.json",
        "Calls",
        ".calls",
        &[lang("calls/Calls.Reference.ngr")],
        "Calls.Program",
        None,
    );
    let text = "let a=1;\nsum(a,a);";
    let analysis = analysis(&dir, "x.calls", text);
    assert_eq!(uses(&analysis, text), [("a".into(), 4), ("a".into(), 4)]);
    assert_eq!(analysis.index.definitions[0].ty, None);
}
