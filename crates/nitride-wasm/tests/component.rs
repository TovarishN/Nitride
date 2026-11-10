use nitride_core::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

static LANGUAGE: Language<'static> = Language {
    trivia: TriviaPolicy::Nitrogen,
    profile: Profile::SyntaxOnly,
    rules: &[],
    start: 0,
    types: &[],
    operations: &[
        Signature {
            id: "Test.Between",
            inputs: &["Test.Date", "Test.Date"],
            result: "Core.Scalar",
        },
        Signature {
            id: "Test.WrongInputs",
            inputs: &["Core.Scalar", "Core.Scalar"],
            result: "Core.Scalar",
        },
        Signature {
            id: "Test.WrongResult",
            inputs: &["Test.Date", "Test.Date"],
            result: "Test.Date",
        },
        Signature {
            id: "Test.ParseDate",
            inputs: &["Core.Text"],
            result: "Test.Date",
        },
        Signature {
            id: "Test.Mondays",
            inputs: &["Test.Span"],
            result: "Test.Dates",
        },
        Signature {
            id: "Test.WrongFields",
            inputs: &["Test.WrongSpan"],
            result: "Test.Dates",
        },
    ],
    handlers: &["Test.API@fixture"; 6],
    checks: &[],
    helpers: &[],
    rule_checks: &[],
    patterns: &[],
    source_hash: "wasm-unit-fixture",
};
fn component() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/calendar/gregorian.wasm")
}
fn prepared() -> Value {
    json!({"catalog":{
        "representations":{"Test.Date":"i64","Test.Span":{"record":[["start","Test.Date"],["end","Test.Date"]]},"Test.WrongSpan":{"record":[["left","Test.Date"],["right","Test.Date"]]},"Test.Dates":{"list":"Test.Date"}},
        "interfaces":{"Test.API":{"version":1}},
        "operations":LANGUAGE.operations.iter().map(|s|json!({"id":s.id,"inputs":s.inputs,"result":s.result,"interface":"Test.API","fails":["Test.InvalidDate","Test.OutOfRange","Test.PartialDay"]})).collect::<Vec<_>>(),
        "failures":{"Test.InvalidDate":"declared invalid date","Test.OutOfRange":"declared range","Test.PartialDay":"declared partial day"}
    }})
}
fn binding(op: &str, export: Value) -> Value {
    json!({"interface":"Test.API","contract":"Test.API@fixture","export":"nitride:calendar/gregorian@3.0.0","exports":{op:export}})
}

#[test]
fn direct_component_calls_keep_values_records_lists_and_declared_failures() {
    let prepared = prepared();
    let between = nitride_wasm::bindings(
        &LANGUAGE,
        &prepared,
        &binding("Test.Between", json!("between")),
        &component(),
    )
    .unwrap()
    .remove(0);
    for _ in 0..2 {
        assert_eq!(
            between
                .call(
                    &[Datum::Integer(3), Datum::Integer(1)],
                    None,
                    &mut Vec::new()
                )
                .unwrap(),
            2.0
        );
    }
    let mondays = nitride_wasm::bindings(
        &LANGUAGE,
        &prepared,
        &binding("Test.Mondays", json!("mondays")),
        &component(),
    )
    .unwrap()
    .remove(0);
    let span = Datum::Record(vec![Datum::Integer(1), Datum::Integer(14)].into());
    assert_eq!(
        mondays.call(&[span], None, &mut Vec::new()).unwrap(),
        Datum::List(vec![Datum::Integer(7), Datum::Integer(14)].into())
    );
    let date = nitride_wasm::bindings(
        &LANGUAGE,
        &prepared,
        &binding("Test.ParseDate", json!("parse-iso")),
        &component(),
    )
    .unwrap()
    .remove(0);
    let error = date
        .call(&[], Some("2024-02-30"), &mut Vec::new())
        .unwrap_err();
    assert_eq!(error.failure.as_deref(), Some("Test.InvalidDate"));
    assert_eq!(error.message, "declared invalid date");
}

#[test]
fn same_arity_type_mismatches_are_rejected_when_binding() {
    let prepared = prepared();
    for (op, export) in [
        ("Test.WrongInputs", "between"),
        ("Test.WrongResult", "between"),
        ("Test.WrongFields", "mondays"),
    ] {
        assert!(
            nitride_wasm::bindings(
                &LANGUAGE,
                &prepared,
                &binding(op, json!(export)),
                &component()
            )
            .is_err(),
            "{op}"
        );
    }
}

#[test]
fn invalid_export_metadata_returns_an_error_instead_of_panicking() {
    assert!(
        nitride_wasm::bindings(
            &LANGUAGE,
            &prepared(),
            &binding("Test.Between", json!(17)),
            &component()
        )
        .is_err()
    );
}

#[test]
fn exhausted_fuel_does_not_poison_unmetered_component_loading() {
    let prepared = prepared();
    let binding = binding("Test.Between", json!("between"));
    let message =
        match nitride_wasm::bounded_bindings(&LANGUAGE, &prepared, &binding, &component(), Some(0))
        {
            Err(error) => error,
            Ok(mut bindings) => {
                let error = bindings
                    .remove(0)
                    .call(
                        &[Datum::Integer(3), Datum::Integer(1)],
                        None,
                        &mut Vec::new(),
                    )
                    .unwrap_err();
                assert_eq!(error.failure.as_deref(), Some("host.component"));
                error.message
            }
        };
    assert!(message.contains("fuel"), "{message}");
    let binding = nitride_wasm::bindings(&LANGUAGE, &prepared, &binding, &component())
        .unwrap()
        .remove(0);
    assert_eq!(
        binding
            .call(
                &[Datum::Integer(3), Datum::Integer(1)],
                None,
                &mut Vec::new()
            )
            .unwrap(),
        2.0
    );
}

#[test]
fn generated_record_names_with_repeated_underscores_remain_compatible() {
    let mut prepared = prepared();
    prepared["catalog"]["representations"]["Test.Span"]["record"] =
        json!([["start__", "Test.Date"], ["__end", "Test.Date"]]);
    let binding = nitride_wasm::bindings(
        &LANGUAGE,
        &prepared,
        &binding("Test.Mondays", json!("mondays")),
        &component(),
    )
    .unwrap()
    .remove(0);
    let span = Datum::Record(vec![Datum::Integer(1), Datum::Integer(14)].into());
    assert_eq!(
        binding.call(&[span], None, &mut Vec::new()).unwrap(),
        Datum::List(vec![Datum::Integer(7), Datum::Integer(14)].into())
    );
}
