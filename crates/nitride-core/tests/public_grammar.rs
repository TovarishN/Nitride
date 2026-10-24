use nitride_core::*;

#[test]
fn invalid_pratt_rule_ids_return_diagnostics() {
    for expr in [
        Expr::Pratt {
            prefixes: &[99],
            infixes: &[],
            memoize: false,
        },
        Expr::Pratt {
            prefixes: &[1],
            infixes: &[99],
            memoize: false,
        },
    ] {
        let rule = |name, expr| Rule {
            name,
            lexical: false,
            expr,
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            owner: None,
            action: Action::None,
        };
        let language = Box::leak(Box::new(Language {
            profile: Profile::Typed,
            rules: Box::leak(
                vec![rule("Start", expr), rule("Value", Expr::Literal("x"))].into_boxed_slice(),
            ),
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "test",
        }));
        assert_eq!(
            parse(language, "x", Limits::default())
                .unwrap_err()
                .category,
            "grammar"
        );
    }
}
