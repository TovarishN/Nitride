use nitride_cli::calendar::FixtureCalendar;
use nitride_core::*;
use serde_json::json;
use std::sync::Arc;

const WORKFLOW: &str = include_str!("../../../languages/calendar/planning.calendar");
fn expected() -> serde_json::Value {
    json!([iso_date_day_number("2024-03-13").unwrap(), "Wednesday", true,
           14.0, 42.0, -14.0, 21.0, 42.0, iso_date_day_number("2024-01-08").unwrap(),
           2.0, iso_date_day_number("2024-03-11").unwrap()])
}

#[test]
fn duration_rewrites_and_planning_workflow_execute_through_rust() {
    let report = nitride_cli::observe_calendar(WORKFLOW, Arc::new(FixtureCalendar), &[]);
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["values"], expected());
}

#[test]
fn invalid_types_and_denied_effects_never_invoke_a_handler() {
    for source in ["2024-01-01 * 2;", "1 + 2; 2024-01-01 + 2024-01-02;", "today + 1 weeks;"] {
        let report = nitride_cli::observe_calendar(source, Arc::new(FixtureCalendar), &[]);
        assert_eq!(report["ok"], false, "{source}: {report}");
        assert_eq!(report["trace"], json!([]), "{source}: {report}");
    }
}

#[test]
fn execution_failures_keep_their_categories_and_declared_codes() {
    for (source, code) in [
        ("2024-01-01 + (1 weeks / 2);", Some("Calendar.PartialDay")),
        ("0001-01-01 + (-1 days);", Some("Calendar.OutOfRange")),
        ("2024-02-30 + 1 weeks;", Some("Calendar.InvalidDate")),
        ("1 weeks / 0;", None),
        ("nth(mondays(span(2024-01-02, 2024-01-03)), 0);", None),
    ] {
        let report = nitride_cli::observe_calendar(source, Arc::new(FixtureCalendar), &[]);
        assert_eq!(report["ok"], false, "{source}: {report}");
        assert_eq!(report["error"], "execution", "{source}: {report}");
        if let Some(code) = code { assert_eq!(report["code"], code, "{source}: {report}"); }
    }
}

#[test]
fn deterministic_workflow_runs_with_process_only_bindings_and_conforms() {
    if std::process::Command::new("python3").arg("--version").output().is_err() { return; }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/calendar/process/CalendarProcess.nproj");
    let report = nitride_cli::run_project(&manifest, WORKFLOW, &[]).unwrap();
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["values"], expected());
    let reports = nitride_cli::conform_project(&manifest, &[]).unwrap();
    for report in reports.as_array().unwrap() {
        assert_eq!(report["failed"], json!([]), "{report}");
    }
}

#[cfg(feature = "components")]
#[test]
fn unchanged_component_contract_runs_the_extended_grammar() {
    if std::process::Command::new("python3").arg("--version").output().is_err() { return; }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/calendar/Calendar.nproj");
    let report = nitride_cli::run_project(&manifest, WORKFLOW, &[]).unwrap();
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["values"], expected());
}

#[test]
fn nested_duration_rewrites_keep_the_original_source_origin() {
    let source = "3 fortnights in days;";
    let language = &nitride_cli::calendar::LANGUAGE;
    let tree = parse(language, source, Limits::default()).unwrap();
    let program = compile(language, &tree, Limits::default()).unwrap();
    let rewritten = (0..program.nodes.len() as u32).filter(|&id| program.origin_in_rewrite(id).iter().any(|&inside| inside)).collect::<Vec<_>>();
    assert!(!rewritten.is_empty());
    assert!(rewritten.iter().any(|&id| program.origins(id).first() == Some(&[0,12])));
}
