use nitride_cli::{observe_power, syntax_observe};
use serde_json::Value;

fn normalize(value: &Value) -> Value {
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(k, v)| (k.clone(), normalize(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn existing_language_tree_spans_and_parents_match_nitrogen() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/syntax-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let language = if case["profile"] == "xml" {
            &nitride_cli::xml::LANGUAGE
        } else {
            &nitride_cli::composition::LANGUAGE
        };
        let actual = syntax_observe(language, case["source"].as_str().unwrap());
        let expected = &case["observation"];
        assert_eq!(actual, *expected, "{}: {}", case["name"], case["source"]);
    }
}

#[test]
fn typed_cross_module_extension_matches_nitrogen() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/power-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let actual = observe_power(case["source"].as_str().unwrap());
        let expected = &case["observation"];
        assert_eq!(actual["ok"], expected["ok"], "{}: {actual}", case["name"]);
        if expected["ok"] == true {
            for key in ["syntax", "bindings", "hir", "values", "trace"] {
                assert_eq!(
                    normalize(&actual[key]),
                    normalize(&expected[key]),
                    "{}: {key}",
                    case["name"]
                );
            }
        } else {
            for key in ["error", "trace"] {
                assert_eq!(actual[key], expected[key], "{}: {key}", case["name"]);
            }
            if expected.get("span").is_some() {
                assert_eq!(actual["span"], expected["span"], "{}", case["name"]);
            }
        }
    }
}
