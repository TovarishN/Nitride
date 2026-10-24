use nitride_prepare::prepare_project;
use std::path::Path;

#[test]
fn grammar_frontend_reproduces_its_seed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let prepared = prepare_project(&root.join("languages/grammar/project.json")).unwrap();
    let generated = nitride_codegen::generate(&prepared.data.to_string()).unwrap();
    let seed =
        std::fs::read_to_string(root.join("crates/nitride-prepare/src/grammar_seed.rs")).unwrap();
    assert_eq!(
        generated, seed,
        "run bootstrap regeneration after changing the grammar"
    );
}

#[test]
fn generated_grammar_rejects_unterminated_comments_with_source_location() {
    let source = "// café 😀\nsyntax module Test { syntax Start = \"x\"; } /* bad";
    let error = nitride_prepare::prepare_sources(
        &[nitride_prepare::Source {
            path: "comment.ngr".into(),
            text: source.into(),
        }],
        "{\"types\":[],\"operations\":[],\"statements\":{}}",
        "Test.Start",
        "syntax",
    )
    .unwrap_err();
    assert_eq!(error.span[0], source.find("/* bad").unwrap());
    assert_eq!(error.line, 2);
    assert!(error.message.contains("unterminated comment"));
}

fn syntax(text: &str) -> Result<serde_json::Value, nitride_prepare::PrepareError> {
    nitride_prepare::prepare_sources(
        &[nitride_prepare::Source {
            path: "grammar.ngr".into(),
            text: text.into(),
        }],
        "{\"types\":[],\"operations\":[],\"statements\":{}}",
        "Test.Start",
        "syntax",
    )
}

#[test]
fn range_punctuation_cannot_become_two_wildcards() {
    for body in ["..", "\"x\"..", "A.."] {
        assert!(
            syntax(&format!(
                "syntax module Test {{ token A=\"a\"; syntax Start={body}; }}"
            ))
            .is_err(),
            "{body}"
        );
    }
}

#[test]
fn lowering_keywords_keep_the_existing_profile_boundary() {
    for target in ["literal", "template", "expand"] {
        assert!(
            syntax(&format!(
                "syntax module Test {{ syntax Start=\"x\" lowers {target}(); }}"
            ))
            .is_err(),
            "{target}"
        );
    }
}

#[test]
fn generated_recognition_keeps_adjacency_and_reserved_word_boundaries() {
    assert!(syntax("syntax module Test { token A=\"a\"; syntax Start=A (\"x\"); }").is_ok());
    assert!(syntax("syntax module Test { token A=\"a\"; syntax Start=A(\"x\"); }").is_err());
    assert!(syntax("syntax module Test { syntax Start=scope; syntax scope=\"x\"; }").is_err());
    assert!(
        syntax("syntax module Test { syntax Start=scopeName; syntax scopeName=\"x\"; }").is_ok()
    );
}

#[test]
fn opaque_blocks_balance_quoted_and_commented_braces() {
    let source = r#"syntax module Test {
      syntax Start="x" { var s="}"; var c='{'; /* } */ if (true) { /* { */ } // }
      }
    }"#;
    let result = syntax(source).unwrap();
    assert_eq!(result["omitted_semantics"][0]["block"], true);
    for body in [
        "{",
        "{ /* missing",
        "{ @\"x\" }",
        "{ \"\"\"raw\"\"\" }",
        "{ \"\\uD800\" }",
    ] {
        assert!(
            syntax(&format!(
                "syntax module Test {{ syntax Start=\"x\" {body} }}"
            ))
            .is_err(),
            "{body}"
        );
    }
}

#[test]
fn utf16_literal_decoding_remains_distinct_from_generated_recognition() {
    let good = r#"syntax module Test { syntax Start="\uD83D\uDE00"; }"#;
    assert_eq!(syntax(good).unwrap()["rules"][0]["expr"]["text"], "😀");
    for body in [r#""\uD800""#, "['😀']", r#""\q""#] {
        assert!(
            syntax(&format!("syntax module Test {{ syntax Start={body}; }}")).is_err(),
            "{body}"
        );
    }
    // Classes match UTF-16 units in Nitrogen: a lone surrogate unit is admitted
    // there and keeps its UTF-16 range (codegen gives it UTF-8 meaning).
    let class = syntax(r#"syntax module Test { syntax Start=['\uD800']; }"#).unwrap();
    assert_eq!(
        class["rules"][0]["expr"]["ranges"],
        serde_json::json!([[0xD800, 0xD800]])
    );
}

#[test]
fn opaque_character_regions_are_scanned_without_csharp_validation() {
    for literal in ["'ab'", "''", "'😀'"] {
        let source = format!("syntax module Test {{ syntax Start=\"x\" {{ var c={literal}; }} }}");
        assert!(syntax(&source).is_ok(), "{literal}");
    }
}

#[test]
fn generated_grammar_limits_reject_deep_opaque_and_postfix_inputs() {
    let blocks = format!("{}{}", "{".repeat(300), "}".repeat(300));
    let error = syntax(&format!(
        "syntax module Test {{ syntax Start=\"x\" {blocks} }}"
    ))
    .unwrap_err();
    assert!(error.message.contains("limit"));
    let suffixes = "?".repeat(300);
    let error = syntax(&format!(
        "syntax module Test {{ syntax Start=\"x\"{suffixes}; }}"
    ))
    .unwrap_err();
    assert!(error.message.contains("nesting"));
}

#[test]
fn ordinary_nested_groups_fit_the_generated_frontend_budget() {
    let body = format!("{}\"x\"{}", "(".repeat(12), ")".repeat(12));
    let result = syntax(&format!("syntax module Test {{ syntax Start={body}; }}")).unwrap();
    assert_eq!(result["rules"][0]["expr"]["text"], "x");
}

#[test]
fn reserved_labels_cannot_extend_an_existing_sequence() {
    for word in [
        "precedence",
        "except",
        "then",
        "else",
        "declares",
        "references",
        "scope",
        "dynamic",
        "lowers",
        "rewrites",
    ] {
        let body = format!("syntax module Test {{ token A=\"a\"; syntax Start=\"x\" {word}:A; }}");
        assert!(syntax(&body).is_err(), "{word}");
        let first = format!("syntax module Test {{ token A=\"a\"; syntax Start={word}:A; }}");
        assert!(syntax(&first).is_ok(), "initial label: {word}");
    }
}

#[test]
fn combined_group_and_postfix_depth_returns_an_error_on_a_small_stack() {
    // Use a subprocess because a stack overflow aborts the whole test process.
    // The child exercises Cargo's ordinary 2 MiB worker-stack size explicitly.
    const CHILD: &str = "NITRIDE_BOOTSTRAP_DEPTH_CHILD";
    if std::env::var_os(CHILD).is_some() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let mut body = "\"x\"".to_owned();
                for _ in 0..12 {
                    body = format!("({body}){}", "?".repeat(128));
                }
                let error =
                    syntax(&format!("syntax module Test {{ syntax Start={body}; }}")).unwrap_err();
                assert!(error.message.contains("nesting") || error.message.contains("limit"));
            })
            .unwrap()
            .join()
            .unwrap();
        return;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "combined_group_and_postfix_depth_returns_an_error_on_a_small_stack",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "deep grammar must return a diagnostic without aborting"
    );
}
