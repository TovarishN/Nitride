use nitride_codegen::generate;

#[test]
fn generates_static_language_from_export() {
    let source = include_str!("../../../languages/portable/language.json");
    let result = generate(source).expect("supported normalized grammar");
    assert!(result.contains("pub static LANGUAGE"));
    assert_eq!(generate(source).unwrap(), result);
}

#[test]
fn unknown_format_version_is_rejected() {
    let mut data: serde_json::Value =
        serde_json::from_str(include_str!("../../../languages/portable/language.json")).unwrap();
    data["version"] = 99.into();
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn zero_width_repeat_is_rejected_even_after_consuming_prefix() {
    let mut data: serde_json::Value =
        serde_json::from_str(include_str!("../../../languages/portable/language.json")).unwrap();
    data["rules"][0]["expr"] = serde_json::json!({"kind":"sequence","items":[{"kind":"literal","text":"x"},{"kind":"repeat","min":0,"max":100,"inner":{"kind":"literal","text":""}}]});
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn unknown_operation_type_and_field_are_rejected() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("../../../languages/portable/language.json")).unwrap();
    let mut data = original.clone();
    data["catalog"]["operations"][0]["result"] = "Missing.Type".into();
    assert!(generate(&data.to_string()).is_err());
    let mut data = original;
    data["catalog"]["statements"]["Portable.Show"]["value"] = "Missing".into();
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn incompatible_symbol_kinds_are_not_erased() {
    let mut data: serde_json::Value =
        serde_json::from_str(include_str!("../../../languages/portable/language.json")).unwrap();
    for rule in data["rules"].as_array_mut().unwrap() {
        for clause in rule["clauses"].as_array_mut().unwrap() {
            if clause["kind"] == "References" {
                clause["symbol_kinds"] = serde_json::json!(["other"]);
            }
        }
    }
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn unknown_or_incompatible_fixture_host_handlers_fail_generation() {
    for handler in ["add", "unknown"] {
        let mut data: serde_json::Value =
            serde_json::from_str(include_str!("../../../languages/portable/language.json"))
                .unwrap();
        data["catalog"]["operations"][0]["handler"] = handler.into();
        assert!(generate(&data.to_string()).is_err());
    }
}
