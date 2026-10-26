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
fn inferred_types_overloads_origins_and_effects_match_real_nitrogen() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/inference-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        assert_eq!(
            normalize(&nitride_cli::observe_inference(
                case["source"].as_str().unwrap()
            )),
            normalize(&case["observation"]),
            "{}",
            case["name"]
        );
    }
}
fn compile_source(source: &str) -> Program<'static> {
    let language = &nitride_cli::inference::LANGUAGE;
    let tree = parse(language, source, Limits::default()).unwrap();
    compile(language, &tree, Limits::default()).unwrap()
}

#[test]
fn inferred_initializers_are_evaluated_once() {
    let program =
        compile_source("let x=duration(1); let y=value(x)+duration(2); value(y); value(y);");
    let result = execute(
        &program,
        &nitride_cli::fixture_bindings_for(program.language),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(result.values, [3.0, 3.0]);
    assert_eq!(
        result.trace,
        [
            "Inference.Duration",
            "Inference.Duration",
            "Inference.AddTime"
        ]
    );
}

#[test]
fn selected_concrete_overload_is_preflighted_before_any_effects() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let program = compile_source("let x=duration(1); value(x)+duration(2);");
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let bindings = [HostBinding::new(
        program.language.operations[0],
        move |args| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(args[0])
        },
    )];
    assert_eq!(
        execute(&program, &bindings, Limits::default())
            .unwrap_err()
            .diagnostic
            .category,
        "host"
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn long_inference_chain_uses_stored_types_and_values() {
    let mut source = String::from("let x0=1;\n");
    for i in 1..=1000 {
        source += &format!("let x{i}=value(x{})+1;\n", i - 1);
    }
    source += "value(x1000);";
    let program = compile_source(&source);
    assert_eq!(
        execute(
            &program,
            &nitride_cli::fixture_bindings_for(program.language),
            Limits::default()
        )
        .unwrap()
        .values,
        [1001.0]
    );
}
