use nitride_core::*;
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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
fn builtin_values_text_literals_and_checks_match_real_nitrogen() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/dates-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        assert_eq!(
            normalize(&nitride_cli::observe_dates(
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
    let language = &nitride_cli::dates::LANGUAGE;
    let tree = parse(language, source, Limits::default()).unwrap();
    compile(language, &tree, Limits::default()).map_err(|mut e| e.remove(0))
}

fn counted_values(reads: &Arc<AtomicUsize>) -> Vec<HostValue> {
    let (pi, today) = (reads.clone(), reads.clone());
    vec![
        HostValue::new("value", "pi", "Core.Scalar", move || {
            pi.fetch_add(1, Ordering::SeqCst);
            Ok(3.0)
        }),
        HostValue::new("value", "today", "Dates.Date", move || {
            today.fetch_add(1, Ordering::SeqCst);
            Ok(TODAY)
        }),
    ]
}

#[test]
fn each_builtin_value_is_read_once_per_execution() {
    let program = compile_source("today - today; (today - 2026-10-05) in days; pi;").unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    let report = execute_with_values(
        &program,
        &nitride_cli::fixture_bindings_for(program.language),
        &counted_values(&reads),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(report.values, [0.0, 19.0, 3.0]);
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}

#[test]
fn checks_are_diagnostics_with_codes_before_any_host_effect() {
    let error = compile_source("let a = today; a + 1 days; 2026-04-31;").unwrap_err();
    assert_eq!(error.category, "check");
    assert_eq!(error.code.as_deref(), Some("DATE0001"));
    assert_eq!(error.span, [27, 37]);
    // An ordinary type diagnostic keeps no code.
    assert_eq!(compile_source("today + today;").unwrap_err().code, None);
}

#[test]
fn missing_mistyped_or_duplicate_host_values_fail_preflight_without_reads() {
    let program = compile_source("2026-10-05; today;").unwrap();
    let bindings = nitride_cli::fixture_bindings_for(program.language);
    let reads = Arc::new(AtomicUsize::new(0));
    let mut duplicated = counted_values(&reads);
    duplicated.extend(counted_values(&reads));
    let counter = reads.clone();
    let mistyped = vec![HostValue::new("value", "today", "Core.Scalar", move || {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(0.0)
    })];
    for values in [Vec::new(), duplicated, mistyped] {
        let failure =
            execute_with_values(&program, &bindings, &values, Limits::default()).unwrap_err();
        assert_eq!(failure.diagnostic.category, "host");
        assert!(failure.trace.is_empty());
    }
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}

#[test]
fn text_operations_require_text_handlers() {
    let program = compile_source("2026-10-05;").unwrap();
    let numeric = [HostBinding::new(program.language.operations[0], |_| {
        Ok(0.0)
    })];
    assert_eq!(
        execute(&program, &numeric, Limits::default())
            .unwrap_err()
            .diagnostic
            .category,
        "host"
    );
    let effects = Arc::new(AtomicUsize::new(0));
    let seen = effects.clone();
    let text = [HostBinding::text(
        program.language.operations[0],
        move |spelling| {
            seen.fetch_add(1, Ordering::SeqCst);
            assert_eq!(spelling, "2026-10-05");
            Ok(1.0)
        },
    )];
    assert_eq!(
        execute(&program, &text, Limits::default()).unwrap().values,
        [1.0]
    );
    assert_eq!(effects.load(Ordering::SeqCst), 1);
}

#[test]
fn iso_dates_follow_the_proleptic_gregorian_calendar() {
    for (text, day) in [
        ("0001-01-01", Some(0)),
        ("1970-01-01", Some(719_162)),
        ("2000-02-29", Some(730_178)),
        ("2026-10-24", Some(739_912)),
        ("9999-12-31", Some(3_652_058)),
        ("1900-02-29", None),
        ("2026-00-10", None),
        ("2026-1-10", None),
        ("2026-10-5x", None),
        ("+026-10-05", None),
    ] {
        assert_eq!(iso_date_day_number(text), day, "{text}");
    }
}

#[test]
fn handcrafted_text_hir_cannot_escape_text_operations() {
    let bindings = |program: &Program<'static>| nitride_cli::fixture_bindings_for(program.language);
    // A text input fed by a non-text node.
    let mut program = compile_source("2026-10-05;").unwrap();
    program.nodes[0].kind = HirKind::Constant(1.0);
    assert!(preflight(&program, &bindings(&program)).is_err());
    // Text fed into a numeric operation input.
    let mut program = compile_source("2026-10-05 - 2026-10-06;").unwrap();
    let HirKind::Operation { args, .. } = &program.nodes.last().unwrap().kind else {
        panic!("operation root");
    };
    let first = program.args[args.start as usize] as usize;
    program.args[args.start as usize] = first as u32 - 1;
    assert!(preflight(&program, &bindings(&program)).is_err());
    // A text node that is a statement result.
    let mut program = compile_source("2026-10-05;").unwrap();
    program.nodes.pop();
    program.statements[0].value = 0;
    assert!(preflight(&program, &bindings(&program)).is_err());
}
