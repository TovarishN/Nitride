use nitride_core::*;

static LANGUAGE: Language = Language {
    profile: Profile::Typed,
    start: 0,
    rules: &[
        Rule {
            name: "Root",
            lexical: false,
            expr: Expr::Repeat {
                inner: &Expr::Ref(1),
                min: 0,
                max: u32::MAX,
            },
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            action: Action::None,
        },
        Rule {
            name: "Item",
            lexical: false,
            expr: Expr::Sequence(&[
                Expr::Literal("("),
                Expr::Label("Value", &Expr::Ref(2)),
                Expr::Literal(")"),
                Expr::Literal(";"),
            ]),
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            action: Action::None,
        },
        Rule {
            name: "Digit",
            lexical: true,
            expr: Expr::Class(&[('0', '9')]),
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            action: Action::None,
        },
    ],
    types: &[],
    operations: &[],
    handlers: &[],
    checks: &[],
    helpers: &[],
    rule_checks: &[],
    source_hash: "recovery-test",
};

#[test]
fn valid_recovery_is_identical_to_strict() {
    for source in ["", " (1); /* comment */ (2); ", "(3); // end"] {
        let strict = parse(&LANGUAGE, source, Limits::default()).unwrap();
        let recovered = parse_recovering(
            &LANGUAGE,
            source,
            Limits::default(),
            RecoveryOptions::default(),
        );
        assert!(recovered.diagnostics.is_empty());
        assert!(recovered.tree.recovery().is_none());
        assert_eq!(
            format!("{:?}", strict.nodes),
            format!("{:?}", recovered.tree.nodes)
        );
        assert_eq!(
            format!("{:?}", strict.edges),
            format!("{:?}", recovered.tree.edges)
        );
        assert_eq!(strict.root, recovered.tree.root);
        assert_eq!(strict.source, recovered.tree.source);
    }
}

fn assert_coverage(parsed: &RecoveredParse, source: &str) {
    assert_eq!(parsed.tree.source, source);
    assert!(parsed.tree.node(parsed.tree.root).is_some());
    assert_eq!(
        parsed.tree.nodes[parsed.tree.root as usize].span,
        [0, source.len() as u32]
    );
    let mut cursor = 0;
    for part in &parsed.tree.recovery().unwrap().coverage {
        assert_eq!(part.span[0], cursor);
        assert!(
            source
                .get(part.span[0] as usize..part.span[1] as usize)
                .is_some()
        );
        cursor = part.span[1];
    }
    assert_eq!(cursor as usize, source.len());
}

#[test]
fn arbitrary_source_retained_on_limit() {
    let source = "𝑥 ☃ invalid";
    let recovered = parse_recovering(
        &LANGUAGE,
        source,
        Limits {
            work: 0,
            parser_work_per_byte: 0,
            ..Limits::default()
        },
        RecoveryOptions {
            max_states: 0,
            ..RecoveryOptions::default()
        },
    );
    assert!(recovered.diagnostics.iter().any(|d| d.category == "limit"));
    assert_coverage(&recovered, source);
}

#[test]
fn strict_compile_rejects_recovery_metadata() {
    let recovered = parse_recovering(
        &LANGUAGE,
        "bad",
        Limits::default(),
        RecoveryOptions {
            max_states: 0,
            ..RecoveryOptions::default()
        },
    );
    assert!(!recovered.diagnostics.is_empty());
    let errors = compile(&LANGUAGE, &recovered.tree, Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == "syntax" || d.category == "limit")
    );
}

#[test]
fn missing_delimiter_preserves_following_item() {
    let parsed = parse_recovering(
        &LANGUAGE,
        "(1; (2);",
        Limits::default(),
        RecoveryOptions::default(),
    );
    assert_eq!(parsed.diagnostics.len(), 1, "{parsed:?}");
    let items: Vec<_> = parsed
        .tree
        .nodes
        .iter()
        .filter(|n| n.rule == 1)
        .map(|n| n.span)
        .collect();
    assert_eq!(items, [[0, 3], [4, 8]]);
    assert!(
        parsed
            .tree
            .recovery()
            .unwrap()
            .repairs
            .contains(&Repair::Missing {
                expected: Expected::Literal(")"),
                at: 2
            })
    );
    assert_coverage(&parsed, "(1; (2);");
}

#[test]
fn stray_token_is_retained_as_skipped_source() {
    let parsed = parse_recovering(
        &LANGUAGE,
        "(1)☃; (2);",
        Limits::default(),
        RecoveryOptions::default(),
    );
    assert_eq!(parsed.diagnostics.len(), 1, "{parsed:?}");
    assert_eq!(parsed.tree.nodes.iter().filter(|n| n.rule == 1).count(), 2);
    assert!(
        parsed
            .tree
            .recovery()
            .unwrap()
            .repairs
            .contains(&Repair::Skipped { span: [3, 6] })
    );
    assert_coverage(&parsed, "(1)☃; (2);");
}

#[test]
fn separated_errors_do_not_share_a_global_repair_budget() {
    let source = "(1; (2); (3; (4); (5; (6); (7; (8); (9; (0);";
    let parsed = parse_recovering(
        &LANGUAGE,
        source,
        Limits::default(),
        RecoveryOptions::default(),
    );
    assert_eq!(
        parsed.tree.nodes.iter().filter(|n| n.rule == 1).count(),
        10,
        "{parsed:?}"
    );
    assert_eq!(parsed.diagnostics.len(), 5);
    assert_coverage(&parsed, source);
}

#[test]
fn replacement_costs_two_and_is_deterministic() {
    let mut previous = None;
    for _ in 0..8 {
        let parsed = parse_recovering(
            &LANGUAGE,
            "(x); (2);",
            Limits::default(),
            RecoveryOptions::default(),
        );
        assert_eq!(parsed.stats.fallback_count, 0);
        assert_eq!(parsed.tree.recovery().unwrap().repairs.len(), 2);
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.tree.nodes.iter().filter(|n| n.rule == 1).count(), 2);
        let signature = format!(
            "{:?} {:?}",
            parsed.tree.nodes,
            parsed.tree.recovery().unwrap().repairs
        );
        if let Some(previous) = &previous {
            assert_eq!(&signature, previous);
        }
        previous = Some(signature);
    }
}

#[test]
fn malformed_comments_retain_original_source() {
    let source = "(1); /* unfinished 𝑥";
    let parsed = parse_recovering(
        &LANGUAGE,
        source,
        Limits::default(),
        RecoveryOptions::default(),
    );
    assert!(!parsed.diagnostics.is_empty());
    assert_coverage(&parsed, source);
}

#[test]
fn random_utf8_damage_terminates_with_complete_coverage() {
    let mut random = 17u64;
    let alphabet = ['(', ')', ';', '1', 'x', '𝑥', ' ', '/', '*', '\n'];
    for _ in 0..150 {
        let mut source = String::new();
        for _ in 0..35 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            source.push(alphabet[(random >> 32) as usize % alphabet.len()]);
        }
        let parsed = parse_recovering(
            &LANGUAGE,
            &source,
            Limits::default(),
            RecoveryOptions {
                max_states: 128,
                ..RecoveryOptions::default()
            },
        );
        if parsed.tree.recovery().is_some() {
            assert_coverage(&parsed, &source);
        }
    }
}

#[test]
fn local_search_matches_exhaustive_small_repair_oracle() {
    static SINGLE: Language = Language {
        profile: Profile::SyntaxOnly,
        start: 1,
        rules: LANGUAGE.rules,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        source_hash: "single",
    };
    fn distance(a: &[u8], b: &[u8]) -> usize {
        let mut row: Vec<_> = (0..=b.len()).collect();
        for (i, x) in a.iter().enumerate() {
            let mut next = vec![i + 1];
            for (j, y) in b.iter().enumerate() {
                next.push(
                    (row[j] + 2 * usize::from(x != y))
                        .min(row[j + 1] + 1)
                        .min(next[j] + 1),
                );
            }
            row = next;
        }
        row[b.len()]
    }
    let mut inputs = vec![String::new()];
    let mut frontier = vec![String::new()];
    for _ in 0..3 {
        frontier = frontier
            .iter()
            .flat_map(|s| {
                ['(', ')', ';', '1', 'x']
                    .iter()
                    .map(move |c| format!("{s}{c}"))
            })
            .collect();
        inputs.extend(frontier.iter().cloned());
    }
    for source in inputs {
        let parsed = parse_recovering(
            &SINGLE,
            &source,
            Limits::default(),
            RecoveryOptions {
                max_local_cost: 8,
                max_states: 100_000,
                ..RecoveryOptions::default()
            },
        );
        assert_eq!(
            parsed.stats.fallback_count, 0,
            "{source:?}: {:?}",
            parsed.diagnostics
        );
        assert_eq!(
            parsed.stats.repair_count as usize,
            distance(source.as_bytes(), b"(1);"),
            "{source:?}"
        );
    }
}

#[test]
fn exhausted_local_search_resynchronizes_to_a_healthy_later_item() {
    let source = "(1)abcdefghij; (2);";
    let parsed = parse_recovering(
        &LANGUAGE,
        source,
        Limits::default(),
        RecoveryOptions {
            max_local_cost: 1,
            max_states: 32,
            ..RecoveryOptions::default()
        },
    );
    assert!(parsed.stats.fallback_count > 0);
    assert!(
        parsed
            .tree
            .nodes
            .iter()
            .any(|n| n.rule == 1 && &source[n.span[0] as usize..n.span[1] as usize] == "(2);"),
        "{:?}",
        parsed.diagnostics
    );
    assert_coverage(&parsed, source);
}

fn with_root(expr: Expr<'static>) -> &'static Language<'static> {
    let rules = Box::leak(Box::new([Rule {
        expr,
        action: Action::None,
        ..LANGUAGE.rules[0]
    }]));
    Box::leak(Box::new(Language { rules, ..LANGUAGE }))
}

#[test]
fn failed_lookahead_cannot_be_repaired_into_success() {
    let language = with_root(Expr::Sequence(&[
        Expr::Predicate {
            positive: true,
            inner: &Expr::Literal("a"),
        },
        Expr::Literal("a"),
        Expr::Literal(";"),
    ]));
    let parsed = parse_recovering(
        language,
        "b;",
        Limits::default(),
        RecoveryOptions::default(),
    );
    assert!(!parsed.diagnostics.is_empty());
    assert!(
        !parsed
            .tree
            .recovery()
            .unwrap()
            .repairs
            .iter()
            .any(|r| matches!(r, Repair::Missing { .. }))
    );
    assert_coverage(&parsed, "b;");
}

#[test]
fn nullable_cycles_and_deep_rules_return_bounded_diagnostics() {
    let language = with_root(Expr::Sequence(&[Expr::Ref(0), Expr::Literal(";")]));
    let parsed = parse_recovering(
        language,
        "x",
        Limits {
            parser_frames: 32,
            parser_spill_stack_bytes: 0,
            ..Limits::default()
        },
        RecoveryOptions::default(),
    );
    assert!(parsed.diagnostics.iter().any(|d| d.category == "limit"));
    assert_coverage(&parsed, "x");
}

#[test]
fn lexical_interiors_are_never_repaired() {
    let rules = Box::leak(Box::new([
        Rule {
            expr: Expr::Repeat {
                inner: &Expr::Sequence(&[
                    Expr::Literal("("),
                    Expr::Ref(1),
                    Expr::Literal(")"),
                    Expr::Literal(";"),
                ]),
                min: 0,
                max: u32::MAX,
            },
            action: Action::None,
            ..LANGUAGE.rules[0]
        },
        Rule {
            expr: Expr::Literal("ab"),
            action: Action::None,
            ..LANGUAGE.rules[2]
        },
    ]));
    let language = Box::leak(Box::new(Language { rules, ..LANGUAGE }));
    let source = "(a); (ab);";
    let parsed = parse_recovering(
        language,
        source,
        Limits::default(),
        RecoveryOptions::default(),
    );
    let repairs = &parsed.tree.recovery().unwrap().repairs;
    assert!(
        repairs.iter().any(|r| matches!(
            r,
            Repair::Missing {
                expected: Expected::Rule(1),
                ..
            }
        )),
        "{repairs:?}"
    );
    assert!(repairs.iter().all(|r| !matches!(
        r,
        Repair::Missing {
            expected: Expected::Literal("a" | "b" | "ab"),
            ..
        }
    )));
    assert!(
        parsed
            .tree
            .nodes
            .iter()
            .any(|n| n.rule == 1 && &source[n.span[0] as usize..n.span[1] as usize] == "ab")
    );
    assert_coverage(&parsed, source);
}
