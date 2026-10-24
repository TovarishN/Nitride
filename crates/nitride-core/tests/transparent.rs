use nitride_core::*;

fn language() -> &'static Language {
    let rule = |name, expr, lexical, owner| Rule {
        name,
        expr,
        lexical,
        action: Action::None,
        except: &[],
        precedence: 0,
        right_associative: false,
        transparent: false,
        owner,
    };
    Box::leak(Box::new(Language {
        profile: Profile::SyntaxOnly,
        start: 0,
        source_hash: "test",
        types: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        operations: &[],
        rules: Box::leak(
            vec![
                rule("Program", Expr::Label("Body", &Expr::Ref(1)), false, None),
                rule(
                    "Stmt",
                    Expr::Pratt {
                        prefixes: &[2, 3],
                        infixes: &[],
                    },
                    false,
                    None,
                ),
                Rule {
                    transparent: true,
                    ..rule(
                        "Plain",
                        Expr::Label("Body", &Expr::Choice(&[Expr::Ref(4), Expr::Literal(";")])),
                        false,
                        Some(1),
                    )
                },
                rule(
                    "Block",
                    Expr::Sequence(&[Expr::Literal("{"), Expr::Literal("}")]),
                    false,
                    Some(1),
                ),
                rule(
                    "Var",
                    Expr::Sequence(&[Expr::Literal("var"), Expr::Label("Name", &Expr::Ref(5))]),
                    false,
                    None,
                ),
                rule(
                    "Name",
                    Expr::Repeat {
                        inner: &Expr::Class(&[('a', 'z')]),
                        min: 1,
                        max: u32::MAX,
                    },
                    true,
                    None,
                ),
            ]
            .into_boxed_slice(),
        ),
    }))
}

#[test]
fn transparent_alternative_yields_its_element_node() {
    let language = language();
    let tree = parse(language, " var x", Limits::default()).unwrap();
    let body = tree.children(tree.root)[0];
    assert_eq!(
        language.rules[tree.nodes[body.node as usize].rule as usize].name,
        "Var"
    );
    assert_eq!(body.label, "Body");
    assert_eq!(tree.nodes[body.node as usize].span, [1, 6]);
    assert!(
        tree.nodes.iter().all(|n| n.rule != 2),
        "no Plain node remains"
    );
    // A literal element has no node to stand in; the alternative keeps one, hidden by observers.
    let tree = parse(language, ";", Limits::default()).unwrap();
    assert_eq!(
        tree.nodes[tree.children(tree.root)[0].node as usize].rule,
        2
    );
}
