use nitride_core::*;

#[test]
fn existing_xml_parses_nested_unicode_and_comments() {
    let source =
        "<?xml version='1.0'?><!--before--><root title=\"λ 😀\"><child/>hello<!--x--></root>";
    let tree = parse(&nitride_cli::xml::LANGUAGE, source, Limits::default()).unwrap();
    assert!(
        tree.nodes
            .iter()
            .any(|n| nitride_cli::xml::LANGUAGE.rules[n.rule as usize].name == "Xml.Comment")
    );
    assert!(compile(&nitride_cli::xml::LANGUAGE, &tree, Limits::default()).is_err());
}

#[test]
fn syntax_admission_does_not_claim_closing_tag_semantics() {
    let tree = parse(
        &nitride_cli::xml::LANGUAGE,
        "<open></different>",
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        compile(&nitride_cli::xml::LANGUAGE, &tree, Limits::default()).unwrap_err()[0].category,
        "capability"
    );
}

#[test]
fn existing_cross_module_grammar_includes_extensions() {
    for source in ["doc let x = -1; (name) end", "doc let x = word; end"] {
        let tree = parse(
            &nitride_cli::composition::LANGUAGE,
            source,
            Limits::default(),
        )
        .unwrap();
        assert!(!tree.nodes.is_empty());
    }
}
