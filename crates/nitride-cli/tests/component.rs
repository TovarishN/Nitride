//! Calendar's Gregorian interface run by a WebAssembly component
//! (`languages/calendar/gregorian.wasm`, built by `component/build.sh` from a
//! crate that knows only its WIT), as `gregorian.nbind` binds it.
use nitride_core::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn calendar() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/calendar")
}

fn manifest() -> PathBuf {
    calendar().join("Calendar.nproj")
}

#[test]
fn the_component_runs_programs_as_the_rust_implementation_does() {
    let sample = std::fs::read_to_string(calendar().join("sample.calendar")).unwrap();
    let component = nitride_cli::run_project(&manifest(), &sample, &[]).unwrap();
    let native = nitride_cli::observe_calendar(
        &sample,
        Arc::new(nitride_cli::calendar::FixtureCalendar),
        &[],
    );
    assert_eq!(component["ok"], true, "{component}");
    assert_eq!(component["values"], native["values"]);
    assert_eq!(component["trace"], native["trace"]);

    for (source, code) in [
        ("2024-02-30;", "Calendar.InvalidDate"),
        ("0001-01-01 - 1 days;", "Calendar.OutOfRange"),
        ("2024-01-01 + 0.5 days;", "Calendar.PartialDay"),
    ] {
        let result = nitride_cli::run_project(&manifest(), source, &[]).unwrap();
        assert_eq!(result["ok"], false, "{source}");
        assert_eq!(result["code"], code, "{source}");
    }
}

#[test]
fn the_component_conforms() {
    let reports = nitride_cli::conform_project(&manifest(), &[]).unwrap();
    assert_eq!(
        reports,
        serde_json::json!([
            {
                "interface": "Calendar.Gregorian",
                "implementation": "gregorian.wasm",
                "examples": 13,
                "skipped": [],
                "failed": [],
            },
            {
                // Bound to a process; its one example needs the Clock effect.
                "interface": "Calendar.Clock",
                "implementation": "python3",
                "examples": 1,
                "skipped": ["example Today() == Today();"],
                "failed": [],
            }
        ])
    );
}

#[test]
fn bindings_are_drafted_from_what_a_component_exports() {
    let prepared = prepared();
    let component = calendar().join("gregorian.wasm");
    // ParseDate is exported as parse-iso, the one export of its shape.
    let draft = nitride_wasm::draft(
        &prepared,
        "Calendar.Gregorian",
        &component,
        "gregorian.wasm",
    )
    .unwrap();
    assert!(
        draft.ends_with(
            "binding Calendar.Gregorian version 3 for wasm component \"gregorian.wasm\"\n{\n  ParseDate = parse-iso;\n}\n"
        ),
        "{draft}"
    );
    // The draft is a binding that checks against the catalog.
    let checked = nitride_prepare::bindings(
        &[nitride_prepare::Source {
            path: "drafted.nbind".into(),
            text: draft,
        }],
        &prepared["catalog"],
    )
    .unwrap();
    assert_eq!(checked[0]["exports"], prepared["bindings"][0]["exports"]);

    // Against another interface, the component's one instance is named, and
    // what has no export of its shape is left to resolve.
    let draft = nitride_wasm::draft(&prepared, "Calendar.Clock", &component, "x.wasm").unwrap();
    assert!(
        draft.contains(
            "export \"nitride:calendar/gregorian@3.0.0\"\n{\n  // Today: no export of its shape\n}"
        ),
        "{draft}"
    );
    assert_eq!(
        nitride_wasm::draft(&prepared, "Calendar.Week", &component, "x.wasm").unwrap_err(),
        "unknown interface 'Calendar.Week'"
    );
}

fn prepared() -> Value {
    nitride_prepare::prepare_project(&manifest()).unwrap().data
}

#[test]
fn loading_checks_exports_and_preflight_checks_the_contract() {
    let prepared = prepared();
    let language = nitride_codegen::LoadedLanguage::load(&prepared.to_string()).unwrap();
    let language = language.language();
    let component = calendar().join("gregorian.wasm");

    let mut renamed = prepared["bindings"][0].clone();
    renamed["exports"]["Calendar.ParseDate"] = "parse-date".into();
    let error = nitride_wasm::bindings(language, &prepared, &renamed, &component)
        .err()
        .unwrap();
    assert!(
        error.ends_with("does not export 'parse-date' of nitride:calendar/gregorian@3.0.0"),
        "{error}"
    );

    let mut wrong = prepared["bindings"][0].clone();
    wrong["exports"]["Calendar.Between"] = "mondays".into();
    let error = nitride_wasm::bindings(language, &prepared, &wrong, &component)
        .err()
        .unwrap();
    assert!(
        error.contains("'mondays' of nitride:calendar/gregorian@3.0.0 takes 1 inputs"),
        "{error}"
    );

    // A component bound under another contract does not pass preflight.
    let mut stale = prepared["bindings"][0].clone();
    stale["contract"] = "Calendar.Gregorian@0000".into();
    let mut bindings = host::bindings(language);
    bindings.extend(nitride_wasm::bindings(language, &prepared, &stale, &component).unwrap());
    let tree = parse(language, "2024-01-01 + 1 days;", Limits::default()).unwrap();
    let program = compile(language, &tree, Limits::default()).unwrap();
    let errors = preflight(&program, &bindings).unwrap_err();
    assert!(
        errors[0]
            .message
            .contains("implements contract Calendar.Gregorian@0000"),
        "{errors:?}"
    );
}

#[test]
fn the_component_wit_is_current() {
    let generated = nitride_codegen::binding_wit(&prepared().to_string()).unwrap();
    assert_eq!(generated.len(), 1);
    assert_eq!(generated[0].0, "gregorian.wasm");
    let checked_in =
        std::fs::read_to_string(calendar().join("component/wit/gregorian.wit")).unwrap();
    assert_eq!(
        generated[0].1, checked_in,
        "regenerate with `nitride interface --target wit-binding languages/calendar/Calendar.nproj`, then rebuild the component"
    );
}
