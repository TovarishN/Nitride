use nitride_core::*;

fn language(builtins: &'static [Builtin<'static>]) -> &'static Language<'static> {
    Box::leak(Box::new(Language {
        profile: Profile::Typed,
        source_hash: "test",
        start: 0,
        types: &["Core.Scalar"],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        operations: &[
            Signature {
                id: "A",
                inputs: &[],
                result: "Core.Scalar",
            },
            Signature {
                id: "B",
                inputs: &[],
                result: "Core.Scalar",
            },
        ],
        rules: Box::leak(
            vec![
                Rule {
                    name: "Call",
                    lexical: false,
                    expr: Expr::Sequence(&[
                        Expr::Label("Name", &Expr::Ref(1)),
                        Expr::Literal("("),
                        Expr::Literal(")"),
                    ]),
                    except: &[],
                    precedence: 0,
                    right_associative: false,
                    transparent: false,
                    alias: false,
                    owner: None,
                    action: Action::Call {
                        kinds: &["function"],
                        field: "Name",
                        fields: &[],
                        builtins,
                    },
                },
                Rule {
                    name: "Name",
                    lexical: true,
                    expr: Expr::Literal("f"),
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
    }))
}
#[test]
fn handcrafted_invalid_call_tables_return_structured_diagnostics() {
    for (candidates, category) in [
        (&[][..], "type"),
        (&[0, 1][..], "type"),
        (&[99][..], "grammar"),
    ] {
        let entries = Box::leak(
            vec![Builtin {
                kind: "function",
                name: "f",
                candidates,
            }]
            .into_boxed_slice(),
        );
        let language = language(entries);
        let tree = parse(language, "f()", Limits::default()).unwrap();
        assert_eq!(
            compile(language, &tree, Limits::default()).unwrap_err()[0].category,
            category
        );
    }
    let language = language(&[
        Builtin {
            kind: "function",
            name: "f",
            candidates: &[0],
        },
        Builtin {
            kind: "function",
            name: "f",
            candidates: &[0],
        },
    ]);
    let tree = parse(language, "f()", Limits::default()).unwrap();
    assert_eq!(
        compile(language, &tree, Limits::default()).unwrap_err()[0].category,
        "grammar"
    );
}
