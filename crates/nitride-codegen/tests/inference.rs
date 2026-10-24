use serde_json::{Value, json};
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../languages/inference/language.json")).unwrap()
}

#[test]
fn explicit_inference_and_nominal_overload_groups_generate() {
    nitride_codegen::generate(&fixture().to_string()).unwrap();
}

#[test]
fn untyped_or_conflicting_inference_policy_is_rejected() {
    for value in [Value::Null, json!(false), json!("yes")] {
        let mut data = fixture();
        data["catalog"]["statements"]["Inference.InferValue"]["infer"] = value;
        assert!(nitride_codegen::generate(&data.to_string()).is_err());
    }
    let mut data = fixture();
    data["catalog"]["statements"]["Inference.LetValue"]["infer"] = json!(true);
    assert!(nitride_codegen::generate(&data.to_string()).is_err());
}

#[test]
fn malformed_or_ambiguous_overload_groups_are_rejected_even_when_unused() {
    for group in [
        json!([]),
        json!(["Missing"]),
        json!(["Inference.AddScalar", "Inference.AddScalar"]),
        json!([3]),
    ] {
        let mut data = fixture();
        data["catalog"]["overloads"]["Unused.Group"] = group;
        assert!(nitride_codegen::generate(&data.to_string()).is_err());
    }
    let mut data = fixture();
    let mut duplicate = data["catalog"]["operations"][1].clone();
    duplicate["id"] = json!("Inference.Duplicate");
    duplicate["result"] = json!("Inference.Time");
    data["catalog"]["operations"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    data["catalog"]["overloads"]["Inference.Add"] =
        json!(["Inference.AddScalar", "Inference.Duplicate"]);
    assert!(nitride_codegen::generate(&data.to_string()).is_err());
}

#[test]
fn inference_policy_cannot_be_ignored_on_non_declarations() {
    for policy in [json!(true), json!("yes")] {
        let mut data = fixture();
        data["catalog"]["statements"]["Inference.Show"]["infer"] = policy;
        assert!(nitride_codegen::generate(&data.to_string()).is_err());
    }
}
