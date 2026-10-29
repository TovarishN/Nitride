//! Calendar's arithmetic is the `Calendar.Gregorian` interface: it runs only
//! through a binding of the contract the catalog declares, and conformance
//! runs the catalog's examples against an implementation.
use nitride_cli::calendar::{
    FixtureCalendar, LANGUAGE,
    gregorian::{self, Gregorian, GregorianFailure},
};
use nitride_core::*;
use std::sync::Arc;

fn program(source: &str) -> Program<'static> {
    let tree = parse(&LANGUAGE, source, Limits::default()).unwrap();
    // The tree's source outlives the test; the program borrows only the language.
    compile(&LANGUAGE, Box::leak(Box::new(tree)), Limits::default()).unwrap()
}

#[test]
fn runs_through_the_interface_binding() {
    let result = nitride_cli::observe_calendar(
        "let start = 2026-10-05;\nstart + 14 days;\n(2026-12-25 - start) in days;\n",
        Arc::new(FixtureCalendar),
    );
    assert_eq!(result["ok"], true, "{result}");
    let day = iso_date_day_number("2026-10-19").unwrap() as f64;
    assert_eq!(result["values"], serde_json::json!([day, 81.0]));
}

#[test]
fn declared_failures_are_execution_diagnostics_with_their_id() {
    for (source, code, message) in [
        ("2024-02-30;", "Calendar.InvalidDate", "not a calendar date"),
        (
            "0001-01-01 - 1 days;",
            "Calendar.OutOfRange",
            "date out of range",
        ),
    ] {
        let result = nitride_cli::observe_calendar(source, Arc::new(FixtureCalendar));
        assert_eq!(result["ok"], false, "{source}");
        assert_eq!(result["error"], "execution", "{source}");
        assert_eq!(result["code"], code, "{source}");
        assert_eq!(result["message"], message, "{source}");
    }
}

#[test]
fn preflight_requires_a_binding_of_the_declared_contract() {
    let program = program("2024-01-01 + 1 days;");
    let host = nitride_core::host::bindings(&LANGUAGE);
    // Built-in handlers alone leave the interface operations unbound.
    assert!(preflight(&program, &host).is_err());

    let parse_date = *LANGUAGE
        .operations
        .iter()
        .find(|s| s.id == "Calendar.ParseDate")
        .unwrap();
    let later = *LANGUAGE
        .operations
        .iter()
        .find(|s| s.id == "Calendar.Later")
        .unwrap();
    let stale = "Calendar.Gregorian@0000";
    let mut bindings = nitride_core::host::bindings(&LANGUAGE);
    bindings.push(HostBinding::contract(parse_date, stale, |_| Ok(0.0)));
    bindings.push(HostBinding::contract(later, stale, |_| Ok(0.0)));
    let errors = preflight(&program, &bindings).unwrap_err();
    assert!(
        errors[0]
            .message
            .contains("implements contract Calendar.Gregorian@0000, the catalog declares"),
        "{errors:?}"
    );

    // A plain host handler cannot stand in for an interface operation.
    let mut bindings = nitride_core::host::bindings(&LANGUAGE);
    bindings.push(HostBinding::text(parse_date, |_| Ok(0.0)));
    bindings.push(HostBinding::new(later, |a| Ok(a[0] + a[1])));
    assert!(preflight(&program, &bindings).is_err());

    let mut bindings = nitride_core::host::bindings(&LANGUAGE);
    bindings.extend(gregorian::bindings(&LANGUAGE, Arc::new(FixtureCalendar)));
    assert!(preflight(&program, &bindings).is_ok());
}

#[test]
fn the_fixture_conforms() {
    assert_eq!(gregorian::EXAMPLES.len(), 5);
    assert_eq!(
        gregorian::conform(&LANGUAGE, Arc::new(FixtureCalendar)),
        vec![]
    );
}

/// Off by one in `Later`, accepts any date, and never reports `OutOfRange`.
struct Sloppy;

impl Gregorian for Sloppy {
    fn parse_date(&self, text: &str) -> Result<f64, GregorianFailure> {
        Ok(iso_date_day_number(text).unwrap_or(0) as f64)
    }
    fn later(&self, date: f64, duration: f64) -> Result<f64, GregorianFailure> {
        Ok(date + duration + 1.0)
    }
    fn earlier(&self, date: f64, duration: f64) -> Result<f64, GregorianFailure> {
        Ok(date - duration)
    }
    fn between(&self, _: f64, _: f64) -> Result<f64, GregorianFailure> {
        // Not a failure `Between` declares.
        Err(GregorianFailure::OutOfRange)
    }
}

#[test]
fn conformance_names_each_example_an_implementation_fails() {
    let failed = gregorian::conform(&LANGUAGE, Arc::new(Sloppy));
    let examples: Vec<&str> = failed.iter().map(|(example, _)| *example).collect();
    assert_eq!(
        examples,
        [
            "example Later(ParseDate(\"2024-01-01\"), Days(3)) == ParseDate(\"2024-01-04\");",
            "example Between(ParseDate(\"2024-03-01\"), ParseDate(\"2024-02-01\")) == Days(29);",
            "example ParseDate(\"2024-02-30\") fails InvalidDate;",
            "example Earlier(ParseDate(\"0001-01-01\"), Days(1)) fails OutOfRange;",
        ]
    );
    let day = |text| iso_date_day_number(text).unwrap() as f64;
    assert_eq!(
        failed[0].1,
        format!("got {}, expected {}", day("2024-01-05"), day("2024-01-04"))
    );
    assert_eq!(
        failed[1].1,
        "failed with host.undeclared (undeclared failure Calendar.OutOfRange)"
    );
    assert_eq!(failed[2].1, "got 0, expected Calendar.InvalidDate");
}

#[test]
fn interface_rust_is_generated_from_the_catalog() {
    let prepared = nitride_prepare::prepare_project(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../languages/calendar/project.json"),
    )
    .unwrap();
    let rust = nitride_codegen::interfaces(&prepared.data.to_string()).unwrap();
    assert!(rust.contains(&format!(
        "pub const CONTRACT: &str = {:?};",
        gregorian::CONTRACT
    )));
    assert!(
        rust.contains(
            "fn between(&self, date_1: f64, date_2: f64) -> Result<f64, GregorianFailure>;"
        )
    );
    // A catalog without interfaces generates nothing.
    let dates = nitride_prepare::prepare_project(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../languages/dates/project.json"),
    )
    .unwrap();
    assert_eq!(
        nitride_codegen::interfaces(&dates.data.to_string()).unwrap(),
        ""
    );
}
