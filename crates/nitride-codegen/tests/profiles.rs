use nitride_codegen::generate;

#[test]
fn composed_typed_and_existing_syntax_profiles_generate() {
    for input in [
        include_str!("../../../languages/power/language.json"),
        include_str!("../../../languages/composition/language.json"),
        include_str!("../../../languages/xml/language.json"),
    ] {
        assert!(generate(input).is_ok());
    }
}

#[test]
fn syntax_profile_cannot_smuggle_semantic_actions() {
    let mut data: serde_json::Value =
        serde_json::from_str(include_str!("../../../languages/portable/language.json")).unwrap();
    data["mode"] = "syntax".into();
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn conflicting_infix_operators_are_rejected() {
    let mut data: serde_json::Value =
        serde_json::from_str(include_str!("../../../languages/portable/language.json")).unwrap();
    let mut twin = data["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Portable.Add")
        .unwrap()
        .clone();
    twin["name"] = "Portable.AddTwin".into();
    data["rules"].as_array_mut().unwrap().push(twin);
    for rule in data["rules"].as_array_mut().unwrap() {
        if rule["name"] == "Portable.Expr" {
            rule["expr"]["infixes"]
                .as_array_mut()
                .unwrap()
                .push("Portable.AddTwin".into());
        }
    }
    assert!(generate(&data.to_string()).is_err());
}
