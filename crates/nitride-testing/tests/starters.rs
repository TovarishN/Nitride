use nitride_prepare::authoring::*;
#[test]
fn both_starter_presets_ship_passing_native_suites() {
    for kind in [TemplateKind::Syntax, TemplateKind::Typed] {
        let plan = plan_language(&TemplateOptions {
            name: "Calculator".into(),
            extension: ".calc".into(),
            kind,
        })
        .unwrap();
        assert!(
            plan.files
                .iter()
                .any(|f| f.relative_path == std::path::Path::new("tests.ntest"))
        );
        let root = std::env::temp_dir().join(format!(
            "nitride-starter-tests-{}-{kind:?}",
            std::process::id()
        ));
        create_language(&root, &plan).unwrap();
        let snapshot = nitride_testing::Snapshot::default();
        let result = nitride_testing::Runner::new(&snapshot).run(
            &root.join("tests.ntest"),
            &[],
            &mut |_| {},
        );
        assert_eq!(result.exit_code(), 0, "{}", result.json());
        std::fs::remove_dir_all(root).unwrap();
    }
}
