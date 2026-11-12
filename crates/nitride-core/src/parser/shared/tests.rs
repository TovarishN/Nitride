//! Shared recognizer regression tests.
use super::*;
#[test]
fn repeated_generated_trivia_speculation_preserves_cancellation_credit() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let language = prefix_fact_fixture(false);
    let lexers = Box::leak(Box::new(crate::lexical::CompiledLexers::new(language, &[])));
    let source = format!("{}a", " ".repeat(4096));
    let text = SourceText::new(&source).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let token = crate::CancellationToken::from_predicate(move || {
        count.fetch_add(1, Ordering::Relaxed);
        false
    });
    crate::with_cancellation(&token, || {
        let mut probe = SharedProbe::new(
            language,
            &text,
            Limits::default(),
            None,
            Index::default(),
            None,
        );
        probe.generated = Some(Generated {
            compiled: None,
            lexers,
            source: &source,
        });
        probe.prerecognize_reference(&Expr::Ref(1), 0);
        let first = calls.load(Ordering::Relaxed);
        for _ in 0..4 {
            probe.prerecognize_reference(&Expr::Ref(1), 0);
        }
        assert!(
            calls.load(Ordering::Relaxed) >= first + 12,
            "repeated speculative trivia must keep consuming polling credit"
        );
    });
}

#[test]
fn choice_limits_precede_speculative_trivia_errors() {
    for (wrapped, frames, work) in [(false, 2, 7), (true, 3, 8)] {
        let template = prefix_fact_fixture(false);
        let parts: &'static [Expr] = Box::leak(
            ["a", "b", "c", "d"]
                .into_iter()
                .map(|text| {
                    if wrapped {
                        Expr::Label("value", Box::leak(Box::new(Expr::Literal(text))))
                    } else {
                        Expr::Literal(text)
                    }
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let root = Rule {
            name: "Root",
            lexical: false,
            expr: Expr::Sequence(Box::leak(
                vec![Expr::Literal("@"), Expr::Choice(parts)].into_boxed_slice(),
            )),
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            origin: None,
            action: Action::None,
        };
        let language = Box::leak(Box::new(Language {
            trivia: crate::TriviaPolicy::Nitrogen,
            rules: Box::leak(vec![root].into_boxed_slice()),
            ..*template
        }));
        let options = ParseOptions {
            strict_comments: true,
            ..Default::default()
        };
        for limits in [
            Limits {
                parser_frames: frames,
                ..Default::default()
            },
            Limits {
                work,
                parser_work_per_byte: 0,
                ..Default::default()
            },
        ] {
            let expected = parse_with_options(language, "@/*", limits, options)
                .expect_err("flat limit diagnostic");
            let actual = parse_snapshot(language, "@/*", limits, SnapshotMode::Strict(options))
                .err()
                .expect("shared limit diagnostic");
            assert_eq!(
                (actual.category, actual.span, actual.message),
                (expected.category, expected.span, expected.message)
            );
        }
    }
}
#[test]
fn batched_skips_preserve_single_step_budget_and_depth_checks() {
    let language = prefix_fact_fixture(false);
    let text = SourceText::new("a").unwrap();
    let make = || {
        Parser::new(
            language,
            Input {
                tables: None,
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: None,
            },
            Limits::default(),
            ParseOptions::default(),
            Limits::default().parser_stack_bytes,
            None,
            Index::default(),
        )
    };
    for (initial, count, depth) in [(0, 0, 0), (0, 3, 0), (4, 9, 0), (0, 9, 3)] {
        let mut batch = make();
        let mut reference = make();
        for parser in [&mut batch, &mut reference] {
            parser.work = initial;
            parser.work_budget = 5;
            parser.limits.parser_frames = 2;
        }
        let actual = batch.tick_steps(count, depth);
        let expected = (0..count).try_for_each(|_| reference.tick(depth));
        assert_eq!(
            actual.err().map(|d| (d.category, d.span, d.message)),
            expected.err().map(|d| (d.category, d.span, d.message))
        );
        assert_eq!(batch.work, reference.work);
        assert_eq!(batch.max_depth, reference.max_depth);
    }
}
#[test]
fn choice_lookahead_stays_lazy_before_a_leading_predicate_error() {
    let template = prefix_fact_fixture(false);
    let root = Rule {
        name: "Root",
        lexical: false,
        expr: Expr::Sequence(&[
            Expr::Literal("@"),
            Expr::Choice(&[
                Expr::Predicate {
                    positive: true,
                    inner: &Expr::Pratt {
                        prefixes: &[],
                        infixes: &[],
                        memoize: false,
                    },
                },
                Expr::Literal("x"),
                Expr::Literal("y"),
                Expr::Literal("z"),
            ]),
        ]),
        except: &[],
        precedence: 0,
        right_associative: false,
        transparent: false,
        alias: false,
        owner: None,
        origin: None,
        action: Action::None,
    };
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        rules: Box::leak(vec![root].into_boxed_slice()),
        ..*template
    }));
    let options = ParseOptions {
        strict_comments: true,
        ..Default::default()
    };
    let expected = parse_with_options(language, "@/*", Limits::default(), options)
        .expect_err("leading predicate grammar error");
    let actual = parse_snapshot(
        language,
        "@/*",
        Limits::default(),
        SnapshotMode::Strict(options),
    )
    .err()
    .expect("leading predicate must run before trivia lookahead");
    assert_eq!(
        (actual.category, actual.span, actual.message),
        (expected.category, expected.span, expected.message)
    );
}
#[test]
fn choice_cache_reclaims_buckets_and_keeps_active_plans_valid() {
    let language = prefix_fact_fixture(false);
    let text = SourceText::new("a").unwrap();
    let mut parser = Parser::new(
        language,
        Input {
            tables: None,
            compiled: None,
            lexers: None,
            text: &text,
            contiguous: None,
        },
        Limits::default(),
        ParseOptions::default(),
        Limits::default().parser_stack_bytes,
        None,
        Index::default(),
    );
    let parts: &'static [Expr] = Box::leak(
        vec![
            Expr::Literal("a"),
            Expr::Literal("b"),
            Expr::Literal("c"),
            Expr::Literal("d"),
        ]
        .into_boxed_slice(),
    );
    let mut held = None;
    for index in 0..900 {
        let expr = Box::leak(Box::new(Expr::Choice(parts)));
        let plan = parser.choice_plan(expr, parts, true).unwrap();
        if index == 0 {
            held = Some(plan);
        }
        assert!(parser.choice_cache_bytes <= 256 * 1024);
    }
    parser.cache_budget = 8192;
    let expr = Box::leak(Box::new(Expr::Choice(parts)));
    parser.choice_plan(expr, parts, true);
    assert!(parser.choice_cache_bytes <= 1024);
    assert!(
        parser.choice_cache.capacity() * std::mem::size_of::<usize>() <= 1024,
        "old bucket capacity escaped the smaller budget"
    );
    assert_eq!(
        held.unwrap().candidates(Some('a'), 0).collect::<Vec<_>>(),
        vec![0]
    );
    assert!(parser.rule(0, 0, 0).unwrap().is_some());
    assert_eq!(parser.position, 1);
}
#[test]
fn ordinary_choices_skip_impossible_rule_bodies() {
    let template = prefix_fact_fixture(false);
    let make = |name, expr| Rule {
        name,
        expr,
        owner: None,
        lexical: false,
        except: &[],
        precedence: 0,
        right_associative: false,
        transparent: false,
        alias: false,
        origin: None,
        action: Action::None,
    };
    let mut rules = vec![make(
        "Root",
        Expr::Choice(Box::leak(
            (1..=65)
                .map(Expr::Ref)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )),
    )];
    for index in 0..65 {
        rules.push(make(
            "Candidate",
            Expr::Literal(if index == 64 { "z" } else { "x" }),
        ));
    }
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        rules: Box::leak(rules.into_boxed_slice()),
        ..*template
    }));
    let snapshot = parse_snapshot(
        language,
        "z",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let expected = parse(language, "z", Limits::default()).unwrap();
    let exported = snapshot.materialize().unwrap();
    assert_eq!(
        exported
            .nodes
            .iter()
            .map(|n| (n.rule, n.span))
            .collect::<Vec<_>>(),
        expected
            .nodes
            .iter()
            .map(|n| (n.rule, n.span))
            .collect::<Vec<_>>()
    );
    assert!(
        snapshot.stats().work < 100,
        "ordinary choice consumed {} steps",
        snapshot.stats().work
    );
}
#[test]
fn predictive_dispatch_keeps_fatal_leading_grammar_checks() {
    for expr in [
        Expr::Sequence(&[
            Expr::Repeat {
                inner: &Expr::Literal(""),
                min: 0,
                max: 2,
            },
            Expr::Literal("x"),
        ]),
        Expr::Sequence(&[
            Expr::Pratt {
                prefixes: &[3],
                infixes: &[],
                memoize: false,
            },
            Expr::Literal("x"),
        ]),
    ] {
        let template = prefix_fact_fixture(false);
        let make = |name, expr, owner| Rule {
            name,
            expr,
            owner,
            lexical: false,
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            origin: None,
            action: Action::None,
        };
        let rules = vec![
            make(
                "Root",
                Expr::Pratt {
                    prefixes: &[1, 2],
                    infixes: &[],
                    memoize: true,
                },
                None,
            ),
            make("Bad", expr, None),
            make("Good", Expr::Literal("y"), Some(0)),
            make("X", Expr::Literal("x"), None),
        ];
        let language = Box::leak(Box::new(Language {
            trivia: crate::TriviaPolicy::Nitrogen,
            rules: Box::leak(rules.into_boxed_slice()),
            ..*template
        }));
        let expected = parse(language, "y", Limits::default())
            .expect_err("flat must reject malformed grammar");
        let actual = parse_snapshot(
            language,
            "y",
            Limits::default(),
            SnapshotMode::Strict(ParseOptions::default()),
        )
        .err()
        .expect("prediction must preserve fatal grammar checks");
        assert_eq!(
            (actual.category, actual.span, actual.message),
            (expected.category, expected.span, expected.message)
        );
    }
}
#[test]
fn predictive_dispatch_keeps_infix_precedence_overflow() {
    let template = prefix_fact_fixture(false);
    let make = |name, expr, owner, precedence| Rule {
        name,
        expr,
        owner,
        precedence,
        lexical: false,
        except: &[],
        right_associative: false,
        transparent: false,
        alias: false,
        origin: None,
        action: Action::None,
    };
    let rules = vec![
        make(
            "Root",
            Expr::Pratt {
                prefixes: &[1],
                infixes: &[2],
                memoize: true,
            },
            None,
            0,
        ),
        make("X", Expr::Literal("x"), Some(0), 0),
        make(
            "Overflow",
            Expr::Sequence(&[Expr::Ref(0), Expr::Literal("+"), Expr::Ref(0)]),
            Some(0),
            u32::MAX,
        ),
    ];
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        rules: Box::leak(rules.into_boxed_slice()),
        ..*template
    }));
    let expected = parse(language, "x", Limits::default()).expect_err("flat precedence overflow");
    let actual = parse_snapshot(
        language,
        "x",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .err()
    .expect("prediction must preserve overflow");
    assert_eq!(
        (actual.category, actual.span, actual.message),
        (expected.category, expected.span, expected.message)
    );
}
#[test]
fn productive_leading_recursion_keeps_the_flat_limit_diagnostic() {
    let template = prefix_fact_fixture(false);
    let make = |name, expr, owner| Rule {
        name,
        expr,
        owner,
        lexical: false,
        except: &[],
        precedence: 0,
        right_associative: false,
        transparent: false,
        alias: false,
        origin: None,
        action: Action::None,
    };
    let rules = vec![
        make(
            "Root",
            Expr::Pratt {
                prefixes: &[1, 2],
                infixes: &[],
                memoize: true,
            },
            None,
        ),
        make(
            "Recursive",
            Expr::Choice(&[Expr::Ref(1), Expr::Literal("x")]),
            None,
        ),
        make("Other", Expr::Literal("y"), Some(0)),
    ];
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        rules: Box::leak(rules.into_boxed_slice()),
        ..*template
    }));
    let limits = Limits {
        parser_frames: 32,
        ..Default::default()
    };
    let expected = parse(language, "y", limits).expect_err("flat must report recursive limit");
    let actual = parse_snapshot(
        language,
        "y",
        limits,
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .err()
    .expect("predictive filtering must not bypass leading recursion");
    assert_eq!(
        (actual.code, actual.span, actual.message),
        (expected.code, expected.span, expected.message)
    );
}
#[test]
fn lexical_results_are_reused_across_repeated_lookahead() {
    let template = prefix_fact_fixture(false);
    let mut parts = (0..16)
        .map(|_| Expr::Predicate {
            positive: true,
            inner: &Expr::Ref(1),
        })
        .collect::<Vec<_>>();
    parts.push(Expr::Ref(1));
    let rules = vec![
        Rule {
            name: "Root",
            lexical: false,
            expr: Expr::Sequence(Box::leak(parts.into_boxed_slice())),
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            origin: None,
            action: Action::None,
        },
        Rule {
            name: "Token",
            lexical: true,
            expr: Expr::Repeat {
                inner: &Expr::Class(&[('a', 'z')]),
                min: 1,
                max: u32::MAX,
            },
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            origin: None,
            action: Action::None,
        },
    ];
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        rules: Box::leak(rules.into_boxed_slice()),
        ..*template
    }));
    let source = "abcdefghijklmnopqrstuvwxyz";
    let snapshot = parse_snapshot(
        language,
        source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let flat = parse(language, source, Limits::default()).unwrap();
    let exported = snapshot.materialize().unwrap();
    assert_eq!(
        exported
            .nodes
            .iter()
            .map(|n| (n.rule, n.span))
            .collect::<Vec<_>>(),
        flat.nodes
            .iter()
            .map(|n| (n.rule, n.span))
            .collect::<Vec<_>>()
    );
    assert!(
        snapshot.stats().work < 200,
        "lexical lookahead consumed {} work steps",
        snapshot.stats().work
    );
}
#[test]
fn predictive_starts_bound_work_without_changing_selected_tree() {
    let template = prefix_fact_fixture(false);
    let mut rules = vec![Rule {
        name: "Root",
        lexical: false,
        expr: Expr::Pratt {
            prefixes: Box::leak((1..=65).collect::<Vec<u16>>().into_boxed_slice()),
            infixes: &[],
            memoize: true,
        },
        except: &[],
        precedence: 0,
        right_associative: false,
        transparent: false,
        alias: false,
        owner: None,
        origin: None,
        action: Action::None,
    }];
    for index in 0..65 {
        rules.push(Rule {
            name: "Candidate",
            lexical: false,
            expr: Expr::Literal(if index == 64 { "z" } else { "x" }),
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: Some(0),
            origin: None,
            action: Action::None,
        });
    }
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        rules: Box::leak(rules.into_boxed_slice()),
        ..*template
    }));
    let snapshot = parse_snapshot(
        language,
        "z",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let flat = parse(language, "z", Limits::default()).unwrap();
    let exported = snapshot.materialize().unwrap();
    assert_eq!(
        exported
            .nodes
            .iter()
            .map(|n| (n.rule, n.span))
            .collect::<Vec<_>>(),
        flat.nodes
            .iter()
            .map(|n| (n.rule, n.span))
            .collect::<Vec<_>>()
    );
    assert!(
        snapshot.stats().work < 100,
        "impossible candidates consumed {} work steps",
        snapshot.stats().work
    );
}
fn prefix_fact_fixture(sensitive: bool) -> &'static Language<'static> {
    let make = |name, expr, owner, precedence| Rule {
        name,
        expr,
        lexical: false,
        owner,
        precedence,
        except: &[],
        right_associative: false,
        transparent: false,
        alias: false,
        origin: None,
        action: Action::None,
    };
    let rules = if sensitive {
        vec![
            make(
                "Root",
                Expr::Pratt {
                    prefixes: &[1, 2],
                    infixes: &[3],
                    memoize: true,
                },
                None,
                0,
            ),
            make(
                "Minus",
                Expr::Sequence(&[Expr::Literal("-"), Expr::Label("Right", &Expr::Ref(0))]),
                Some(0),
                10,
            ),
            make("Number", Expr::Class(&[('0', '9')]), Some(0), 100),
            make(
                "Add",
                Expr::Sequence(&[
                    Expr::Label("Left", &Expr::Ref(0)),
                    Expr::Literal("+"),
                    Expr::Label("Right", &Expr::Ref(0)),
                ]),
                Some(0),
                1,
            ),
        ]
    } else {
        vec![
            make(
                "Root",
                Expr::Pratt {
                    prefixes: &[1],
                    infixes: &[],
                    memoize: true,
                },
                None,
                0,
            ),
            make("Value", Expr::Literal("a"), Some(0), 0),
        ]
    };
    Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        patterns: &[],
        source_hash: "prefix-fact",
        rules: Box::leak(rules.into_boxed_slice()),
    }))
}
fn prefix_snapshot(language: &'static Language<'static>, text: &SourceText) -> ParseSnapshot {
    let mut parser = Parser::new(
        language,
        Input {
            tables: None,
            compiled: None,
            lexers: None,
            text,
            contiguous: None,
        },
        Limits::default(),
        ParseOptions::default(),
        Limits::default().parser_stack_bytes,
        None,
        Index::default(),
    );
    let root = parser.rule(1, 0, 0).unwrap().unwrap();
    assert_eq!(parser.position, text.len() as usize);
    publish(
        parser,
        text,
        root,
        None,
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap()
}
#[test]
fn cache_pressure_preserves_facts_for_history_owned_roots() {
    let language = prefix_fact_fixture(false);
    let text = SourceText::new(&"a ".repeat(5000)).unwrap();
    let mut probe = SharedProbe::new(
        language,
        &text,
        Limits::default(),
        None,
        Index::default(),
        None,
    );
    probe.parser.cache_budget = 1024;
    let first = probe.rule(1, 0, 0).unwrap().unwrap();
    let root = probe.handle(first.root).unwrap();
    probe.selected(first.root);
    for at in 1..5000 {
        assert!(probe.rule(1, at * 2, 0).unwrap().is_some());
    }
    let replay = probe.rule(1, 0, 0).unwrap().unwrap();
    assert_eq!(
        probe.handle(replay.root).unwrap().node,
        root.node,
        "cache pressure discarded a fact whose history-owned root could not release graph storage"
    );
    assert!(probe.parser.arena.cache_only_bytes() <= probe.parser.cache_budget);
}
#[test]
fn warm_pratt_reuses_a_validated_power_insensitive_prefix_fact() {
    let language = prefix_fact_fixture(false);
    let text = SourceText::new("a").unwrap();
    let base = prefix_snapshot(language, &text);
    let next = parse_source(
        language,
        Input {
            tables: None,
            compiled: None,
            lexers: None,
            text: &text,
            contiguous: None,
        },
        Limits::default(),
        ParseOptions::default(),
        Limits::default().parser_stack_bytes,
        Some(&base),
        base.recognitions.clone(),
        None,
    )
    .unwrap();
    assert_eq!(
        next.root().subtree_id(),
        base.root().subtree_id(),
        "validated prefix was reparsed because its caller was not a recording checkpoint"
    );
    assert!(next.stats().reused_subtrees > 0);
}
#[test]
fn warm_pratt_does_not_reuse_a_prefix_at_a_different_sensitive_power() {
    let language = prefix_fact_fixture(true);
    let text = SourceText::new("-1+2").unwrap();
    let base = prefix_snapshot(language, &text);
    assert_eq!(base.root().rule(), 1);
    let next = parse_source(
        language,
        Input {
            tables: None,
            compiled: None,
            lexers: None,
            text: &text,
            contiguous: None,
        },
        Limits::default(),
        ParseOptions::default(),
        Limits::default().parser_stack_bytes,
        Some(&base),
        base.recognitions.clone(),
        None,
    )
    .unwrap();
    assert_eq!(next.root().rule(), 3);
    assert_eq!(next.root().children().next().unwrap().1.text(), "-1");
}
#[test]
fn construction_hint_cache_does_not_retain_every_source_position() {
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        patterns: &[],
        source_hash: "hint-cache",
        rules: &[],
    }));
    let count = 50_000;
    let text = SourceText::new(&"a".repeat(count)).unwrap();
    let mut builder = ArenaBuilder::new(vec![""].into());
    let leaf = builder.add(1, 1, vec![]).unwrap();
    let root = builder
        .add(
            0,
            count as u32,
            vec![
                crate::syntax::arena::Child {
                    node: leaf,
                    extent: 1,
                    label: 0
                };
                count
            ],
        )
        .unwrap();
    let mode = SnapshotMode::Strict(ParseOptions::default());
    let base = ParseSnapshot::from_parts(language, text.clone(), builder.finish(), root, 0, mode);
    let mut parser = Parser::new(
        language,
        Input {
            tables: None,
            compiled: None,
            lexers: None,
            text: &text,
            contiguous: None,
        },
        Limits::default(),
        ParseOptions::default(),
        Limits::default().parser_stack_bytes,
        Some(&base),
        Index::default(),
    );
    for at in 0..count as u32 {
        let hint = parser.find_prior(1, at).unwrap().unwrap();
        assert_eq!((hint.node, hint.start, hint.old_start), (leaf, at, at));
    }
    assert!(
        parser.prior_paths.len() <= 4096,
        "retained {} construction paths",
        parser.prior_paths.len()
    );
    assert_eq!(base.root().subtree_id(), root);
}
#[test]
fn warm_maintenance_uses_the_approved_retention_allowance() {
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        patterns: &[],
        source_hash: "maintenance-slack",
        rules: &[],
    }));
    let count = 200_000;
    let text = SourceText::new(&"a".repeat(count)).unwrap();
    let mut builder = ArenaBuilder::new(vec![""].into());
    let leaf = builder.add(1, 1, vec![]).unwrap();
    let root = builder
        .add(
            0,
            count as u32,
            vec![
                crate::syntax::arena::Child {
                    node: leaf,
                    extent: 1,
                    label: 0
                };
                count
            ],
        )
        .unwrap();
    let mode = SnapshotMode::Strict(ParseOptions::default());
    let mut base =
        ParseSnapshot::from_parts(language, text.clone(), builder.finish(), root, 0, mode);
    // Model previously accumulated publication churn, without allocating
    // disposable history merely to test the retention trigger.
    base.maintenance_debt = 8 * 1024 * 1024 + 1;
    let parser = Parser::new(
        language,
        Input {
            tables: None,
            compiled: None,
            lexers: None,
            text: &text,
            contiguous: None,
        },
        Limits::default(),
        ParseOptions::default(),
        Limits::default().parser_stack_bytes,
        Some(&base),
        Index::default(),
    );
    let next = publish(
        parser,
        &text,
        Handle {
            node: root,
            start: 0,
            width: count as u32,
        },
        Some(&base),
        mode,
    )
    .unwrap();
    assert_eq!(
        next.stats().maintenance_passes,
        0,
        "full graph copied before the approved retention allowance was used"
    );
    assert_eq!(next.stats().syntax_bytes_copied, 0);
    assert_eq!(next.root().subtree_id(), base.root().subtree_id());
    base.maintenance_debt = 8 * 1024 * 1024 + base.arena.payload_bytes() / 2 + 1;
    let parser = Parser::new(
        language,
        Input {
            tables: None,
            compiled: None,
            lexers: None,
            text: &text,
            contiguous: None,
        },
        Limits::default(),
        ParseOptions::default(),
        Limits::default().parser_stack_bytes,
        Some(&base),
        Index::default(),
    );
    let compacted = publish(
        parser,
        &text,
        Handle {
            node: root,
            start: 0,
            width: count as u32,
        },
        Some(&base),
        mode,
    )
    .unwrap();
    assert_eq!(compacted.stats().maintenance_passes, 1);
    assert!(compacted.stats().syntax_bytes_copied > 0);
    assert_eq!(
        base.root().children().nth(count - 1).unwrap().1.span(),
        [count as u32 - 1, count as u32]
    );
}
#[test]
fn unselected_recovery_probe_cache_is_bounded() {
    let make = |name, expr| Rule {
        name,
        expr,
        lexical: false,
        precedence: 0,
        owner: None,
        except: &[],
        right_associative: false,
        transparent: false,
        alias: false,
        origin: None,
        action: Action::None,
    };
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        patterns: &[],
        source_hash: "unselected-recovery",
        rules: Box::leak(
            vec![
                make(
                    "Root",
                    Expr::Repeat {
                        inner: &Expr::Ref(1),
                        min: 1,
                        max: u32::MAX,
                    },
                ),
                make("Item", Expr::Literal(";")),
            ]
            .into_boxed_slice(),
        ),
    }));
    let text = SourceText::new(&";".repeat(4000)).unwrap();
    let mut probe = SharedProbe::new(
        language,
        &text,
        Limits::default(),
        None,
        Index::default(),
        None,
    );
    probe.parser.cache_budget = 1024;
    for at in 0..4000 {
        assert!(probe.rule(1, at, 0).unwrap().is_some());
    }
    assert!(probe.parser.arena.cache_only_bytes() + probe.parser.memo_storage_bound() <= 1024);
    assert!(probe.parser.stats.cache_evictions > 0);
}
#[test]
fn recovery_history_roots_are_active_syntax_not_disposable_cache() {
    let make = |name, expr| Rule {
        name,
        expr,
        lexical: false,
        precedence: 0,
        owner: None,
        except: &[],
        right_associative: false,
        transparent: false,
        alias: false,
        origin: None,
        action: Action::None,
    };
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        patterns: &[],
        source_hash: "active-recovery",
        rules: Box::leak(
            vec![
                make(
                    "Root",
                    Expr::Repeat {
                        inner: &Expr::Ref(1),
                        min: 1,
                        max: u32::MAX,
                    },
                ),
                make("Item", Expr::Literal(";")),
            ]
            .into_boxed_slice(),
        ),
    }));
    let text = SourceText::new(&";".repeat(4000)).unwrap();
    let mut probe = SharedProbe::new(
        language,
        &text,
        Limits::default(),
        None,
        Index::default(),
        None,
    );
    probe.parser.cache_budget = 1024;
    for at in 0..4000 {
        let hit = probe.rule(1, at, 0).unwrap().unwrap();
        probe.selected(hit.root);
    }
    assert_eq!(probe.handles.len(), 4000);
    assert!(
        probe
            .handles
            .iter()
            .all(|h| probe.parser.arena.node(h.node).is_some())
    );
    assert_eq!(probe.parser.arena.cache_only_bytes(), 0);
}
#[test]
fn cache_pressure_reclaims_failed_alternative_roots() {
    let make = |name, expr| Rule {
        name,
        expr,
        lexical: false,
        precedence: 0,
        owner: None,
        except: &[],
        right_associative: false,
        transparent: false,
        alias: false,
        origin: None,
        action: Action::None,
    };
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        patterns: &[],
        source_hash: "pressure",
        rules: Box::leak(
            vec![
                make(
                    "Root",
                    Expr::Choice(&[
                        Expr::Sequence(&[Expr::Ref(1), Expr::Literal("!")]),
                        Expr::Ref(1),
                    ]),
                ),
                make(
                    "List",
                    Expr::Repeat {
                        inner: &Expr::Ref(2),
                        min: 1,
                        max: u32::MAX,
                    },
                ),
                make("Item", Expr::Literal(";")),
            ]
            .into_boxed_slice(),
        ),
    }));
    let text = SourceText::new(&";".repeat(4000)).unwrap();
    let mut parser = Parser::new(
        language,
        Input {
            tables: None,
            compiled: None,
            lexers: None,
            text: &text,
            contiguous: None,
        },
        Limits::default(),
        ParseOptions::default(),
        usize::MAX,
        None,
        Index::default(),
    );
    parser.cache_budget = 1024;
    assert!(parser.rule(0, 0, 0).unwrap().is_some());
    assert_eq!(parser.position, 4000);
    assert!(parser.stats.cache_evictions > 0);
    assert!(parser.arena.cache_only_bytes() + parser.memo_storage_bound() <= 1024);
}
#[test]
fn wide_snapshot_edit_shares_sequence_blocks() {
    let make = |name, expr, lexical| Rule {
        name,
        expr,
        lexical,
        precedence: 0,
        owner: None,
        except: &[],
        right_associative: false,
        transparent: false,
        alias: false,
        origin: None,
        action: Action::None,
    };
    let language = Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        patterns: &[],
        source_hash: "wide",
        rules: Box::leak(
            vec![
                make(
                    "Root",
                    Expr::Repeat {
                        inner: &Expr::Ref(1),
                        min: 1,
                        max: u32::MAX,
                    },
                    false,
                ),
                make(
                    "Item",
                    Expr::Sequence(&[Expr::Ref(2), Expr::Literal(";")]),
                    false,
                ),
                make(
                    "Number",
                    Expr::Repeat {
                        inner: &Expr::Class(&[('0', '9')]),
                        min: 16,
                        max: 16,
                    },
                    true,
                ),
            ]
            .into_boxed_slice(),
        ),
    }));
    let source = "1111111111111111;".repeat(1000);
    let old = parse_snapshot(
        language,
        &source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let at = 500 * 17 + 5;
    let edited = old
        .edit(
            &TextEdit {
                range: [at, at + 1],
                replacement: "2".into(),
            },
            Limits::default(),
        )
        .unwrap();
    assert!(
        old.arena
            .sequence(old.root)
            .unwrap()
            .shared_leaves(&edited.arena.sequence(edited.root).unwrap())
            > 25
    );
    assert!(edited.stats.reused_subtrees >= 998);
}
/// `Item = List / Pair / Number`, where `Pair` starts with a `Wrap` node
/// around a `Number`, so plain numbers leave a dead `Wrap` for compaction.
fn backtracking_list() -> &'static Language<'static> {
    let make = |name, expr, lexical| Rule {
        name,
        expr,
        lexical,
        owner: None,
        precedence: 0,
        except: &[],
        right_associative: false,
        transparent: false,
        alias: false,
        origin: None,
        action: Action::None,
    };
    const ITEM: Expr = Expr::Label("Items", &Expr::Ref(1));
    let rules = vec![
        make("Root", Expr::Ref(1), false),
        make(
            "Item",
            Expr::Choice(&[Expr::Ref(2), Expr::Ref(3), Expr::Ref(4)]),
            false,
        ),
        make(
            "List",
            Expr::Sequence(&[
                Expr::Literal("["),
                ITEM,
                Expr::Repeat {
                    inner: &Expr::Sequence(&[Expr::Literal(","), ITEM]),
                    min: 0,
                    max: u32::MAX,
                },
                Expr::Literal("]"),
            ]),
            false,
        ),
        make(
            "Pair",
            Expr::Sequence(&[Expr::Ref(5), Expr::Literal(":"), Expr::Ref(4)]),
            false,
        ),
        make(
            "Number",
            Expr::Repeat {
                inner: &Expr::Class(&[('0', '9')]),
                min: 1,
                max: u32::MAX,
            },
            true,
        ),
        make("Wrap", Expr::Label("Value", &Expr::Ref(4)), false),
    ];
    Box::leak(Box::new(Language {
        trivia: crate::TriviaPolicy::Nitrogen,
        profile: Profile::SyntaxOnly,
        rules: Box::leak(rules.into_boxed_slice()),
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        patterns: &[],
        source_hash: "backtracking-list",
    }))
}
fn list_source(items: usize) -> String {
    let items: Vec<_> = (0..items)
        .map(|i| match i % 3 {
            0 => format!("{i}"),
            1 => format!("{i}:{}", i + 1),
            _ => format!("[{i},{}]", i * 7),
        })
        .collect();
    format!("[{}]", items.join(","))
}
fn shape(snapshot: &ParseSnapshot) -> Vec<(u16, [u32; 2], u32)> {
    let tree = snapshot.materialize().unwrap();
    tree.nodes
        .iter()
        .map(|n| (n.rule, n.span, n.child_count))
        .collect()
}
fn strict() -> SnapshotMode {
    SnapshotMode::Strict(ParseOptions::default())
}
/// Replaces the first digit at or after `from` with another digit.
fn digit_edit(source: &str, from: usize) -> (TextEdit, String) {
    let at = from + source[from..].find(|c: char| c.is_ascii_digit()).unwrap();
    let digit = if &source[at..=at] == "1" { "2" } else { "1" };
    let mut text = source.to_string();
    text.replace_range(at..=at, digit);
    let edit = TextEdit {
        range: [at as u32, at as u32 + 1],
        replacement: digit.into(),
    };
    (edit, text)
}
#[test]
fn cold_snapshot_edit_reuses_its_facts() {
    let language = backtracking_list();
    let source = list_source(300);
    let snapshot = parse_snapshot(language, &source, Limits::default(), strict()).unwrap();
    let (edit, text) = digit_edit(&source, source.len() / 2);
    let edited = snapshot.edit(&edit, Limits::default()).unwrap();
    let fresh = parse_snapshot(language, &text, Limits::default(), strict()).unwrap();
    assert_eq!(shape(&edited), shape(&fresh));
    assert!(edited.stats.reused_subtrees > 0);
}
#[test]
fn remap_facts_follows_compaction_and_drops_reclaimed_syntax() {
    let key = Key {
        rule: 1,
        minimum: 0,
        in_token: 0,
    };
    let hit = |root| Hit {
        root,
        root_offset: 0,
        end_offset: 1,
        observed_offset: 1,
        furthest_offset: 1,
        depth: 0,
    };
    let (kept, moved, gone) = (
        SubtreeId::new(1, 4),
        SubtreeId::new(9, 0),
        SubtreeId::new(1, 5),
    );
    let mut facts = vec![
        (0, key, hit(kept)),
        (1, key, hit(SubtreeId::NONE)),
        (2, key, hit(gone)),
    ];
    remap_facts(&mut facts, |id| {
        (id == SubtreeId::NONE)
            .then_some(id)
            .or((id == kept).then_some(moved))
    });
    assert!(facts == vec![(0, key, hit(moved)), (1, key, hit(SubtreeId::NONE))]);
}
#[test]
fn eviction_order_grows_with_the_fact_log() {
    // Fact counts just past a power of two, where doubling overshoots.
    for items in [1700, 3200, 6200] {
        let source = list_source(items);
        let text = SourceText::new(&source).unwrap();
        let mut parser = Parser::new(
            backtracking_list(),
            Input {
                tables: None,
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: Some(&source),
            },
            Limits::default(),
            ParseOptions::default(),
            Limits::default().parser_stack_bytes,
            None,
            Index::default(),
        );
        parser.rule(0, 0, 0).unwrap().unwrap();
        assert!(
            parser.pending_order.capacity() <= parser.facts.capacity().max(4),
            "{items} items: order {} facts {}",
            parser.pending_order.capacity(),
            parser.facts.capacity()
        );
    }
}
#[test]
fn static_specialization_fallback_reuses_warmed_dispatch() {
    let language = backtracking_list();
    let mode = SnapshotMode::Strict(ParseOptions::default());
    let initial = parse_snapshot(language, "[1,2]", Limits::default(), mode).unwrap();
    let lexers = Box::leak(Box::new(crate::lexical::CompiledLexers::new(language, &[])));
    let fallback = parse_snapshot_compiled(lexers, "[1,2]", Limits::default(), mode).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        initial.dispatch.as_ref().unwrap(),
        fallback.dispatch.as_ref().unwrap()
    ));
}
#[test]
fn flat_recovery_keeps_owner_scoped_analysis_for_later_edits() {
    let language = backtracking_list();
    let mode = SnapshotMode::Recovering(RecoveryOptions {
        max_states: 0,
        ..Default::default()
    });
    let initial = parse_snapshot(language, "[1,2]", Limits::default(), mode).unwrap();
    let broken = initial
        .edit(
            &TextEdit {
                range: [4, 5],
                replacement: String::new(),
            },
            Limits::default(),
        )
        .unwrap();
    assert_eq!(broken.stats().full_parse_fallbacks, 1);
    assert!(
        broken.tables.is_some(),
        "flat fallback discarded its owner's analysis"
    );
    assert!(std::sync::Arc::ptr_eq(
        initial.dispatch.as_ref().unwrap(),
        broken.dispatch.as_ref().unwrap()
    ));
}
/// A memoized Pratt result replays its own depth. Recognized after a deep
/// sibling and replayed one rule deeper, it must not inherit the sibling's
/// depth: memoizing the point changes no depth-limit outcome.
#[test]
fn memoized_pratt_replays_its_own_depth() {
    let language = |memoize: bool| -> &'static Language<'static> {
        let template = prefix_fact_fixture(false);
        let make = |name, expr, owner| Rule {
            name,
            expr,
            owner,
            lexical: false,
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            origin: None,
            action: Action::None,
        };
        let rules = vec![
            // Root = Deep (Point "!" / Wrapped)
            make(
                "Root",
                Expr::Sequence(&[
                    Expr::Ref(1),
                    Expr::Choice(&[
                        Expr::Sequence(&[Expr::Ref(2), Expr::Literal("!")]),
                        Expr::Ref(4),
                    ]),
                ]),
                None,
            ),
            // Deep = "(" Deep ")" / "d"
            make(
                "Deep",
                Expr::Choice(&[
                    Expr::Sequence(&[Expr::Literal("("), Expr::Ref(1), Expr::Literal(")")]),
                    Expr::Literal("d"),
                ]),
                None,
            ),
            make(
                "Point",
                Expr::Pratt {
                    prefixes: &[3],
                    infixes: &[],
                    memoize,
                },
                None,
            ),
            make("Value", Expr::Literal("e"), Some(2)),
            // Wrapped = Point "?", one rule deeper than the first attempt.
            make(
                "Wrapped",
                Expr::Sequence(&[Expr::Ref(2), Expr::Literal("?")]),
                None,
            ),
        ];
        Box::leak(Box::new(Language {
            trivia: crate::TriviaPolicy::Nitrogen,
            rules: Box::leak(rules.into_boxed_slice()),
            ..*template
        }))
    };
    let (memoized, plain) = (language(true), language(false));
    let source = format!("{}d{}e?", "(".repeat(12), ")".repeat(12));
    let mut accepted = 0;
    for frames in 1..=96 {
        let limits = Limits {
            parser_frames: frames,
            ..Limits::default()
        };
        let expected = parse_snapshot(plain, &source, limits, strict()).is_ok();
        let actual = parse_snapshot(memoized, &source, limits, strict()).is_ok();
        assert_eq!(actual, expected, "frame limit {frames}");
        accepted += usize::from(expected);
    }
    // Both the rejecting and the accepting side of the limit are exercised.
    assert!(accepted > 0 && accepted < 96);
}
#[cfg(feature = "census")]
mod census {
    use super::*;
    use crate::{Census, DecisionKind, DecisionStats, census_begin, census_take};

    fn rule(name: &'static str, expr: Expr<'static>) -> Rule<'static> {
        Rule {
            name,
            expr,
            owner: None,
            lexical: false,
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            origin: None,
            action: Action::None,
        }
    }
    fn language(rules: Vec<Rule<'static>>) -> &'static Language<'static> {
        let template = prefix_fact_fixture(false);
        Box::leak(Box::new(Language {
            trivia: crate::TriviaPolicy::Nitrogen,
            rules: Box::leak(rules.into_boxed_slice()),
            ..*template
        }))
    }
    /// `Root = "a" "x" / "a" "y" / "b"`: leads decide every input.
    fn leads_decide() -> &'static Language<'static> {
        language(vec![rule(
            "Root",
            Expr::Choice(&[
                Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("x")]),
                Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("y")]),
                Expr::Literal("b"),
            ]),
        )])
    }
    /// `Root = "a" "b" "c" / "a" "b" "d"`: both alternatives share their lead.
    fn shared_leads() -> &'static Language<'static> {
        language(vec![rule(
            "Root",
            Expr::Choice(&[
                Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("b"), Expr::Literal("c")]),
                Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("b"), Expr::Literal("d")]),
            ]),
        )])
    }
    /// `Root = ("a" / "b") ("c" / "d")`.
    fn two_choices() -> &'static Language<'static> {
        language(vec![rule(
            "Root",
            Expr::Sequence(&[
                Expr::Choice(&[Expr::Literal("a"), Expr::Literal("b")]),
                Expr::Choice(&[Expr::Literal("c"), Expr::Literal("d")]),
            ]),
        )])
    }
    fn counted(language: &'static Language<'static>, source: &str, limits: Limits) -> Census {
        census_begin();
        parse_snapshot(language, source, limits, strict()).unwrap();
        census_take()
    }
    fn only(census: &Census, kind: DecisionKind) -> &DecisionStats {
        let mut found = census.decisions.iter().filter(|d| d.kind == kind);
        let first = found.next().expect("one decision of this kind");
        assert!(found.next().is_none(), "exactly one decision of this kind");
        first
    }

    /// `Root = Item* Tail`, `Item = "a" "x" / "a" "z"`, `Tail = "a" "y"`.
    fn lookahead_exit() -> &'static Language<'static> {
        language(vec![
            rule(
                "Root",
                Expr::Sequence(&[
                    Expr::Repeat {
                        inner: &Expr::Ref(1),
                        min: 0,
                        max: u32::MAX,
                    },
                    Expr::Ref(2),
                ]),
            ),
            rule(
                "Item",
                Expr::Choice(&[
                    Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("x")]),
                    Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("z")]),
                ]),
            ),
            rule(
                "Tail",
                Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("y")]),
            ),
        ])
    }
    /// `Root = Item* Tail`, `Item = "a" "x"`, `Tail = "a" "y"`.
    fn consuming_exit() -> &'static Language<'static> {
        language(vec![
            rule(
                "Root",
                Expr::Sequence(&[
                    Expr::Repeat {
                        inner: &Expr::Ref(1),
                        min: 0,
                        max: u32::MAX,
                    },
                    Expr::Ref(2),
                ]),
            ),
            rule(
                "Item",
                Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("x")]),
            ),
            rule(
                "Tail",
                Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("y")]),
            ),
        ])
    }

    #[test]
    fn census_lookahead_rejection_is_not_consumption() {
        // The final iteration's choice rejects both alternatives by their
        // second token: nothing is matched, so the exit consumed nothing.
        let census = counted(lookahead_exit(), "a x a y", Limits::default());
        assert_eq!((census.repeat_exits, census.repeat_consumed_exits), (1, 0));
        let choice = only(&census, DecisionKind::Choice);
        assert_eq!((choice.entries, choice.empty, choice.unmatched), (2, 1, 1));
        // Matching "a" before failing at "y" is consumption.
        let census = counted(consuming_exit(), "a x a y", Limits::default());
        assert_eq!((census.repeat_exits, census.repeat_consumed_exits), (1, 1));
    }
    /// `Root = Inner "x" / Inner "y"`, `Inner = "a" "b" "c" / "a" "b" "d"`.
    fn nested_failures() -> &'static Language<'static> {
        language(vec![
            rule(
                "Root",
                Expr::Choice(&[
                    Expr::Sequence(&[Expr::Ref(1), Expr::Literal("x")]),
                    Expr::Sequence(&[Expr::Ref(1), Expr::Literal("y")]),
                ]),
            ),
            rule(
                "Inner",
                Expr::Choice(&[
                    Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("b"), Expr::Literal("c")]),
                    Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("b"), Expr::Literal("d")]),
                ]),
            ),
        ])
    }
    fn work(language: &'static Language<'static>, source: &str) -> u64 {
        parse_snapshot(language, source, Limits::default(), strict())
            .unwrap()
            .stats()
            .work
    }

    #[test]
    fn census_measures_work_in_failed_attempts() {
        let census = counted(leads_decide(), "a y", Limits::default());
        assert_eq!(
            (
                only(&census, DecisionKind::Choice).failed_work,
                census.wasted_work
            ),
            (0, 0)
        );
        let census = counted(shared_leads(), "a b d", Limits::default());
        let failed = only(&census, DecisionKind::Choice).failed_work;
        assert!(failed > 0);
        assert_eq!(census.wasted_work, failed);
        assert!(census.wasted_work < work(shared_leads(), "a b d"));
    }
    #[test]
    fn census_counts_nested_waste_once() {
        let census = counted(nested_failures(), "a b d y", Limits::default());
        let failed = |rule: u16| {
            census
                .decisions
                .iter()
                .find(|d| d.rule == rule && d.kind == DecisionKind::Choice)
                .unwrap()
                .failed_work
        };
        let (outer, inner) = (failed(0), failed(1));
        // The inner failure inside the outer failed attempt is part of it;
        // the inner failure inside the outer successful attempt adds to it.
        assert!(census.wasted_work > outer);
        assert!(census.wasted_work < outer + inner);
        assert!(census.wasted_work < work(nested_failures(), "a b d y"));
    }
    #[test]
    fn census_counts_one_attempt_when_leads_decide() {
        for source in ["a y", "b"] {
            let census = counted(leads_decide(), source, Limits::default());
            let choice = only(&census, DecisionKind::Choice);
            assert_eq!(
                (
                    choice.entries,
                    choice.single,
                    choice.attempts,
                    choice.failed_attempts
                ),
                (1, 1, 1, 0),
                "{source}"
            );
        }
    }
    #[test]
    fn census_counts_backtracking_past_shared_leads() {
        let census = counted(shared_leads(), "a b d", Limits::default());
        let choice = only(&census, DecisionKind::Choice);
        assert_eq!(
            (
                choice.entries,
                choice.single,
                choice.attempts,
                choice.failed_attempts,
                choice.consumed_failures
            ),
            (1, 0, 2, 1, 1)
        );
    }
    #[test]
    fn census_counts_pratt_phases() {
        let census = counted(prefix_fact_fixture(true), "1+2", Limits::default());
        let prefix = census
            .decisions
            .iter()
            .find(|d| d.kind == DecisionKind::PrattPrefix && d.rule == 0)
            .expect("prefix phase");
        let infix = census
            .decisions
            .iter()
            .find(|d| d.kind == DecisionKind::PrattInfix && d.rule == 0)
            .expect("infix phase");
        assert!(prefix.attempts >= 1 && infix.attempts >= 1);
        let keys: Vec<_> = census
            .decisions
            .iter()
            .map(|d| (d.rule, d.kind as u8, d.ordinal))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
    #[test]
    fn census_keys_choices_by_rule_and_ordinal() {
        let census = counted(two_choices(), "b d", Limits::default());
        let ordinals: Vec<_> = census
            .decisions
            .iter()
            .filter(|d| d.kind == DecisionKind::Choice && d.rule == 0)
            .map(|d| d.ordinal)
            .collect();
        assert_eq!(ordinals, [0, 1]);
    }
    #[test]
    fn census_does_not_change_results() {
        for (language, source) in [
            (leads_decide(), "a y"),
            (shared_leads(), "a b d"),
            (two_choices(), "b d"),
            (prefix_fact_fixture(true), "1+2"),
        ] {
            let plain = parse_snapshot(language, source, Limits::default(), strict()).unwrap();
            census_begin();
            let counted = parse_snapshot(language, source, Limits::default(), strict()).unwrap();
            census_take();
            assert_eq!(shape(&plain), shape(&counted), "{source}");
            assert_eq!(plain.diagnostics(), counted.diagnostics(), "{source}");
            assert_eq!(plain.stats().work, counted.stats().work, "{source}");
        }
    }
    #[test]
    fn census_take_resets() {
        counted(leads_decide(), "a y", Limits::default());
        let census = counted(leads_decide(), "a y", Limits::default());
        assert_eq!(only(&census, DecisionKind::Choice).entries, 1);
    }
    /// `Root = "(" Root ")" / "d"`: one choice entry per nesting level.
    fn nested() -> &'static Language<'static> {
        language(vec![rule(
            "Root",
            Expr::Choice(&[
                Expr::Sequence(&[Expr::Literal("("), Expr::Ref(0), Expr::Literal(")")]),
                Expr::Literal("d"),
            ]),
        )])
    }
    #[test]
    fn census_counts_a_spilled_parse_once() {
        // A small first stack runs out partway through the nesting, after
        // some decisions; the whole parse then reruns on a spill thread.
        let depth = 400;
        let source = format!("{}d{}", "(".repeat(depth), ")".repeat(depth));
        let census = counted(
            nested(),
            &source,
            Limits {
                parser_stack_bytes: 32 * 1024,
                parser_spill_stack_bytes: 64 << 20,
                ..Limits::default()
            },
        );
        assert_eq!(
            only(&census, DecisionKind::Choice).entries,
            depth as u64 + 1
        );
    }
    #[test]
    fn census_counts_on_spill_threads() {
        let spilled = counted(
            leads_decide(),
            "a y",
            Limits {
                parser_stack_bytes: 1,
                parser_spill_stack_bytes: 8 << 20,
                ..Limits::default()
            },
        );
        let direct = counted(leads_decide(), "a y", Limits::default());
        let key = |c: &Census| {
            c.decisions
                .iter()
                .map(|d| (d.rule, d.ordinal, d.entries, d.single, d.attempts))
                .collect::<Vec<_>>()
        };
        assert!(!key(&direct).is_empty());
        assert_eq!(key(&spilled), key(&direct));
    }
}
