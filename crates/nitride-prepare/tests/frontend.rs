use nitride_prepare::{Source, prepare_sources};
use serde_json::Value;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn existing_exports_match_native_frontend() {
    for profile in [
        "portable",
        "power",
        "composition",
        "xml",
        "namespaces",
        "inference",
        "calls",
    ] {
        let directory = root().join("languages").join(profile);
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(directory.join("project.json")).unwrap())
                .unwrap();
        let sources: Vec<_> = manifest["grammars"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let path = directory.join(p.as_str().unwrap());
                Source {
                    path: path.display().to_string(),
                    text: std::fs::read_to_string(path).unwrap(),
                }
            })
            .collect();
        let catalog = std::fs::read_to_string(directory.join("catalog.json")).unwrap();
        let result = prepare_sources(
            &sources,
            &catalog,
            manifest["start"].as_str().unwrap(),
            manifest["mode"].as_str().unwrap(),
        )
        .unwrap();
        let mut expected: Value = serde_json::from_str(
            &std::fs::read_to_string(directory.join("language.json")).unwrap(),
        )
        .unwrap();
        normalize_legacy_classes(&mut expected);
        for (key, value) in expected.as_object().unwrap() {
            if key == "rules" {
                for (actual, want) in result[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .zip(value.as_array().unwrap())
                {
                    assert_eq!(actual, want, "{profile}: {}", want["name"]);
                }
                assert_eq!(
                    result[key].as_array().unwrap().len(),
                    value.as_array().unwrap().len()
                );
            } else {
                assert_eq!(&result[key], value, "{profile}: {key}");
            }
        }
    }
}

fn prepare(text: &str) -> Result<Value, nitride_prepare::PrepareError> {
    prepare_sources(
        &[Source {
            path: "test.ngr".into(),
            text: text.into(),
        }],
        "{\"types\":[],\"operations\":[],\"statements\":{}}",
        "Test.Start",
        "syntax",
    )
}

#[test]
fn malformed_grammar_reports_utf8_source_span() {
    let source = "// café 😀\nsyntax module Test { syntax Start = ; }";
    let error = prepare(source).unwrap_err();
    assert_eq!(error.path, "test.ngr");
    assert_eq!(error.span[0], source.find(';').unwrap());
}

#[test]
fn malformed_and_unadmitted_constructs_are_rejected() {
    for text in [
        "syntax module Test { syntax Start = Missing; }",
        "syntax module Test { using Missing; syntax Start = \"x\"; }",
        "syntax module Test { syntax Start = Start \"x\" / \"y\"; }",
        "syntax module Test { token N = Start; syntax Start = \"x\"; }",
        "syntax module Test { syntax Start = \"x\"; syntax Start = \"y\"; }",
        "syntax module Test { syntax Start = \"\"; }",
        "syntax module Test { syntax Start = ['z'..'a']; }",
        "syntax module Test { syntax Start = (\"x\"?)*; }",
        "syntax module Test { syntax Start = E; extensible syntax E { | N = \"x\"? } }",
        "syntax module Test { token T = \"x\"?; syntax Start = T; }",
        "syntax module Test { syntax Start = \"x\" rewrites <[x]>; }",
        "syntax module Test { syntax Start = E; extensible syntax E { | Num = \"x\" | Add = E \"+\" E precedence 256 } }",
        "syntax module Test { syntax Start = \"x\"; } /* unterminated",
    ] {
        assert!(prepare(text).is_err(), "{text}");
    }
}

#[test]
fn grammar_depth_is_bounded() {
    let source = format!(
        "syntax module Test {{ syntax Start = {}\"x\"{}; }}",
        "(".repeat(300),
        ")".repeat(300)
    );
    assert!(prepare(&source).unwrap_err().message.contains("nesting"));
}

#[test]
fn duplicate_sequence_labels_fail_but_choice_labels_remain_valid() {
    let error = prepare("syntax module Test { syntax Start = X:\"a\" X:\"b\"; }").unwrap_err();
    assert!(error.message.contains("duplicate label"));
    assert!(prepare("syntax module Test { syntax Start = X:\"a\" / X:\"b\"; }").is_ok());
}

#[test]
fn omitted_declarative_clauses_are_validated_before_discarding() {
    for clause in [
        "references foo X",
        "declares foo X sequential type Y",
        "lowers literal Core.Scalar X",
        "lowers Op(X)",
    ] {
        let text =
            format!("syntax module Test {{ symbols {{ foo }} syntax Start = \"a\" {clause}; }}");
        assert!(prepare(&text).is_err(), "{clause}");
    }
    let data =
        prepare("syntax module Test { symbols { foo } syntax Start = X:\"a\" references foo X; }")
            .unwrap();
    assert_eq!(
        data["omitted_semantics"][0]["clauses"],
        serde_json::json!(["References"])
    );
}

#[test]
fn unreachable_choices_are_rejected_with_keyword_boundaries_preserved() {
    for source in [
        "syntax module Test { syntax Start = \"a\" / \"a\"; }",
        "syntax module Test { syntax Start = \"a\"? / \"b\"; }",
        "syntax module Test { token T = \"a\" / \"ab\"; syntax Start = T; }",
    ] {
        assert!(prepare(source).is_err(), "{source}");
    }
    assert!(prepare("syntax module Test { syntax Start = \"a\" / \"ab\"; }").is_ok());
}

#[test]
fn extension_alternative_names_are_unique_across_modules() {
    assert!(prepare("syntax module Test { syntax Start = E; extensible syntax E { | N = \"a\" } } syntax module Extra { extend syntax Test.E { | N = \"b\" } }").is_err());
}

// Stage-one exports predate the optional negated flag. The live exporter now
// emits false explicitly; normalize only that documented format default.
fn normalize_legacy_classes(value: &mut Value) {
    if value["kind"] == "class" && value.get("negated").is_none() {
        value["negated"] = Value::Bool(false);
    }
    match value {
        Value::Object(map) => {
            for child in map.values_mut() {
                normalize_legacy_classes(child);
            }
        }
        Value::Array(items) => {
            for child in items {
                normalize_legacy_classes(child);
            }
        }
        _ => {}
    }
}
