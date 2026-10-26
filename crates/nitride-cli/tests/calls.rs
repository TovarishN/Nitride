use nitride_core::*;
use serde_json::Value;

fn normalize(value: &Value) -> Value {
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(k, _)| k.as_str() != "message")
                .map(|(k, v)| (k.clone(), normalize(v)))
                .collect(),
        ),
        v => v.clone(),
    }
}

#[test]
fn builtin_calls_bindings_hir_and_effects_match_real_nitrogen() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/calls-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        assert_eq!(
            normalize(&nitride_cli::observe_calls(
                case["source"].as_str().unwrap()
            )),
            normalize(&case["observation"]),
            "{}",
            case["name"]
        );
    }
}
fn compile_source(source: &str) -> Program<'static> {
    let language = &nitride_cli::calls::LANGUAGE;
    let tree = parse(language, source, Limits::default()).unwrap();
    compile(language, &tree, Limits::default()).unwrap()
}

#[test]
fn calls_evaluate_arguments_once_left_to_right_and_reuse_initializers() {
    let program = compile_source("let x=sum(neg(2),neg(3)); x; x;");
    let report = execute(
        &program,
        &nitride_cli::fixture_bindings_for(program.language),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(report.values, [-5.0, -5.0]);
    assert_eq!(
        report.trace,
        ["Calls.NegScalar", "Calls.NegScalar", "Calls.SumScalar"]
    );
}

#[test]
fn missing_selected_call_binding_blocks_earlier_effects() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let program = compile_source("duration(1); neg(2);");
    let effects = Arc::new(AtomicUsize::new(0));
    let seen = effects.clone();
    let bindings = [HostBinding::new(program.language.operations[0], move |a| {
        seen.fetch_add(1, Ordering::SeqCst);
        Ok(a[0])
    })];
    assert_eq!(
        execute(&program, &bindings, Limits::default())
            .unwrap_err()
            .diagnostic
            .category,
        "host"
    );
    assert_eq!(effects.load(Ordering::SeqCst), 0);
}
