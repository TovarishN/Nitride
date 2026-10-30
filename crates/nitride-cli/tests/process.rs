//! Interfaces implemented by other processes over JSON-RPC: Calendar's Clock
//! in Python (`languages/calendar/clock.py`), and a Python Gregorian written
//! here. Skipped where python3 is not installed.
use nitride_core::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn calendar() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/calendar")
}

fn python() -> bool {
    let found = std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok();
    if !found {
        eprintln!("python3 not found; skipped");
    }
    found
}

#[test]
#[cfg(feature = "wasm")] // Calendar.nproj also binds Gregorian's WASM component.
fn the_python_clock_runs_when_granted() {
    if !python() {
        return;
    }
    let manifest = calendar().join("Calendar.nproj");
    let source = "(today - today) in days;";
    let denied = nitride_cli::run_project(&manifest, source, &[]).unwrap();
    assert_eq!(
        denied["message"],
        "'Calendar.Today' needs Calendar.Clock, which the host does not grant"
    );
    let granted = nitride_cli::run_project(&manifest, source, &["Calendar.Clock".into()]).unwrap();
    assert_eq!(granted["values"], json!([0.0]), "{granted}");

    let reports = nitride_cli::conform_project(&manifest, &["Calendar.Clock".into()]).unwrap();
    let clock = reports
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["interface"] == "Calendar.Clock")
        .unwrap();
    assert_eq!(
        clock,
        &json!({"interface":"Calendar.Clock","implementation":"python3","examples":1,
            "skipped":[],"failed":[]})
    );
}

/// Part of Calendar.Gregorian in Python: dates with a declared failure, and
/// a record in and a list out.
const GREGORIAN: &str = r#"
import datetime, json, sys
def parse_date(text):
    try:
        return {"result": datetime.date.fromisoformat(text).toordinal() - 1}
    except ValueError:
        return {"error": {"code": 1, "message": "bad date", "data": {"failure": "invalid-date"}}}
def mondays(span):
    first = span["start"] + (7 - span["start"] % 7) % 7
    return {"result": list(range(first, span["end"] + 1, 7))}
for line in sys.stdin:
    request = json.loads(line)
    handler = {"parse-date": parse_date, "mondays": mondays}[request["method"]]
    response = handler(*request["params"])
    response.update(jsonrpc="2.0", id=request["id"])
    print(json.dumps(response), flush=True)
"#;

#[test]
fn values_and_failures_cross_as_json() {
    if !python() {
        return;
    }
    let prepared = nitride_prepare::prepare_project(&calendar().join("Calendar.nproj"))
        .unwrap()
        .data;
    let loaded = nitride_codegen::LoadedLanguage::load(&prepared.to_string()).unwrap();
    let language = loaded.language();
    let gregorian = prepared["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["interface"] == "Calendar.Gregorian")
        .unwrap();
    let mut binding: Value = gregorian.clone();
    binding["target"] = "process".into();
    binding["program"] = "python3".into();
    binding["args"] = json!(["-c", GREGORIAN]);
    binding["exports"] = json!({"Calendar.ParseDate":"parse-date","Calendar.Mondays":"mondays"});
    let mut bindings = host::bindings(language);
    bindings.extend(nitride_process::bindings(language, &prepared, &binding, &calendar()).unwrap());
    let run = |source: &str| {
        let tree = parse(language, source, Limits::default()).unwrap();
        let program = compile(language, &tree, Limits::default()).unwrap();
        execute(&program, &bindings, Limits::default())
    };
    let day = |text: &str| iso_date_day_number(text).unwrap();
    let report = run("mondays(span(2024-01-02, 2024-01-15));").unwrap();
    assert_eq!(
        report.values,
        [Datum::List(
            vec![
                Datum::Integer(day("2024-01-08")),
                Datum::Integer(day("2024-01-15"))
            ]
            .into()
        )]
    );
    let failure = run("2024-02-30;").unwrap_err();
    assert_eq!(
        failure.diagnostic.code.as_deref(),
        Some("Calendar.InvalidDate")
    );
    assert_eq!(failure.diagnostic.message, "not a calendar date");
}
