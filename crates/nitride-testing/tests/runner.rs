use nitride_testing::{Runner, Snapshot, discover, values_equal};
use serde_json::json;
#[test]
fn real_runner_observes_phases_values_and_unsaved_inputs() {
    let root = std::env::temp_dir().join(format!("nitride-test-runner-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let plan =
        nitride_prepare::authoring::plan_language(&nitride_prepare::authoring::TemplateOptions {
            name: "Calculator".into(),
            extension: ".calc".into(),
            kind: nitride_prepare::authoring::TemplateKind::Typed,
        })
        .unwrap();
    let target = root.join("Calculator");
    nitride_prepare::authoring::create_language(&target, &plan).unwrap();
    let suite = target.join("tests.ntest");
    std::fs::write(&suite,"tests version 1 { project \"nitride.nproj\"; test \"run\" {source \"let answer = 42; show answer;\";expect run values [42];} test \"reject\" {source \"let = ;\"; expect parse reject;} test \"wrong phase\" {source \"let = ;\";expect compile reject;} }").unwrap();
    let snapshot = Snapshot::default();
    let discovery = discover(&suite, &snapshot).unwrap();
    assert_eq!(discovery.suite.tests.len(), 3);
    let mut runner = Runner::new(&snapshot);
    let report = runner.run(&suite, &[], &mut |_| {});
    assert_eq!(report.exit_code(), 1);
    assert_eq!(report.results[0]["status"], "passed");
    assert_eq!(report.results[1]["status"], "passed");
    assert_eq!(report.results[2]["status"], "failed");
    let grammar = target.join("Calculator.ngr").canonicalize().unwrap();
    let text = std::fs::read_to_string(&grammar)
        .unwrap()
        .replace("\"let\"", "\"bind\"");
    let snapshot =
        Snapshot::from_json(&json!({"version":1,"files":[{"path":grammar,"text":text}]})).unwrap();
    let mut runner = Runner::new(&snapshot);
    let report = runner.run(&suite, &["run".into()], &mut |_| {});
    assert_eq!(report.results[0]["status"], "failed");
    assert!(
        std::fs::read_to_string(grammar)
            .unwrap()
            .contains("\"let\"")
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn numeric_comparison_never_rounds_distinct_large_integers() {
    assert!(values_equal(&json!([42]), &json!([42.0])));
    assert!(!values_equal(
        &json!(9007199254740993i64),
        &json!(9007199254740992i64)
    ));
    assert!(!values_equal(
        &json!(9007199254740993i64),
        &json!(9007199254740992.0)
    ));
    assert!(!values_equal(&json!("42"), &json!(42)));
}
#[test]
fn snapshots_reject_duplicate_and_library_identities() {
    assert!(
        Snapshot::from_json(&json!({"version":1,"files":[{"path":"nitride-lib:/x","text":"x"}]}))
            .is_err()
    );
    assert!(Snapshot::from_json(&json!({"version":2,"files":[]})).is_err());
}

#[test]
fn diagnostic_matching_and_infrastructure_are_distinct() {
    let root =
        std::env::temp_dir().join(format!("nitride-test-diagnostics-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let target = root.join("Language");
    let plan =
        nitride_prepare::authoring::plan_language(&nitride_prepare::authoring::TemplateOptions {
            name: "Language".into(),
            extension: ".test".into(),
            kind: nitride_prepare::authoring::TemplateKind::Typed,
        })
        .unwrap();
    nitride_prepare::authoring::create_language(&target, &plan).unwrap();
    let path = target.join("tests.ntest");
    std::fs::write(&path,"tests version 1 {project \"nitride.nproj\";test \"unknown\"{source \"show missing;\";expect compile reject;}test \"isolated\"{source \"show missing;\";expect run reject;}}").unwrap();
    let snapshot = Snapshot::default();
    let result = Runner::new(&snapshot).run(&path, &[], &mut |_| {});
    assert_eq!(result.results[0]["status"], "passed");
    let diagnostic = &result.results[0]["actual"]["diagnostics"][0];
    let expected = format!(
        "tests version 1 {{project \"nitride.nproj\";test \"unknown\"{{source \"show missing;\";expect compile reject {{diagnostic {} code none span [{},{}];}}}}}}",
        diagnostic["category"], diagnostic["span"][0], diagnostic["span"][1]
    );
    std::fs::write(&path, expected).unwrap();
    let snapshot = Snapshot::default();
    let result = Runner::new(&snapshot).run(&path, &[], &mut |_| {});
    assert_eq!(result.exit_code(), 0);
    std::fs::write(target.join("Language.ngr"), "bad grammar").unwrap();
    let snapshot = Snapshot::default();
    let result = Runner::new(&snapshot).run(&path, &[], &mut |_| {});
    assert_eq!(result.exit_code(), 2);
    assert_eq!(result.results[0]["status"], "errored");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn disk_inputs_are_immutable_after_first_snapshot_read() {
    let root = std::env::temp_dir().join(format!("nitride-snapshot-freeze-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let file = root.join("source");
    std::fs::write(&file, "old").unwrap();
    let snapshot = Snapshot::default();
    assert_eq!(snapshot.read(&file).unwrap(), "old");
    std::fs::write(&file, "new").unwrap();
    assert_eq!(snapshot.read(&file).unwrap(), "old");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn editor_snapshot_roots_are_enforced_before_reading_project_inputs() {
    let root =
        std::env::temp_dir().join(format!("nitride-test-root-policy-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let outside = root.with_extension("outside");
    std::fs::write(&outside, "private input").unwrap();
    let snapshot = Snapshot::from_json(&json!({"version":1,"files":[],"roots":[root]})).unwrap();
    let error = snapshot.read(&outside).unwrap_err();
    assert!(error.to_string().contains("outside"));
    std::fs::remove_file(outside).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
