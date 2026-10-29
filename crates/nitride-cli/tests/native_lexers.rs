use nitride_core::*;

type NormalNode = (u16, [u32; 2], Vec<(String, u32)>);
fn normalized(tree: &SyntaxTree) -> Vec<NormalNode> {
    tree.nodes
        .iter()
        .enumerate()
        .map(|(id, n)| {
            (
                n.rule,
                n.span,
                tree.children(id as u32)
                    .iter()
                    .map(|e| (tree.label(e).to_owned(), e.node))
                    .collect(),
            )
        })
        .collect()
}
fn compare(source: &str, options: ParseOptions, limits: Limits) -> Option<ParseSnapshot> {
    let old = parse_snapshot(
        &nitride_cli::csharp4::LANGUAGE,
        source,
        limits,
        SnapshotMode::Strict(options),
    );
    let new = parse_snapshot_compiled(
        &nitride_cli::csharp4::LEXERS,
        source,
        limits,
        SnapshotMode::Strict(options),
    );
    match (old, new) {
        (Ok(a), Ok(b)) => {
            assert_eq!(
                normalized(&a.materialize().unwrap()),
                normalized(&b.materialize().unwrap()),
                "{source}"
            );
            Some(b)
        }
        (Err(a), Err(b)) => {
            assert_eq!(a, b, "{source}");
            None
        }
        _ => panic!("compiled acceptance mismatch: {source}"),
    }
}
#[test]
fn compiled_csharp_lexers_activate_and_preserve_modes_unicode_and_exclusions() {
    for source in [
        "class C { int F(int value) { return value + 123; } }",
        "class @class { string é = @\"a\"\"b\"; char x = '\\u0041'; }",
        "class 𐐀 { double value = 1.2e-3; string text = \"é😀\"; }",
        "class \\u0043 { int from; }",
        "class class {}",
        "class C {/*",
    ] {
        for bits in 0..8 {
            let options = ParseOptions {
                strict_comments: bits & 1 != 0,
                token_children: bits & 2 != 0,
                alias_nodes: bits & 4 != 0,
            };
            if let Some(snapshot) = compare(source, options, Limits::default()) {
                if options.token_children {
                    assert_eq!(snapshot.stats().native_lexical_calls, 0);
                } else {
                    assert!(snapshot.stats().native_lexical_calls > 0);
                }
            }
        }
    }
}
#[test]
fn compiled_parse_preserves_budget_errors() {
    let source = "class C { int F() { return a + 1; } }";
    for work in [0, 1, 20, 80, 300, 1000, 5000] {
        compare(
            source,
            ParseOptions::default(),
            Limits {
                work,
                parser_work_per_byte: 0,
                ..Limits::default()
            },
        );
    }
    for frames in [0, 1, 8, 16, 32, 64, 128] {
        compare(
            source,
            ParseOptions::default(),
            Limits {
                parser_frames: frames,
                ..Limits::default()
            },
        );
    }
}
#[test]
fn compiled_parse_matches_frozen_csharp_corpus() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/csharp4-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        compare(
            case["source"].as_str().unwrap(),
            ParseOptions::default(),
            Limits::default(),
        );
    }
}
#[test]
fn compiled_snapshot_edits_recheck_lexical_boundaries() {
    let source = "class C { int identifier; }";
    let snapshot = parse_snapshot_compiled(
        &nitride_cli::csharp4::LEXERS,
        source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let start = source.find("identifier").unwrap() as u32;
    let edit = TextEdit {
        range: [start, start + 10],
        replacement: "éclair".into(),
    };
    let next = snapshot.edit(&edit, Limits::default()).unwrap();
    let fresh = compare(
        "class C { int éclair; }",
        ParseOptions::default(),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        normalized(&next.materialize().unwrap()),
        normalized(&fresh.materialize().unwrap())
    );
    assert_eq!(next.stats().native_lexical_calls, 0);
    assert_eq!(snapshot.materialize().unwrap().source, source);
}

#[test]
fn native_depth_facts_do_not_reject_lower_budget_interpreted_edits() {
    use nitride_core::lexical::{CompiledLexers, LexicalContext};
    fn scan(c: &mut LexicalContext<'_>, d: u32) -> Option<bool> {
        c.enter(d)?;
        c.step(d + 1)?;
        let start = c.position();
        for level in 0..=32 {
            c.step(d + 2 + level)?;
        }
        if c.literal("z") {
            return Some(true);
        }
        c.restore(start);
        for text in ["b", "c", "a"] {
            c.step(d + 2)?;
            if c.literal(text) {
                return Some(true);
            }
            c.restore(start);
        }
        Some(false)
    }
    fn rule(name: &'static str, expr: Expr<'static>, lexical: bool) -> Rule<'static> {
        Rule {
            name,
            expr,
            lexical,
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            action: Action::None,
        }
    }
    let mut deep = Expr::Literal("z");
    for _ in 0..32 {
        deep = Expr::Label("nested", Box::leak(Box::new(deep)));
    }
    let language = Box::leak(Box::new(Language {
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        source_hash: "native-depth-test",
        rules: Box::leak(
            vec![
                rule(
                    "Root",
                    Expr::Repeat {
                        inner: &Expr::Ref(1),
                        min: 1,
                        max: u32::MAX,
                    },
                    false,
                ),
                rule(
                    "Item",
                    Expr::Sequence(&[Expr::Ref(2), Expr::Literal(";")]),
                    false,
                ),
                rule(
                    "Token",
                    Expr::Choice(Box::leak(
                        vec![
                            deep,
                            Expr::Literal("b"),
                            Expr::Literal("c"),
                            Expr::Literal("a"),
                        ]
                        .into_boxed_slice(),
                    )),
                    true,
                ),
            ]
            .into_boxed_slice(),
        ),
    }));
    let lexers = Box::leak(Box::new(CompiledLexers::new(
        language,
        &[None, None, Some(scan)],
    )));
    let original = parse_snapshot_compiled(
        lexers,
        "a;a;a;",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    assert!(original.stats().native_lexical_calls > 0);
    let limits = Limits {
        parser_frames: 20,
        ..Limits::default()
    };
    let fresh = parse_snapshot(
        language,
        "a;a;b;",
        limits,
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let next = original
        .edit(
            &TextEdit {
                range: [4, 5],
                replacement: "b".into(),
            },
            limits,
        )
        .unwrap();
    assert_eq!(
        normalized(&fresh.materialize().unwrap()),
        normalized(&next.materialize().unwrap())
    );
    assert_eq!(next.stats().full_parse_fallbacks, 0);
}

#[test]
fn ascii_fusion_keeps_format_escape_unicode_and_limit_behavior() {
    for source in [
        "class C { int from; int value; int F(int x){ return x + 123; } }",
        "class C { int a\u{200d}b; }",
        "class C { int \\u0061; int \\U00000062; }",
        "class 𐐀 { string é = \"😀\"; }",
        "class C { int a; } @",
        "class C { string s = \"\\u0041\"; }",
    ] {
        for bits in 0..8 {
            compare(
                source,
                ParseOptions {
                    strict_comments: bits & 1 != 0,
                    token_children: bits & 2 != 0,
                    alias_nodes: bits & 4 != 0,
                },
                Limits::default(),
            );
        }
        for work in [0, 1, 10, 100, 1000, 10000] {
            compare(
                source,
                ParseOptions::default(),
                Limits {
                    work,
                    parser_work_per_byte: 0,
                    ..Default::default()
                },
            );
        }
        for frames in [0, 8, 16, 32, 64, 128] {
            compare(
                source,
                ParseOptions::default(),
                Limits {
                    parser_frames: frames,
                    ..Default::default()
                },
            );
        }
    }
}
#[test]
fn ascii_fusion_read_reach_keeps_comment_boundary_edits_and_old_readers() {
    let source = "class C { int left; int right; }";
    let original = parse_snapshot_compiled(
        &nitride_cli::csharp4::LEXERS,
        source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let at = source.find("; int").unwrap() + 1;
    let edit = TextEdit {
        range: [at as u32, (at + 1) as u32],
        replacement: "/*é*/".into(),
    };
    let next = original.edit(&edit, Limits::default()).unwrap();
    let mut changed = source.to_owned();
    changed.replace_range(at..at + 1, &edit.replacement);
    let fresh = compare(&changed, ParseOptions::default(), Limits::default()).unwrap();
    assert_eq!(
        normalized(&next.materialize().unwrap()),
        normalized(&fresh.materialize().unwrap())
    );
    assert_eq!(original.materialize().unwrap().source, source);
    assert_eq!(next.stats().native_lexical_calls, 0);
    assert_eq!(next.stats().full_parse_fallbacks, 0);
}
