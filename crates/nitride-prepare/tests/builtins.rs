use nitride_prepare::{Source, prepare_sources};

#[test]
fn unscoped_builtin_inventory_is_preserved_and_validated() {
    let source = Source { path: "calls.ngr".into(), text: "syntax module Test { symbols { function } builtin function { neg sys.neg } syntax Start = \"x\"; }".into() };
    let data = prepare_sources(
        &[source],
        "{\"types\":[],\"operations\":[],\"statements\":{}}",
        "Test.Start",
        "syntax",
    )
    .unwrap();
    assert_eq!(
        data["builtins"],
        serde_json::json!([{"kind":"function","name":"neg"},{"kind":"function","name":"sys.neg"}])
    );
    for declaration in [
        "builtin missing { neg }",
        "builtin function in Start { neg }",
        "builtin function { }",
        "builtin function { neg neg }",
    ] {
        let source = Source {
            path: "bad.ngr".into(),
            text: format!(
                "syntax module Test {{ symbols {{ function }} {declaration} syntax Start = \"x\"; }}"
            ),
        };
        assert!(
            prepare_sources(
                &[source],
                "{\"types\":[],\"operations\":[],\"statements\":{}}",
                "Test.Start",
                "syntax"
            )
            .is_err(),
            "{declaration}"
        );
    }
}
