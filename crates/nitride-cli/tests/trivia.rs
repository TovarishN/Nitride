use nitride_codegen::{LoadedLanguage, RuntimeSnapshot};
use nitride_core::*;
use std::sync::Arc;

fn prepared() -> serde_json::Value {
    nitride_prepare::prepare_project(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/json/project.json")).unwrap().data
}

#[test]
fn strict_json_rejects_comments_and_non_json_whitespace_on_every_runtime_path() {
    let mut data = prepared();
    data["trivia"] = "json".into();
    let loaded = Arc::new(LoadedLanguage::load(&data.to_string()).unwrap());
    for source in ["[1 // c\n]", "[1 /* c */]", "[1,\u{a0}2]", "[1,\u{b}2]", "[1,\u{c}2]", "[1,\u{2003}2]"] {
        assert!(parse(loaded.language(), source, Limits::default()).is_err(), "{source:?}");
        assert!(RuntimeSnapshot::parse(loaded.clone(), source, Limits::default(), SnapshotMode::Strict(Default::default())).is_err(), "{source:?}");
        let recovered = RuntimeSnapshot::parse(loaded.clone(), source, Limits::default(), SnapshotMode::Recovering(Default::default())).unwrap();
        assert!(!recovered.diagnostics().is_empty(), "{source:?}");
    }
    for source in [" \t\r\n[1, 2]\r\n", r#"["//","/*"," "," "]"#] {
        assert!(parse(loaded.language(), source, Limits::default()).is_ok(), "{source:?}");
    }
}

#[test]
fn strict_policy_survives_runtime_edits_and_default_comments_stay_supported() {
    let mut data = prepared();
    data["trivia"] = "json".into();
    let loaded = Arc::new(LoadedLanguage::load(&data.to_string()).unwrap());
    let first = RuntimeSnapshot::parse(loaded, "[1,2]", Limits::default(), SnapshotMode::Recovering(Default::default())).unwrap();
    let broken = first.edit(&TextEdit { range: [3,3], replacement: "/*x*/".into() }, Limits::default()).unwrap();
    assert!(!broken.diagnostics().is_empty());
    let fixed = broken.edit(&TextEdit { range: [3,8], replacement: String::new() }, Limits::default()).unwrap();
    assert!(fixed.diagnostics().is_empty());
    data.as_object_mut().unwrap().remove("trivia");
    let default = LoadedLanguage::load(&data.to_string()).unwrap();
    assert!(parse(default.language(), "[1, /*x*/ 2]", Limits::default()).is_ok());
}

#[test]
fn unknown_policy_is_not_silently_ignored() {
    let mut data = prepared();
    data["trivia"] = "typo".into();
    assert!(LoadedLanguage::load(&data.to_string()).is_err());
}

#[test]
fn generated_json_matches_runtime_policy_and_pinned_conformance_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/json-testsuite/test_parsing");
    let mut accepted = 0;
    let mut rejected = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if !name.starts_with("y_") && !name.starts_with("n_") { continue; }
        let bytes = std::fs::read(&path).unwrap();
        let result = std::str::from_utf8(&bytes).ok().and_then(|source| {
            parse_snapshot_generated(&nitride_cli::json_language::PARSER, source, Limits::default(), SnapshotMode::Strict(Default::default())).ok()
        });
        let valid = result.as_ref().is_some_and(|s| s.diagnostics().is_empty());
        if name.starts_with("y_") { accepted += 1; assert!(valid, "rejected {name}"); }
        else { rejected += 1; assert!(!valid, "accepted {name}"); }
    }
    assert!(accepted > 90 && rejected > 180, "incomplete conformance corpus");
    for source in ["[1 /*x*/]", "[1, 2]", " \r\n[1,2]\t"] {
        let generated = parse_snapshot_generated(&nitride_cli::json_language::PARSER, source, Limits::default(), SnapshotMode::Strict(Default::default())).is_ok();
        let loaded = LoadedLanguage::load(&prepared().to_string()).unwrap();
        assert_eq!(generated, parse(loaded.language(), source, Limits::default()).is_ok());
    }
}
