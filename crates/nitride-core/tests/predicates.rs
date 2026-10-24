use nitride_core::*;

fn language(expr: Expr) -> &'static Language {
    Box::leak(Box::new(Language {
        profile: Profile::SyntaxOnly,
        start: 0,
        rules: Box::leak(
            vec![
                Rule {
                    name: "Start",
                    lexical: true,
                    expr,
                    except: &[],
                    precedence: 0,
                    right_associative: false,
                    transparent: false,
                    alias: false,
                    owner: None,
                    action: Action::None,
                },
                Rule {
                    name: "Word",
                    lexical: true,
                    expr: Expr::Literal("x"),
                    except: &[],
                    precedence: 0,
                    right_associative: false,
                    transparent: false,
                    alias: false,
                    owner: None,
                    action: Action::None,
                },
            ]
            .into_boxed_slice(),
        ),
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        source_hash: "test",
    }))
}

#[test]
fn lookahead_restores_tree_and_position() {
    let lang = language(Expr::Sequence(&[
        Expr::Predicate {
            positive: true,
            inner: &Expr::Ref(1),
        },
        Expr::Ref(1),
    ]));
    // Token internals are kept here so a leaked speculative node would show.
    let options = ParseOptions {
        token_children: true,
        ..Default::default()
    };
    let tree = parse_with_options(lang, "x", Limits::default(), options).unwrap();
    assert_eq!(tree.nodes.len(), 2);
    assert_eq!(tree.nodes[tree.root as usize].span, [0, 1]);
    // By default the token is a leaf.
    assert_eq!(parse(lang, "x", Limits::default()).unwrap().nodes.len(), 1);
}

#[test]
fn negative_lookahead_does_not_leak_speculative_failure_span() {
    let lang = language(Expr::Sequence(&[
        Expr::Predicate {
            positive: false,
            inner: &Expr::Sequence(&[Expr::Literal("x"), Expr::Literal("@"), Expr::Literal("y")]),
        },
        Expr::Literal("x"),
    ]));
    assert_eq!(
        parse(lang, "x@z", Limits::default()).unwrap_err().span,
        [1, 1]
    );
}

fn prefix_language(prefixes: &'static [u16], second: Expr) -> &'static Language {
    let rule = |name, expr| Rule {
        name,
        lexical: false,
        expr,
        except: &[],
        precedence: 0,
        right_associative: false,
        transparent: false,
        alias: false,
        owner: None,
        action: Action::None,
    };
    Box::leak(Box::new(Language {
        profile: Profile::SyntaxOnly,
        start: 0,
        rules: Box::leak(
            vec![
                rule(
                    "Expr",
                    Expr::Pratt {
                        prefixes,
                        infixes: &[],
                        memoize: false,
                    },
                ),
                rule("Short", Expr::Literal("a")),
                rule("Long", second),
            ]
            .into_boxed_slice(),
        ),
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        source_hash: "test",
    }))
}

#[test]
fn extensible_prefix_uses_longest_match_independent_of_module_order() {
    for order in [&[1, 2][..], &[2, 1][..]] {
        let language = prefix_language(
            order,
            Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("b")]),
        );
        let tree = parse(language, "a b", Limits::default()).unwrap();
        assert_eq!(
            language.rules[tree.nodes[tree.root as usize].rule as usize].name,
            "Long"
        );
    }
}

#[test]
fn equally_ranked_prefixes_are_reported_as_unadmitted_ambiguity() {
    let language = prefix_language(&[1, 2], Expr::Literal("a"));
    assert_eq!(
        parse(language, "a", Limits::default())
            .unwrap_err()
            .category,
        "ambiguity"
    );
}
