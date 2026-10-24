use nitride_prepare::{Source, prepare_sources};
#[test]
fn separated_literal_lists_are_normalized_into_existing_parser_forms() {
    let source = Source {
        path: "list.ngr".into(),
        text: "syntax module L { syntax Start=Items:(Item; \",\")*; syntax Item=\"a\"; }".into(),
    };
    let data = prepare_sources(
        &[source],
        "{\"types\":[],\"operations\":[],\"statements\":{}}",
        "L.Start",
        "syntax",
    )
    .unwrap();
    assert_eq!(data["rules"][0]["expr"]["inner"]["kind"], "repeat");
}
#[test]
fn declarative_template_fixture_prepares_without_csharp() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../languages/functions/project.json");
    let prepared = nitride_prepare::prepare_project(&root).unwrap();
    assert!(prepared.data["rules"].as_array().unwrap().iter().any(|r| {
        r["clauses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "LowersTemplate")
    }));
}
