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
        transparent: false,
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
        helpers: &[],
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

fn call_language() -> &'static Language {
    let rule = |name, expr, precedence, owner| Rule {
        name,
        expr,
        precedence,
        owner,
        lexical: false,
        except: &[],
        right_associative: false,
        transparent: false,
        action: Action::None,
    };
    Box::leak(Box::new(Language {
        profile: Profile::SyntaxOnly,
        start: 0,
        rules: Box::leak(
            vec![
                rule(
                    "E",
                    Expr::Pratt {
                        prefixes: &[1],
                        infixes: &[2, 3],
                    },
                    0,
                    None,
                ),
                rule("Id", Expr::Class(&[('a', 'z')]), 0, Some(0)),
                rule(
                    "Invoke",
                    Expr::Sequence(&[
                        Expr::Label("Target", &Expr::Ref(0)),
                        Expr::Label("Args", &Expr::Ref(4)),
                    ]),
                    180,
                    Some(0),
                ),
                rule(
                    "Member",
                    Expr::Sequence(&[
                        Expr::Label("Target", &Expr::Ref(0)),
                        Expr::Literal("."),
                        Expr::Label("Name", &Expr::Ref(5)),
                    ]),
                    180,
                    Some(0),
                ),
                rule(
                    "Arguments",
                    Expr::Sequence(&[
                        Expr::Literal("("),
                        Expr::Label("Value", &Expr::Ref(0)),
                        Expr::Literal(")"),
                    ]),
                    0,
                    None,
                ),
                rule("Name", Expr::Class(&[('a', 'z')]), 0, None),
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
fn call_postfix_with_reference_operator() {
    let language = call_language();
    let tree = parse(language, "f(x)(y).w", Limits::default()).unwrap();
    let name = |id: u32| language.rules[tree.nodes[id as usize].rule as usize].name;
    let target = |id: u32| {
        tree.children(id)
            .iter()
            .find(|e| e.label == "Target")
            .unwrap()
            .node
    };
    assert_eq!(name(tree.root), "Member");
    assert_eq!(name(target(tree.root)), "Invoke");
    assert_eq!(name(target(target(tree.root))), "Invoke");
    assert_eq!(name(target(target(target(tree.root)))), "Id");
    assert_eq!(tree.nodes[target(tree.root) as usize].span, [0, 7]);
}
