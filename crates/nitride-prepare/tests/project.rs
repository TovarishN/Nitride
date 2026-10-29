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

fn nproj(path: &std::path::Path, text: &str) -> std::path::PathBuf {
    let file = path.join("Test.nproj");
    std::fs::write(&file, text).unwrap();
    file
}

#[test]
fn nproj_project_prepares_like_json() {
    let path = directory();
    fixture(&path);
    let file = nproj(&path, "language Test { grammar Test.ngr; start Test.Start; }");
    let from_nproj = prepare_project(&file).unwrap();
    assert_eq!(
        from_nproj.data,
        prepare_project(&path.join("project.json")).unwrap().data
    );
    assert_eq!(from_nproj.inputs.len(), 2);
    assert!(from_nproj.inputs[0].ends_with("Test.nproj"));
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn nproj_typed_with_catalog_prepares_like_json() {
    let path = directory();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/dates");
    for file in ["Dates.ngr", "Dates.ncatalog"] {
        std::fs::copy(root.join(file), path.join(file)).unwrap();
    }
    std::fs::write(path.join("project.json"), r#"{"mode":"typed","start":"Dates.Program","grammars":["Dates.ngr"],"catalogs":["Dates.ncatalog"]}"#).unwrap();
    let file = nproj(&path, "language Dates { grammar Dates.ngr; start Dates.Program; catalog Dates.ncatalog; }");
    assert_eq!(
        prepare_project(&file).unwrap().data,
        prepare_project(&path.join("project.json")).unwrap().data
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn nproj_unknown_start_points_at_start() {
    let path = directory();
    fixture(&path);
    let text = "language Test { grammar Test.ngr; start Test.Nope; }";
    let error = prepare_project(&nproj(&path, text)).unwrap_err();
    assert!(error.path.ends_with("Test.nproj"), "{error}");
    assert_eq!(&text[error.span[0]..error.span[1]], "Test.Nope");
    assert!(error.message.contains("start rule"), "{error}");
    assert!(error.project);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn nproj_errors_surface() {
    let path = directory();
    fixture(&path);
    let text = "language Test { grammar Gone.ngr; start Test.Start; }";
    let error = prepare_project(&nproj(&path, text)).unwrap_err();
    assert!(error.path.ends_with("Test.nproj"), "{error}");
    assert_eq!(&text[error.span[0]..error.span[1]], "Gone.ngr");
    std::fs::remove_dir_all(path).unwrap();
}
