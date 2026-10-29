//! Calendar's arithmetic is the `Calendar.Gregorian` interface: it runs only
//! through a binding of the contract the catalog declares, and conformance
//! runs the catalog's examples against an implementation. Its dates are
//! integers, weekdays text and leap years booleans.
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

fn day(text: &str) -> i64 {
    iso_date_day_number(text).unwrap()
}

#[test]
fn runs_through_the_interface_binding() {
    let result = nitride_cli::observe_calendar(
        include_str!("../../../languages/calendar/sample.calendar"),
        Arc::new(FixtureCalendar),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(
        result["values"],
        serde_json::json!([day("2026-10-19"), 81.0, "Monday", true])
    );
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
        (
            "2024-01-01 + 0.5 days;",
            "Calendar.PartialDay",
            "not a whole number of days",
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
    bindings.push(HostBinding::contract(parse_date, stale, |_| {
        Ok(Datum::Integer(0))
    }));
    bindings.push(HostBinding::contract(later, stale, |_| {
        Ok(Datum::Integer(0))
    }));
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
    assert_eq!(gregorian::EXAMPLES.len(), 10);
    assert_eq!(
        gregorian::conform(&LANGUAGE, Arc::new(FixtureCalendar)),
        vec![]
    );
}

/// Off by one in `Later`, accepts any date and part days, never reports
/// `OutOfRange`, lowercases weekdays and calls every year leap.
struct Sloppy;

impl Gregorian for Sloppy {
    fn parse_date(&self, text: &str) -> Result<i64, GregorianFailure> {
        Ok(iso_date_day_number(text).unwrap_or(0))
    }
    fn later(&self, date: i64, duration: f64) -> Result<i64, GregorianFailure> {
        Ok(date + duration as i64 + 1)
    }
    fn earlier(&self, date: i64, duration: f64) -> Result<i64, GregorianFailure> {
        Ok(date - duration as i64)
    }
    fn between(&self, _: i64, _: i64) -> Result<f64, GregorianFailure> {
        // Not a failure `Between` declares.
        Err(GregorianFailure::OutOfRange)
    }
    fn day_of_week(&self, _: i64) -> Result<String, GregorianFailure> {
        Ok("monday".into())
    }
    fn is_leap_year(&self, _: i64) -> Result<bool, GregorianFailure> {
        Ok(true)
    }
}

#[test]
fn conformance_names_each_example_an_implementation_fails() {
    let failed = gregorian::conform(&LANGUAGE, Arc::new(Sloppy));
    let why = |example: &str| {
        failed
            .iter()
            .find(|(source, _)| source.contains(example))
            .map(|(_, why)| why.as_str())
    };
    assert_eq!(failed.len(), 8, "{failed:?}");
    assert_eq!(
        why("Later(ParseDate(\"2024-01-01\"), Days(3))"),
        Some(format!("got {}, expected {}", day("2024-01-05"), day("2024-01-04")).as_str())
    );
    assert_eq!(
        why("Between("),
        Some("failed with host.undeclared (undeclared failure Calendar.OutOfRange)")
    );
    assert_eq!(
        why("ParseDate(\"2024-02-30\")"),
        Some("got 0, expected Calendar.InvalidDate")
    );
    assert_eq!(
        why("2026-10-29"),
        Some("got \"monday\", expected \"Thursday\"")
    );
    assert_eq!(why("1900-03-01"), Some("got true, expected false"));
    // Correct answers pass, whatever else is wrong.
    assert_eq!(why("Earlier(ParseDate(\"2025-01-01\")"), None);
    assert_eq!(why("2024-03-01\")) == true"), None);
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
    for signature in [
        "fn parse_date(&self, text: &str) -> Result<i64, GregorianFailure>;",
        "fn between(&self, date_1: i64, date_2: i64) -> Result<f64, GregorianFailure>;",
        "fn day_of_week(&self, date: i64) -> Result<String, GregorianFailure>;",
        "fn is_leap_year(&self, date: i64) -> Result<bool, GregorianFailure>;",
    ] {
        assert!(rust.contains(signature), "{signature}");
    }
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
