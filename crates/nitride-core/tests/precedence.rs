use nitride_core::*;

fn language() -> Language {
    let rule = |name, expr, precedence, owner| Rule {
        name,
        expr,
        precedence,
        owner,
        lexical: false,
        except: &[],
        right_associative: false,
        action: Action::None,
    };
    Language {
        profile: Profile::SyntaxOnly,
        start: 0,
        rules: Box::leak(
            vec![
                rule(
                    "E",
                    Expr::Pratt {
                        prefixes: &[1, 2, 3],
                        infixes: &[4, 5],
                    },
                    0,
                    None,
                ),
                rule("Number", Expr::Class(&[('0', '9')]), 0, Some(0)),
                rule(
                    "Group",
                    Expr::Sequence(&[Expr::Literal("("), Expr::Ref(0), Expr::Literal(")")]),
                    100,
                    Some(0),
                ),
                rule(
                    "Neg",
                    Expr::Sequence(&[Expr::Literal("-"), Expr::Label("Value", &Expr::Ref(0))]),
                    10,
                    Some(0),
                ),
                rule(
                    "Add",
                    Expr::Sequence(&[Expr::Ref(0), Expr::Literal("+"), Expr::Ref(0)]),
                    10,
                    Some(0),
                ),
                rule(
                    "With",
                    Expr::Sequence(&[
                        Expr::Ref(0),
                        Expr::Literal("with"),
                        Expr::Literal("("),
                        Expr::Ref(0),
                        Expr::Literal(")"),
                    ]),
                    20,
                    Some(0),
                ),
            ]
            .into_boxed_slice(),
        ),
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        source_hash: "test",
    }
}

#[test]
fn enclosed_prefix_operand_resets_precedence() {
    let language = Box::leak(Box::new(language()));
    let tree = parse(language, "(1+2)", Limits::default()).unwrap();
    assert!(tree.nodes.iter().any(|node| node.rule == 4));
}

#[test]
fn enclosed_infix_operand_resets_precedence() {
    let language = Box::leak(Box::new(language()));
    let tree = parse(language, "1 with (2+3)", Limits::default()).unwrap();
    assert_eq!(tree.nodes[tree.root as usize].rule, 5);
    assert!(tree.nodes.iter().any(|node| node.rule == 4));
}

#[test]
fn prefix_excludes_equal_precedence_infix_from_its_final_operand() {
    let language = Box::leak(Box::new(language()));
    let tree = parse(language, "-1+2", Limits::default()).unwrap();
    assert_eq!(tree.nodes[tree.root as usize].rule, 4);
}
