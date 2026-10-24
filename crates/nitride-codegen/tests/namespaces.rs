use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../languages/namespaces/language.json")).unwrap()
}

#[test]
fn explicit_namespace_inventory_admits_multiple_kinds() {
    nitride_codegen::generate(&fixture().to_string()).unwrap();
}

#[test]
fn unknown_empty_or_duplicate_namespace_inventories_are_rejected() {
    for inventory in [
        json!([]),
        json!(["value"]),
        json!(["value", "value", "rate"]),
        json!(["value", 3]),
    ] {
        let mut data = fixture();
        data["symbol_kinds"] = inventory;
        assert!(nitride_codegen::generate(&data.to_string()).is_err());
    }
}
