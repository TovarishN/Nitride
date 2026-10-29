use serde_json::{Value, json};
fn fixture() -> Value {
    serde_json::from_str(
        &nitride_prepare::prepare_project(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../languages/functions/project.json"
        )))
        .unwrap()
        .data
        .to_string(),
    )
    .unwrap()
}
#[test]
fn declarative_templates_parameters_and_expansions_generate() {
    let code = nitride_codegen::generate(&fixture().to_string()).unwrap();
    for action in [
        "Action::Template",
        "Action::Parameter",
        "Action::Expand",
        "Action::Alias",
    ] {
        assert!(code.contains(action), "{action}");
    }
}
#[test]
fn invalid_template_clause_combinations_and_profile_mixing_are_rejected() {
    for (name, key, value) in [
        ("Functions.Definition", "field", json!("Missing")),
        ("Functions.Parameter", "sequential", json!(true)),
        ("Functions.Apply", "args", json!(["Missing"])),
        ("Functions.Definition", "export", json!(false)),
    ] {
        let mut d = fixture();
        let rule = d["rules"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["name"] == name)
            .unwrap();
        rule["clauses"][0][key] = value;
        assert!(nitride_codegen::generate(&d.to_string()).is_err());
    }
    let mut d = fixture();
    d["catalog"]["calls"] = json!({"Functions.Apply":{"args":[]}});
    assert!(nitride_codegen::generate(&d.to_string()).is_err());
}

#[test]
fn template_metadata_flags_and_unconsumed_policies_are_rejected() {
    let mut variants = Vec::new();
    let mut d = fixture();
    d["rules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["name"] == "Functions.Definition")
        .unwrap()["clauses"][1]["field"] = json!("Missing");
    variants.push(d);
    let mut d = fixture();
    d["rules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["name"] == "Functions.Parameter")
        .unwrap()["lexical"] = json!(true);
    variants.push(d);
    let mut d = fixture();
    d["catalog"]["statements"]["Functions.Parameter"] = json!({"kind":"let","value":"Name"});
    variants.push(d);
    for d in variants {
        assert!(nitride_codegen::generate(&d.to_string()).is_err());
    }
}
