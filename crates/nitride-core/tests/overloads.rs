use nitride_core::*;

fn language(candidates: &'static [u16]) -> &'static Language {
    let rule = |name, expr, lexical, action| Rule {
        name,
        expr,
        lexical,
        action,
        except: &[],
        precedence: 0,
        right_associative: false,
        owner: None,
    };
    Box::leak(Box::new(Language {
        profile: Profile::Typed,
        start: 0,
        source_hash: "test",
        types: &["Core.Scalar"],
        handlers: &[],
        checks: &[],
        helpers: &[],
        operations: &[
            Signature {
                id: "A",
                inputs: &["Core.Scalar", "Core.Scalar"],
                result: "Core.Scalar",
            },
            Signature {
                id: "B",
                inputs: &["Core.Scalar", "Core.Scalar"],
                result: "Core.Scalar",
            },
        ],
        rules: Box::leak(
            vec![
                rule(
                    "Start",
                    Expr::Sequence(&[
                        Expr::Label("Left", &Expr::Ref(1)),
                        Expr::Literal("+"),
                        Expr::Label("Right", &Expr::Ref(1)),
                    ]),
                    false,
                    Action::Overload {
                        candidates,
                        fields: &["Left", "Right"],
                    },
                ),
                rule(
                    "Number",
                    Expr::Label("Value", &Expr::Ref(2)),
                    false,
                    Action::Literal {
                        ty: "Core.Scalar",
                        field: "Value",
                    },
                ),
                rule("Token", Expr::Literal("1"), true, Action::None),
            ]
            .into_boxed_slice(),
        ),
    }))
}

#[test]
fn public_grammar_overload_failures_return_diagnostics_without_executable_programs() {
    for (candidates, category) in [
        (&[][..], "type"),
        (&[0, 1][..], "type"),
        (&[99][..], "grammar"),
    ] {
        let language = language(candidates);
        let tree = parse(language, "1+1", Limits::default()).unwrap();
        assert_eq!(
            compile(language, &tree, Limits::default()).unwrap_err()[0].category,
            category
        );
    }
}
