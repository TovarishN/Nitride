use nitride_core::*;

fn rule<'a>(name: &'a str, lexical: bool, expr: Expr<'a>) -> Rule<'a> {
    Rule {
        name,
        lexical,
        expr,
        except: &[],
        precedence: 0,
        right_associative: false,
        transparent: false,
        alias: false,
        owner: None,
        action: Action::None,
    }
}

/// A language whose tables live in local, dropped storage rather than in statics.
#[test]
fn borrowed_language_parses() {
    let names = [String::from("Doc"), String::from("Number")];
    let digits = [('0', '9')];
    let number = Expr::Class(&digits);
    let rules = vec![
        rule(
            &names[0],
            false,
            Expr::Repeat {
                inner: &Expr::Ref(1),
                min: 0,
                max: u32::MAX,
            },
        ),
        rule(
            &names[1],
            true,
            Expr::Repeat {
                inner: &number,
                min: 1,
                max: u32::MAX,
            },
        ),
    ];
    let language = Language {
        profile: Profile::SyntaxOnly,
        rules: &rules,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        source_hash: "",
    };
    let tree = parse(&language, "12 34", Limits::default()).unwrap();
    assert_eq!(tree.nodes.len(), 3);
    drop(tree);
    drop(rules);
}
