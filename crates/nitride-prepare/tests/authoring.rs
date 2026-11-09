use nitride_prepare::authoring::*;
use std::path::Path;
fn options(kind: TemplateKind) -> TemplateOptions {
    TemplateOptions {
        name: "Calculator".into(),
        extension: ".calc".into(),
        kind,
    }
}
#[test]
fn templates_prepare_and_parse_using_the_pinned_library() {
    for kind in [TemplateKind::Syntax, TemplateKind::Typed] {
        let plan = plan_language(&options(kind)).unwrap();
        let parent = std::env::temp_dir().join(format!(
            "nitride-template-{}-{:?}",
            std::process::id(),
            kind
        ));
        std::fs::create_dir(&parent).unwrap();
        let target = parent.join("Calculator");
        let report = create_language(&target, &plan).unwrap();
        assert_eq!(report.created.len(), plan.files.len());
        let prepared = nitride_prepare::prepare_project(&target.join("nitride.nproj")).unwrap();
        assert_eq!(prepared.libraries[0].version, "1.0.0");
        let loaded = nitride_codegen::LoadedLanguage::load(&prepared.data.to_string()).unwrap();
        nitride_core::parse(
            loaded.language(),
            &std::fs::read_to_string(target.join("sample.calc")).unwrap(),
            nitride_core::Limits::default(),
        )
        .unwrap();
        assert!(create_language(&target, &plan).is_err());
        std::fs::remove_dir_all(parent).unwrap();
    }
}
#[test]
fn planner_rejects_names_extensions_and_unsafe_files() {
    for name in ["../escape", "é", "with space", "a.b", ""] {
        let mut o = options(TemplateKind::Typed);
        o.name = name.into();
        assert!(plan_language(&o).is_err());
    }
    for extension in ["calc", "../calc", ".a/b", "."] {
        let mut o = options(TemplateKind::Typed);
        o.extension = extension.into();
        assert!(plan_language(&o).is_err());
    }
    let mut plan = plan_language(&options(TemplateKind::Syntax)).unwrap();
    plan.files[0].relative_path = "../escape".into();
    assert!(create_language(Path::new("/tmp/not-created-authoring"), &plan).is_err());
}

#[cfg(unix)]
#[test]
fn editor_target_rejects_symlinked_ancestors_and_traversal() {
    let root = std::env::temp_dir().join(format!("nitride-target-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    std::os::unix::fs::symlink(std::env::temp_dir(), root.join("escape")).unwrap();
    assert!(workspace_target(&root.join("escape/Project"), std::slice::from_ref(&root)).is_err());
    assert!(workspace_target(&root.join("../Project"), std::slice::from_ref(&root)).is_err());
    assert!(workspace_target(&root.join("Project"), std::slice::from_ref(&root)).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_file_issues_are_structured_and_scaffolds_are_valid() {
    use nitride_prepare::Source;
    let source = Source {
        path: "/virtual/nitride.nproj".into(),
        text: "language Test { grammar Test.ngr; catalog Test.ncatalog; start Test.Program; }"
            .into(),
    };
    let issues = project_issues(&source, &|_| false);
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].code, "NPROJ0001");
    let grammar = missing_file_scaffold(
        &issues[0].kind,
        issues[0].target.as_ref().unwrap(),
        Some("Test.Program"),
    )
    .unwrap();
    assert!(nitride_prepare::grammar_parses(&Source {
        path: "Test.ngr".into(),
        text: grammar
    }));
    let catalog =
        missing_file_scaffold(&issues[1].kind, issues[1].target.as_ref().unwrap(), None).unwrap();
    assert!(nitride_prepare::catalog_parses(&Source {
        path: "Test.ncatalog".into(),
        text: catalog
    }));
}
