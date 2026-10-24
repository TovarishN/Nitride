use nitride_cli::observe;

fn normalize_numbers(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.iter().map(normalize_numbers).collect()),
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(k, v)| (k.clone(), normalize_numbers(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn frozen_nitrogen_observations_match() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let actual = observe(case["source"].as_str().unwrap());
        let expected = &case["observation"];
        assert_eq!(actual["ok"], expected["ok"], "{}: {actual}", case["name"]);
        if expected["ok"] == true {
            for key in ["syntax", "bindings", "hir", "values", "trace"] {
                assert_eq!(
                    normalize_numbers(&actual[key]),
                    normalize_numbers(&expected[key]),
                    "{} / {key}",
                    case["name"]
                );
            }
        } else {
            assert_eq!(
                actual["error"], expected["error"],
                "{}: {actual}",
                case["name"]
            );
            assert_eq!(actual["trace"], expected["trace"], "{}", case["name"]);
            if expected.get("span").is_some() {
                assert_eq!(actual["span"], expected["span"], "{}", case["name"]);
            }
        }
    }
}
