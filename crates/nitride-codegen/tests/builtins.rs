use serde_json::{Value, json};
fn fixture() -> Value {
    let mut d: Value =
        serde_json::from_str(include_str!("../../../languages/inference/language.json")).unwrap();
    d["symbol_kinds"]
        .as_array_mut()
        .unwrap()
        .push(json!("function"));
    d["rules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["name"] == "Inference.ValueRef")
        .unwrap()["clauses"][0]["symbol_kinds"] = json!(["function"]);
    d["builtins"] = json!([{"kind":"function","name":"neg"}]);
    d["catalog"]["builtins"] = json!({"function":{"neg":["Inference.Duration"]}});
    d["catalog"]["calls"] = json!({"Inference.ValueRef":{"args":[]}});
    d
}
#[test]
fn builtin_call_tables_are_generated_from_explicit_catalog() {
    assert!(
        nitride_codegen::generate(&fixture().to_string())
            .unwrap()
            .contains("Action::Call")
    );
}
#[test]
fn malformed_unused_builtin_metadata_and_call_mappings_are_rejected() {
    let mut variants = Vec::new();
    let mut d = fixture();
    d["catalog"]["builtins"]["unknown"] = json!({});
    variants.push(d);
    let mut d = fixture();
    d["builtins"] = json!([{"kind":"missing","name":"neg"}]);
    variants.push(d);
    let mut d = fixture();
    d["builtins"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"function","name":"neg"}));
    variants.push(d);
    for ops in [
        json!([]),
        json!(["Unknown"]),
        json!(["Inference.Duration", "Inference.Duration"]),
        json!(false),
    ] {
        let mut d = fixture();
        d["catalog"]["builtins"]["function"]["neg"] = ops;
        variants.push(d);
    }
    let mut d = fixture();
    d["catalog"]["builtins"]["function"]["unused"] = json!(["Inference.Duration"]);
    variants.push(d);
    let mut d = fixture();
    d["catalog"]["calls"]["Unknown"] = json!({"args":[]});
    variants.push(d);
    let mut d = fixture();
    d["catalog"]["calls"]["Inference.Show"] = json!({"args":[]});
    variants.push(d);
    let mut d = fixture();
    d["catalog"]["calls"]["Inference.ValueRef"] = json!({"args":["Missing"]});
    variants.push(d);
    let mut d = fixture();
    d["catalog"].as_object_mut().unwrap().remove("builtins");
    variants.push(d);
    for d in variants {
        assert!(nitride_codegen::generate(&d.to_string()).is_err(), "{d}");
    }
}
