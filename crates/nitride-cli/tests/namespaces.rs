use nitride_core::*;
use serde_json::Value;

fn normalized(value: &Value) -> Value {
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(normalized).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(k, _)| k.as_str() != "message")
                .map(|(k, v)| (k.clone(), normalized(v)))
                .collect(),
        ),
        v => v.clone(),
    }
}

#[test]
fn binding_kinds_types_hir_and_effects_match_real_nitrogen() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/namespaces-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let actual = nitride_cli::observe_namespaces(case["source"].as_str().unwrap());
        assert_eq!(
            normalized(&actual),
            normalized(&case["observation"]),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn namespace_initializers_are_executed_once_and_references_keep_kind_identity() {
    let language = &nitride_cli::namespaces::LANGUAGE;
    let tree = parse(
        language,
        "let x:Time=duration(2); rate x:Time=duration(3); either(x); reverse(x); either(x);",
        Limits::default(),
    )
    .unwrap();
    let program = compile(language, &tree, Limits::default()).unwrap();
    assert_eq!(
        program.bindings.iter().map(|b| b.kind).collect::<Vec<_>>(),
        ["value", "rate", "value"]
    );
    let result = execute(
        &program,
        &nitride_cli::fixture_bindings_for(language),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(result.values, [2.0, 3.0, 2.0]);
    assert_eq!(result.trace, ["Namespaces.Duration", "Namespaces.Duration"]);
}
