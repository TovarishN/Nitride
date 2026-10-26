use nitride_core::*;
use serde_json::Value;

const TODAY: f64 = 739_912.0; // 2026-10-24, the frozen reference date.

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
fn quote_rewrites_origins_and_effects_match_real_nitrogen() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/rewrites-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        assert_eq!(
            normalize(&nitride_cli::observe_rewrites(
                case["source"].as_str().unwrap(),
                TODAY
            )),
            normalize(&case["observation"]),
            "{}",
            case["name"]
        );
    }
}

fn compile_source(source: &str) -> Result<Program<'static>, Diagnostic> {
    let language = &nitride_cli::rewrites::LANGUAGE;
    let tree = parse(language, source, Limits::default()).unwrap();
    compile(language, &tree, Limits::default()).map_err(|mut e| e.remove(0))
}

#[test]
fn only_reachable_hir_is_emitted_and_repeated_holes_are_copied() {
    // `count + 2` is discarded syntax: only the swapped addition remains.
    let program = compile_source("let count = 3; count + 2 swapped;").unwrap();
    assert_eq!(program.nodes.len(), 4);
    let program = compile_source("twice(2 days);").unwrap();
    let days = program
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, HirKind::Operation { operation, .. } if program.language.operations[operation as usize].id == "Dates.Days"))
        .count();
    assert_eq!(days, 2);
}

#[test]
fn nested_repeated_holes_are_bounded() {
    // Each nested `twice` doubles the emitted tree: 2^16 leaves exceed the work bound.
    let language = &nitride_cli::rewrites::LANGUAGE;
    let source = format!("{}1{};", "twice(".repeat(16), ")".repeat(16));
    let limits = Limits {
        work: 10_000,
        ..Limits::default()
    };
    let tree = parse(language, &source, Limits::default()).unwrap();
    let error = compile(language, &tree, limits).unwrap_err().remove(0);
    assert_eq!(error.category, "limit");
}
