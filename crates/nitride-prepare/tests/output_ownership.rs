use nitride_prepare::{prepare_project, write_prepared};

#[test]
fn colliding_temporary_file_is_never_deleted() {
    let path =
        std::env::temp_dir().join(format!("nitride-output-ownership-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
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
    let project = prepare_project(&path.join("project.json")).unwrap();
    // This integration-test process has made no previous write_prepared call.
    let occupied = path.join(format!(".nitride-prepare-{}-0.tmp", std::process::id()));
    std::fs::write(&occupied, "belongs to someone else").unwrap();
    assert!(write_prepared(&project, &path.join("language.json")).is_err());
    assert_eq!(
        std::fs::read_to_string(&occupied).unwrap(),
        "belongs to someone else"
    );
    assert!(!path.join("language.json").exists());
    std::fs::remove_dir_all(path).unwrap();
}
