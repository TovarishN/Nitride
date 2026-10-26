use nitride_core::*;
type NodeShape = (u16, [u32; 2], Vec<(String, u32)>);
fn shape(t: &SyntaxTree) -> Vec<NodeShape> {
    t.nodes
        .iter()
        .enumerate()
        .map(|(id, n)| {
            (
                n.rule,
                n.span,
                t.children(id as u32)
                    .iter()
                    .map(|e| (t.label(e).to_owned(), e.node))
                    .collect(),
            )
        })
        .collect()
}
fn compare(
    parser: &'static CompiledParser,
    language: &'static Language,
    source: &str,
    options: ParseOptions,
    limits: Limits,
) -> Option<ParseSnapshot> {
    let old = parse_snapshot(language, source, limits, SnapshotMode::Strict(options));
    let new = parse_snapshot_generated(parser, source, limits, SnapshotMode::Strict(options));
    match (old, new) {
        (Ok(a), Ok(b)) => {
            let a = a.materialize().unwrap();
            let bt = b.materialize().unwrap();
            assert_eq!(a.root, bt.root, "{source}");
            assert_eq!(shape(&a), shape(&bt), "{source}");
            Some(b)
        }
        (Err(a), Err(b)) => {
            assert_eq!(a, b, "{source}");
            None
        }
        _ => panic!("generated acceptance mismatch: {source}"),
    }
}
#[test]
fn generated_syntax_preserves_precedence_labels_aliases_and_modes() {
    for (p, l, s) in [
        (
            &nitride_cli::PARSER,
            &nitride_cli::LANGUAGE,
            "let x : Scalar = 1 + 2 * 3; x - 4 / 2;",
        ),
        (
            &nitride_cli::power::PARSER,
            &nitride_cli::power::LANGUAGE,
            "let x : Scalar = 2; -x ^ 3 ^ 2;",
        ),
        (
            &nitride_cli::xml::PARSER,
            &nitride_cli::xml::LANGUAGE,
            "<root><item key=\"é\">value</item><item/></root>",
        ),
        (
            &nitride_cli::csharp4::PARSER,
            &nitride_cli::csharp4::LANGUAGE,
            "class C { int F(int x) { while (x > 0) { x = x - 1; } return x * 2 + 3; } }",
        ),
    ] {
        for bits in 0..8 {
            let options = ParseOptions {
                strict_comments: bits & 1 != 0,
                token_children: bits & 2 != 0,
                alias_nodes: bits & 4 != 0,
            };
            let snapshot = compare(p, l, s, options, Limits::default()).unwrap();
            assert!(snapshot.stats().native_syntax_calls > 0);
            if bits == 0 && !std::ptr::eq(l, &nitride_cli::xml::LANGUAGE) {
                assert!(snapshot.stats().native_syntax_tails > 0);
            }
        }
    }
}
#[test]
fn generated_syntax_matches_entire_frozen_csharp_corpus() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/csharp4-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        compare(
            &nitride_cli::csharp4::PARSER,
            &nitride_cli::csharp4::LANGUAGE,
            case["source"].as_str().unwrap(),
            ParseOptions::default(),
            Limits::default(),
        );
    }
}
#[test]
fn generated_syntax_keeps_budget_and_comment_errors() {
    for source in [
        "class C { int F(){return 1+2*3;} }",
        "class C {/*",
        "class C { int F(){ return (1+); } }",
    ] {
        for frames in [0, 1, 8, 16, 32, 64, 128] {
            compare(
                &nitride_cli::csharp4::PARSER,
                &nitride_cli::csharp4::LANGUAGE,
                source,
                ParseOptions {
                    strict_comments: true,
                    ..Default::default()
                },
                Limits {
                    parser_frames: frames,
                    ..Limits::default()
                },
            );
        }
        for work in [0, 1, 10, 100, 500, 1000, 10000] {
            compare(
                &nitride_cli::csharp4::PARSER,
                &nitride_cli::csharp4::LANGUAGE,
                source,
                ParseOptions {
                    strict_comments: true,
                    ..Default::default()
                },
                Limits {
                    work,
                    parser_work_per_byte: 0,
                    ..Limits::default()
                },
            );
        }
    }
}
#[test]
fn generated_initial_snapshot_keeps_interpreted_edits_and_old_readers() {
    let source = "class C { int F(){return 111;} int G(){return 222;} }";
    let original = parse_snapshot_generated(
        &nitride_cli::csharp4::PARSER,
        source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let at = source.find("222").unwrap() as u32;
    let edit = TextEdit {
        range: [at, at + 3],
        replacement: "éclair + 3".into(),
    };
    let next = original.edit(&edit, Limits::default()).unwrap();
    let changed = source.replace("222", "éclair + 3");
    let fresh = compare(
        &nitride_cli::csharp4::PARSER,
        &nitride_cli::csharp4::LANGUAGE,
        &changed,
        ParseOptions::default(),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        shape(&next.materialize().unwrap()),
        shape(&fresh.materialize().unwrap())
    );
    assert_eq!(next.stats().native_syntax_calls, 0);
    assert_eq!(original.materialize().unwrap().source, source);
}

#[test]
fn generated_syntax_preserves_lexical_only_logical_work() {
    for (p, l, s) in [
        (
            &nitride_cli::PARSER,
            &nitride_cli::LEXERS,
            "let x : Scalar = (1+2)*3; x-4/2;",
        ),
        (
            &nitride_cli::power::PARSER,
            &nitride_cli::power::LEXERS,
            "let x : Scalar = 2; -x ^ 3 ^ 2;",
        ),
        (
            &nitride_cli::csharp4::PARSER,
            &nitride_cli::csharp4::LEXERS,
            "class C { int F(int x){while(x>0){x=x-1;}return (x+1)*2;} }",
        ),
    ] {
        for bits in 0..8 {
            let mode = SnapshotMode::Strict(ParseOptions {
                strict_comments: bits & 1 != 0,
                token_children: bits & 2 != 0,
                alias_nodes: bits & 4 != 0,
            });
            let old = parse_snapshot_compiled(l, s, Limits::default(), mode).unwrap();
            let new = parse_snapshot_generated(p, s, Limits::default(), mode).unwrap();
            assert!(new.stats().native_syntax_calls > 0);
            assert_eq!(old.stats().work, new.stats().work, "mode {bits}: {s}");
        }
    }
}
