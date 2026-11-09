use nitride_core::*;
#[allow(dead_code)]
mod case_0 {
    include!(concat!(env!("OUT_DIR"), "/stdlib_AsciiIdentifierStart.rs"));
}
#[allow(dead_code)]
mod case_1 {
    include!(concat!(
        env!("OUT_DIR"),
        "/stdlib_AsciiIdentifierContinue.rs"
    ));
}
#[allow(dead_code)]
mod case_2 {
    include!(concat!(env!("OUT_DIR"), "/stdlib_AsciiIdentifier.rs"));
}
#[allow(dead_code)]
mod case_3 {
    include!(concat!(env!("OUT_DIR"), "/stdlib_Digit.rs"));
}
#[allow(dead_code)]
mod case_4 {
    include!(concat!(env!("OUT_DIR"), "/stdlib_HexDigit.rs"));
}
#[allow(dead_code)]
mod case_5 {
    include!(concat!(env!("OUT_DIR"), "/stdlib_UnsignedInteger.rs"));
}
#[allow(dead_code)]
mod case_6 {
    include!(concat!(env!("OUT_DIR"), "/stdlib_UnsignedDecimal.rs"));
}
#[allow(dead_code)]
mod case_7 {
    include!(concat!(env!("OUT_DIR"), "/stdlib_HexInteger.rs"));
}
#[allow(dead_code)]
mod case_8 {
    include!(concat!(env!("OUT_DIR"), "/stdlib_JsonString.rs"));
}
#[allow(dead_code)]
mod case_9 {
    include!(concat!(env!("OUT_DIR"), "/stdlib_Keyword.rs"));
}
#[allow(dead_code)]
mod case_10 {
    include!(concat!(env!("OUT_DIR"), "/stdlib_JsonTrivia.rs"));
}

#[test]
fn generated_library_tokens_match_runtime_and_exact_policies() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../nitride-prepare/tests/fixtures/stdlib_cases.json"
    ))
    .unwrap();
    let parsers: &[(&str, &'static CompiledParser, &'static Language)] = &[
        ("AsciiIdentifierStart", &case_0::PARSER, &case_0::LANGUAGE),
        (
            "AsciiIdentifierContinue",
            &case_1::PARSER,
            &case_1::LANGUAGE,
        ),
        ("AsciiIdentifier", &case_2::PARSER, &case_2::LANGUAGE),
        ("Digit", &case_3::PARSER, &case_3::LANGUAGE),
        ("HexDigit", &case_4::PARSER, &case_4::LANGUAGE),
        ("UnsignedInteger", &case_5::PARSER, &case_5::LANGUAGE),
        ("UnsignedDecimal", &case_6::PARSER, &case_6::LANGUAGE),
        ("HexInteger", &case_7::PARSER, &case_7::LANGUAGE),
        ("JsonString", &case_8::PARSER, &case_8::LANGUAGE),
    ];
    for &(name, parser, language) in parsers {
        for (group, expected) in [("valid", true), ("invalid", false)] {
            for input in cases[name][group].as_array().unwrap() {
                let input = input.as_str().unwrap();
                let old = parse_snapshot(
                    language,
                    input,
                    Limits::default(),
                    SnapshotMode::Strict(ParseOptions::default()),
                );
                let new = parse_snapshot_generated(
                    parser,
                    input,
                    Limits::default(),
                    SnapshotMode::Strict(ParseOptions::default()),
                );
                assert_eq!(old.is_ok(), expected, "runtime {name}: {input:?}");
                assert_eq!(new.is_ok(), expected, "generated {name}: {input:?}");
                if let (Ok(a), Ok(b)) = (old, new) {
                    let a = a.materialize().unwrap();
                    let b = b.materialize().unwrap();
                    let nodes = |tree: &SyntaxTree| {
                        tree.nodes
                            .iter()
                            .map(|n| (n.rule, n.span, n.first_child, n.child_count))
                            .collect::<Vec<_>>()
                    };
                    let edges = |tree: &SyntaxTree| {
                        tree.edges
                            .iter()
                            .map(|e| (tree.label(e).to_owned(), e.node))
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(nodes(&a), nodes(&b), "{name}: {input:?}");
                    assert_eq!(edges(&a), edges(&b), "{name}: {input:?}");
                }
            }
        }
    }
    for (input, expected) in [
        ("let", false),
        ("letter", true),
        ("_let", true),
        ("let2", true),
    ] {
        assert_eq!(
            parse_snapshot_generated(
                &case_9::PARSER,
                input,
                Limits::default(),
                SnapshotMode::Strict(ParseOptions::default())
            )
            .is_ok(),
            expected
        );
    }
    assert!(
        parse_snapshot_generated(
            &case_10::PARSER,
            "/*comment*/ 42",
            Limits::default(),
            SnapshotMode::Strict(ParseOptions::default())
        )
        .is_err()
    );
}
