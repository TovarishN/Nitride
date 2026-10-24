use nitride_core::*;

static INNER: Hole = Hole {
    name: "Inner",
    category: 1,
};

fn language(extra: &'static [Helper]) -> &'static Language {
    let rule = |name, expr, action, precedence, owner| Rule {
        name,
        expr,
        lexical: false,
        action,
        except: &[],
        precedence,
        right_associative: false,
        owner,
    };
    let mut helpers = vec![
        Helper {
            name: "Loop",
            parameters: &[],
            result: 1,
            body: "loop",
            selector: None,
            arms: &[],
        },
        Helper {
            name: "Inc",
            parameters: std::slice::from_ref(&INNER),
            result: 1,
            body: " $Inner + 1 ",
            selector: None,
            arms: &[],
        },
    ];
    helpers.extend(extra.iter().map(|h| Helper { ..*h }));
    Box::leak(Box::new(Language {
        profile: Profile::Typed,
        start: 0,
        source_hash: "test",
        types: &["Core.Scalar"],
        handlers: &[],
        checks: &[],
        helpers: Box::leak(helpers.into_boxed_slice()),
        operations: &[Signature {
            id: "Add",
            inputs: &["Core.Scalar", "Core.Scalar"],
            result: "Core.Scalar",
        }],
        rules: Box::leak(
            vec![
                rule(
                    "Program",
                    Expr::Label("Value", &Expr::Ref(1)),
                    Action::Show { field: "Value" },
                    0,
                    None,
                ),
                rule(
                    "Expr",
                    Expr::Pratt {
                        prefixes: &[2, 3, 4],
                        infixes: &[5],
                    },
                    Action::None,
                    0,
                    None,
                ),
                rule(
                    "Num",
                    Expr::Label("Value", &Expr::Ref(6)),
                    Action::Literal {
                        ty: "Core.Scalar",
                        field: "Value",
                    },
                    0,
                    Some(1),
                ),
                rule(
                    "Loop",
                    Expr::Literal("loop"),
                    Action::Rewrite {
                        helper: 0,
                        fields: &[],
                        name: "Loop",
                    },
                    0,
                    Some(1),
                ),
                rule(
                    "Inc",
                    Expr::Sequence(&[Expr::Literal("inc"), Expr::Label("Inner", &Expr::Ref(1))]),
                    Action::Rewrite {
                        helper: 1,
                        fields: &["Inner"],
                        name: "Inc",
                    },
                    20,
                    Some(1),
                ),
                rule(
                    "Add",
                    Expr::Sequence(&[
                        Expr::Label("Left", &Expr::Ref(1)),
                        Expr::Literal("+"),
                        Expr::Label("Right", &Expr::Ref(1)),
                    ]),
                    Action::Operation {
                        id: 0,
                        fields: &["Left", "Right"],
                        texts: &[],
                    },
                    10,
                    Some(1),
                ),
                Rule {
                    lexical: true,
                    ..rule("Digit", Expr::Class(&[('0', '9')]), Action::None, 0, None)
                },
            ]
            .into_boxed_slice(),
        ),
    }))
}

fn compile_source(language: &'static Language, source: &str) -> Result<Program, Diagnostic> {
    let tree = parse(language, source, Limits::default()).unwrap();
    compile(language, &tree, Limits::default()).map_err(|mut e| e.remove(0))
}

#[test]
fn templates_parse_holes_as_single_nodes_relative_to_trimmed_text() {
    let language = language(&[]);
    let (tree, holes) =
        parse_template(language, "  $Inner + 1 ", 1, &[&INNER], Limits::default()).unwrap();
    assert_eq!(holes.len(), 1);
    assert_eq!(tree.nodes[holes[0].0 as usize].span, [0, 6]);
    assert_eq!(tree.nodes[tree.root as usize].span, [0, 10]);
    // Undeclared holes and holes of another category are not syntax.
    assert!(parse_template(language, "$Other + 1", 1, &[&INNER], Limits::default()).is_err());
    assert!(parse_template(language, "$Inner", 2, &[&INNER], Limits::default()).is_err());
}

#[test]
fn a_hole_template_lowers_with_the_caller_operand() {
    let language = language(&[]);
    let program = compile_source(language, "inc 2").unwrap();
    let bindings = [HostBinding::new(
        language.operations[0],
        |a| Ok(a[0] + a[1]),
    )];
    let report = execute(&program, &bindings, Limits::default()).unwrap();
    assert_eq!(report.values, [3.0]);
    // The root keeps the use origin first, then its template origin.
    let root = program.statements[0].value;
    assert_eq!(program.origins(root), [[0, 5], [0, 10]]);
    assert_eq!(program.origin_in_rewrite(root), [false, true]);
}

#[test]
fn a_rewrite_reaching_itself_is_a_diagnostic_at_the_use() {
    let error = compile_source(language(&[]), "loop").unwrap_err();
    assert_eq!(error.category, "type");
    assert_eq!(error.span, [0, 4]);
    assert!(error.message.contains("reaches itself"));
}

#[test]
fn invalid_unused_templates_fail_every_compile() {
    for body in ["+ +", "$Missing", "1 1"] {
        let extra = Box::leak(Box::new([Helper {
            name: "Bad",
            parameters: &[],
            result: 1,
            body,
            selector: None,
            arms: &[],
        }]));
        let error = compile_source(language(extra), "2").unwrap_err();
        assert_eq!(error.category, "grammar", "{body}");
        assert!(error.message.contains("invalid template in Bad"), "{body}");
    }
}
