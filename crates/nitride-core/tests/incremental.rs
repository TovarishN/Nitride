use nitride_core::*;
fn rule(name: &'static str, expr: Expr<'static>) -> Rule<'static> {
    Rule {
        name,
        expr,
        precedence: 0,
        owner: None,
        lexical: true,
        except: &[],
        right_associative: false,
        transparent: false,
        alias: false,
        action: Action::None,
    }
}
fn language(lookahead: bool) -> &'static Language<'static> {
    let a = if lookahead {
        Expr::Sequence(&[
            Expr::Literal("1234567890123456"),
            Expr::Predicate {
                positive: true,
                inner: &Expr::Literal("Q"),
            },
        ])
    } else {
        Expr::Sequence(&[Expr::Literal("1234567890123456"), Expr::Literal("Q")])
    };
    let mut rules = vec![
        rule(
            "Root",
            Expr::Sequence(&[
                Expr::Repeat {
                    inner: &Expr::Ref(3),
                    min: 1,
                    max: u32::MAX,
                },
                Expr::Choice(&[Expr::Literal("Q"), Expr::Literal("R")]),
            ]),
        ),
        rule("Long", a),
        rule("Short", Expr::Literal("1234567890123456")),
        rule("Item", Expr::Choice(&[Expr::Ref(1), Expr::Ref(2)])),
    ];
    for r in &mut rules {
        r.lexical = false;
    }
    Box::leak(Box::new(Language {
        profile: Profile::SyntaxOnly,
        rules: Box::leak(rules.into_boxed_slice()),
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        source_hash: "dependency-test",
    }))
}
fn normalize(tree: &SyntaxTree) -> Vec<(u16, [u32; 2], u32)> {
    tree.nodes
        .iter()
        .map(|n| (n.rule, n.span, n.child_count))
        .collect()
}
#[test]
fn predicate_outside_consumed_span_invalidates_selection() {
    let language = language(true);
    let old = parse_snapshot(
        language,
        "1234567890123456Q",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let new = old
        .edit(
            &TextEdit {
                range: [16, 17],
                replacement: "R".into(),
            },
            Limits::default(),
        )
        .unwrap();
    let full = parse(language, "1234567890123456R", Limits::default()).unwrap();
    assert_eq!(normalize(&new.materialize().unwrap()), normalize(&full));
    assert_ne!(normalize(&old.materialize().unwrap()), normalize(&full));
}
#[test]
fn failed_alternative_lookahead_changes_winner() {
    let language = language(false);
    let old = parse_snapshot(
        language,
        "1234567890123456R",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let warm = old.edit(
        &TextEdit {
            range: [16, 17],
            replacement: "Q".into(),
        },
        Limits::default(),
    );
    let full = parse(language, "1234567890123456Q", Limits::default());
    assert_eq!(warm.err(), full.err());
}
#[test]
fn append_at_eof_invalidates_negative_predicate() {
    let rule = rule(
        "Root",
        Expr::Sequence(&[
            Expr::Literal("1234567890123456"),
            Expr::Predicate {
                positive: false,
                inner: &Expr::Any,
            },
        ]),
    );
    let language = Box::leak(Box::new(Language {
        profile: Profile::SyntaxOnly,
        rules: Box::leak(vec![rule].into_boxed_slice()),
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        source_hash: "eof-test",
    }));
    let old = parse_snapshot(
        language,
        "1234567890123456",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    assert!(
        old.edit(
            &TextEdit {
                range: [16, 16],
                replacement: "Q".into()
            },
            Limits::default()
        )
        .is_err()
    );
    assert_eq!(old.root().text(), "1234567890123456");
}

#[test]
fn nested_token_visibility_is_part_of_memo_context() {
    let mut rules = vec![
        rule(
            "Root",
            Expr::Sequence(&[
                Expr::Predicate {
                    positive: true,
                    inner: &Expr::Ref(2),
                },
                Expr::Ref(1),
            ]),
        ),
        rule("Outer", Expr::Ref(4)),
        rule("NestedOuter", Expr::Ref(3)),
        rule("Inner", Expr::Ref(4)),
        rule(
            "Point",
            Expr::Pratt {
                prefixes: &[5],
                infixes: &[],
                memoize: true,
            },
        ),
        rule("Value", Expr::Literal("x")),
    ];
    for id in [0, 4, 5] {
        rules[id].lexical = false;
    }
    rules[5].owner = Some(4);
    let language = Box::leak(Box::new(Language {
        profile: Profile::SyntaxOnly,
        rules: Box::leak(rules.into_boxed_slice()),
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        source_hash: "token-context",
    }));
    let old = parse(language, "x", Limits::default()).unwrap();
    let new = parse_snapshot(
        language,
        "x",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap()
    .materialize()
    .unwrap();
    assert_eq!(normalize(&new), normalize(&old));
}
