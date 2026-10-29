use nitride_prepare::{prepare_project, write_prepared};
use std::path::PathBuf;

fn directory() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "nitride-prepare-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
fn fixture(path: &std::path::Path) {
    std::fs::write(
        path.join("Test.ngr"),
        "syntax module Test { syntax Start = \"x\"; }",
    )
    .unwrap();
    std::fs::write(
        path.join("project.json"),
        r#"{"mode":"syntax","start":"Test.Start","grammars":["Test.ngr"]}"#,
    )
    .unwrap();
}
#[test]
fn project_is_relocatable_and_output_is_validated_before_replacement() {
    let a = directory();
    let b = directory();
    fixture(&a);
    fixture(&b);
    let first = prepare_project(&a.join("project.json")).unwrap();
    assert_eq!(
        first.data,
        prepare_project(&b.join("project.json")).unwrap().data
    );
    assert_eq!(first.inputs.len(), 2);
    let output = a.join("language.json");
    std::fs::write(&output, "previous").unwrap();
    write_prepared(&first, &output).unwrap();
    let saved = std::fs::read(&output).unwrap();
    std::fs::write(
        a.join("Test.ngr"),
        "syntax module Test { syntax Start = Missing; }",
    )
    .unwrap();
    assert!(prepare_project(&a.join("project.json")).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), saved);
    for input in &first.inputs {
        assert!(write_prepared(&first, input).is_err());
    }
    std::fs::remove_dir_all(a).unwrap();
    std::fs::remove_dir_all(b).unwrap();
}
#[test]
fn invalid_manifest_and_missing_files_report_paths() {
    let path = directory();
    fixture(&path);
    std::fs::remove_file(path.join("Test.ngr")).unwrap();
    assert!(
        prepare_project(&path.join("project.json"))
            .unwrap_err()
            .path
            .ends_with("Test.ngr")
    );
    for text in [
        "{",
        r#"{"grammars":[],"start":"Test.Start"}"#,
        r#"{"grammars":[3],"start":"Test.Start"}"#,
    ] {
        std::fs::write(path.join("project.json"), text).unwrap();
        assert!(
            prepare_project(&path.join("project.json"))
                .unwrap_err()
                .path
                .ends_with("project.json")
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn catalogs_are_inputs_and_their_grammar_names_are_checked() {
    let path = directory();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/portable");
    std::fs::copy(root.join("Portable.ngr"), path.join("Portable.ngr")).unwrap();
    let catalog = std::fs::read_to_string(root.join("Portable.ncatalog")).unwrap();
    std::fs::write(path.join("project.json"), r#"{"mode":"typed","start":"Portable.Program","grammars":["Portable.ngr"],"catalogs":["Portable.ncatalog"]}"#).unwrap();
    for (text, message) in [
        (
            catalog.replace("show Show(Value)", "show Show(Missing)"),
            "'Portable.Show' has no field 'Missing'",
        ),
        (
            catalog.replace("show Show(Value)", "show Shown(Value)"),
            "unknown grammar rule 'Portable.Shown'",
        ),
    ] {
        std::fs::write(path.join("Portable.ncatalog"), &text).unwrap();
        let error = prepare_project(&path.join("project.json")).unwrap_err();
        assert_eq!(error.message, message);
        assert!(error.path.ends_with("Portable.ncatalog"), "{}", error.path);
        assert_eq!(
            &text[error.span[0]..error.span[1]],
            if message.contains("field") {
                "Missing"
            } else {
                "Shown"
            }
        );
    }
    std::fs::write(path.join("Portable.ncatalog"), &catalog).unwrap();
    let prepared = prepare_project(&path.join("project.json")).unwrap();
    assert_eq!(prepared.inputs.len(), 3);
    std::fs::write(
        path.join("project.json"),
        r#"{"start":"Portable.Program","grammars":["Portable.ngr"],"catalog":"catalog.json"}"#,
    )
    .unwrap();
    assert_eq!(
        prepare_project(&path.join("project.json"))
            .unwrap_err()
            .message,
        "'catalog' is replaced by 'catalogs', a list of .ncatalog files"
    );
    std::fs::remove_dir_all(path).unwrap();
}
