mod support;
use nitride_lsp::manifest::Mode;
use nitride_lsp::registry::{Registry, discover};
use std::path::Path;
use support::*;

fn write(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

const CALC: &str = "#!nitrogen grammar\nlanguage Calc { start Calc.Program; extensions \".calc\"; }\nsyntax module Calc\n{\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Number*;\n}\n";

#[test]
fn discovers_signed_grammars() {
    let found = discover(&[repo()], &|_| None, &[]);
    assert!(
        found
            .iter()
            .any(|p| p.ends_with("languages/signed/Portable.syntax")),
        "{found:?}"
    );
    let mut registry = Registry::default();
    registry.reload(&[repo()], &|_| None);
    let signed = registry.signed("Portable");
    assert_eq!(signed.len(), 1);
    assert_eq!(signed[0].entry.mode, Mode::Typed);
    assert!(signed[0].current.is_some(), "{:?}", signed[0].errors);
}

#[test]
fn sidecar_catalog_makes_typed() {
    let dir = scratch("signed-sidecar");
    write(&dir, "Calc.syntax", CALC);
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    assert_eq!(registry.signed("Calc")[0].entry.mode, Mode::Syntax);
    assert!(
        registry.signed("Calc")[0].current.is_some(),
        "{:?}",
        registry.signed("Calc")[0].errors
    );
    std::fs::copy(
        repo().join("languages/portable/catalog.json"),
        dir.join("Calc.catalog.json"),
    )
    .unwrap();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    let calc = registry.signed("Calc")[0];
    assert_eq!(calc.entry.mode, Mode::Typed);
    assert_eq!(
        calc.entry.catalog.as_deref(),
        Some(dir.join("Calc.catalog.json").as_path())
    );
}

#[test]
fn missing_start_is_reported_on_declaration() {
    let dir = scratch("signed-no-start");
    write(
        &dir,
        "Calc.syntax",
        &CALC.replace("start Calc.Program; ", ""),
    );
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    let calc = registry.signed("Calc")[0];
    let error = calc.errors.first().expect("an error");
    assert_eq!(error.message, "signed language Calc needs a start rule");
    assert!(error.path.ends_with("Calc.syntax"));
    assert!(!error.project);
}

#[test]
fn manifest_and_signature_coexist() {
    let dir = scratch("signed-and-manifest");
    write(&dir, "Calc.syntax", CALC);
    manifest(
        &dir,
        "nitride.json",
        "Listed",
        ".listed",
        &[dir.join("Calc.syntax")],
        "Calc.Program",
        None,
    );
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    assert_eq!(registry.languages().len(), 2);
    assert!(
        registry
            .languages()
            .iter()
            .all(|l| l.current.is_some() && l.errors.is_empty())
    );
    // Manifest extensions come before signed ones.
    assert_eq!(
        registry.for_path(&dir.join("x.listed")).unwrap().entry.name,
        "Listed"
    );
    assert_eq!(
        registry.for_path(&dir.join("x.calc")).unwrap().entry.name,
        "Calc"
    );
}

#[test]
fn scan_bounds() {
    let dir = scratch("signed-bounds");
    for i in 0..2100 {
        write(&dir, &format!("noise/f{i:04}.txt"), "plain text\n");
    }
    write(&dir, "big.bin", &"x".repeat(2 * 1024 * 1024));
    write(&dir, "zz/Calc.syntax", CALC);
    write(
        &dir,
        "node_modules/Ignored.syntax",
        &CALC.replace("Calc", "Ignored"),
    );
    let started = std::time::Instant::now();
    let found = discover(std::slice::from_ref(&dir), &|_| None, &[]);
    assert_eq!(found, [dir.join("zz/Calc.syntax")]);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn open_buffers_are_discovered() {
    let dir = scratch("signed-unsaved");
    let unsaved = dir.join("New.syntax");
    let open = |p: &Path| (p == unsaved).then(|| CALC.to_owned());
    let found = discover(
        std::slice::from_ref(&dir),
        &open,
        std::slice::from_ref(&unsaved),
    );
    assert_eq!(found, [unsaved]);
}

#[test]
fn watched_changes_that_matter() {
    use nitride_lsp::registry::watched_change_matters;
    let dir = scratch("signed-watched");
    write(&dir, "Calc.syntax", CALC);
    write(
        &dir,
        "notes.md",
        "This mentions #!nitrogen grammar in prose.\n",
    );
    write(&dir, "code.rs", "fn main() {}\n");
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    // 1 created, 2 changed, 3 deleted (LSP FileChangeType).
    assert!(!watched_change_matters(&registry, &dir.join("code.rs"), 2));
    assert!(!watched_change_matters(&registry, &dir.join("notes.md"), 2));
    assert!(watched_change_matters(
        &registry,
        &dir.join("Calc.syntax"),
        2
    ));
    assert!(watched_change_matters(
        &registry,
        &dir.join("nitride.json"),
        1
    ));
    assert!(watched_change_matters(&registry, &dir.join("Other.ngr"), 2));
    assert!(watched_change_matters(
        &registry,
        &dir.join("gone.syntax"),
        3
    ));
    assert!(!watched_change_matters(&registry, &dir.join("gone.rs"), 3));
}

#[test]
fn may_declare_tolerates_spacing() {
    assert!(nitride_lsp::signed::declares_grammar(
        "#!nitrogen\tgrammar\nlanguage A { start A.P; }"
    ));
    assert!(!nitride_lsp::signed::declares_grammar(
        "prose about #!nitrogen grammar\n"
    ));
}
