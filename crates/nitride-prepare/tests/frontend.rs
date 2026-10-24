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
        "functions",
        "dates",
        "rewrites",
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
        let catalog =
            std::fs::read_to_string(directory.join(manifest["catalog"].as_str().unwrap())).unwrap();
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
        "syntax module Test { syntax function F(X: Start*) : Start = <[x]>; syntax Start = \"x\"; }",
        "syntax module Test { syntax function F(X: Start) : Start = match X { | <[x]> => <[x]>; }; syntax Start = \"x\"; }",
        "syntax module Test { syntax function F(X: Start) : Start = match X { | _ => <[x]>; | <[x]> => <[x]>; }; syntax Start = \"x\"; }",
        "syntax module Test { syntax function F(X: N) : Start = <[x]>; token N = \"n\"; syntax Start = \"x\"; }",
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

#[test]
fn text_arguments_are_positional_and_keep_plain_text_fields() {
    let typed = |text: &str| {
        prepare_sources(
            &[Source {
                path: "test.ngr".into(),
                text: text.into(),
            }],
            r#"{"types":["Core.Text","T.D"],"operations":[
                {"id":"T.Parse","inputs":["Core.Text"],"result":"T.D","handler":"iso-date"},
                {"id":"T.Same","inputs":["T.D"],"result":"T.D","handler":"identity"}],"statements":{}}"#,
            "T.Start",
            "typed",
        )
    };
    let grammar = |rule: &str| format!("syntax module T {{ token W = ['a'..'z']+; {rule} }}");
    let prepared = typed(&grammar("syntax Start = V:W lowers T.Parse(text V);")).unwrap();
    let clause = &prepared["rules"][1]["clauses"][0];
    assert_eq!(clause["args"], serde_json::json!(["V"]));
    assert_eq!(clause["text_args"], serde_json::json!([0]));
    let plain = typed(&grammar("syntax Start = text:W lowers T.Same(text);")).unwrap();
    let clause = &plain["rules"][1]["clauses"][0];
    assert_eq!(clause["args"], serde_json::json!(["text"]));
    assert!(clause.get("text_args").is_none());
    assert!(typed(&grammar("syntax Start = V:W lowers T.Parse(text Missing);")).is_err());
}

#[test]
fn vendored_csharp4_matches_provenance() {
    use sha2::{Digest, Sha256};
    let directory = root().join("languages/csharp4");
    let provenance: Value =
        serde_json::from_str(&std::fs::read_to_string(directory.join("provenance.json")).unwrap())
            .unwrap();
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(directory.join("project.json")).unwrap())
            .unwrap();
    let files = provenance["files"].as_object().unwrap();
    assert_eq!(files.len(), 30);
    assert_eq!(manifest["grammars"].as_array().unwrap().len(), files.len());
    for (path, hash) in files {
        let bytes = std::fs::read(directory.join(path)).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            hash.as_str().unwrap(),
            "{path}"
        );
    }
}

#[test]
fn transparent_marker_requires_name_or_element() {
    let prepared = prepare(
        "syntax module Test { syntax Start = S; extensible syntax S { | transparent Plain = Body:(W / \";\") | transparent W2 = W | Other = transparent } syntax W = \"w\"; syntax transparent = \"t\"; }",
    )
    .unwrap();
    let rule = |name: &str| {
        prepared["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(rule("Test.Plain")["transparent"], true);
    assert_eq!(rule("Test.W2")["transparent"], true);
    assert!(rule("Test.Other").get("transparent").is_none());
    assert_eq!(rule("Test.Other")["expr"]["name"], "Test.transparent");
    for invalid in [
        "| transparent P = W precedence 5",
        "| transparent P = W W",
        "| transparent P = W?",
        "| transparent P = S \"+\" W precedence 10",
    ] {
        let source = format!(
            "syntax module Test {{ syntax Start = S; extensible syntax S {{ {invalid} | X = \"x\" }} syntax W = \"w\"; }}"
        );
        assert!(prepare(&source).is_err(), "{invalid}");
    }
}

fn generated_syntax(text: &str) -> Result<String, String> {
    let prepared = prepare(text).map_err(|e| e.message)?;
    nitride_codegen::generate(&prepared.to_string())
}

#[test]
fn surrogate_pair_classes_match_supplementary_characters() {
    let source = "syntax module Test { token T = ['\\ud835'] ['\\udc65'..'\\udc66']; syntax Start = Value:T; }";
    let prepared = prepare(source).unwrap();
    // Normalized JSON keeps Nitrogen's UTF-16 ranges.
    assert_eq!(
        prepared["rules"][0]["expr"]["items"][0]["ranges"],
        serde_json::json!([[0xD835, 0xD835]])
    );
    let generated = generated_syntax(source).unwrap();
    assert!(
        generated.contains("Expr::Class(&[('\u{1d465}','\u{1d466}')])"),
        "{generated}"
    );
}

#[test]
fn unpaired_surrogate_class_is_inert() {
    let generated = generated_syntax(
        "syntax module Test { token T = ['\\ud835'] / \"a\"; syntax Start = Value:T; }",
    )
    .unwrap();
    assert!(generated.contains("Expr::Class(&[])"), "{generated}");
    // Strings still reject unpaired surrogates.
    assert!(prepare("syntax module Test { syntax Start = \"\\ud835\"; }").is_err());
}
