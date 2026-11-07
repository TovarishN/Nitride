//! Run the deterministic Calendar workflow with Rust host bindings.
use nitride_cli::calendar::{FixtureCalendar, LANGUAGE, gregorian};
use nitride_core::*;
use std::sync::Arc;

fn main() {
    let source = include_str!("../../../languages/calendar/planning.calendar");
    let tree = parse(&LANGUAGE, source, Limits::default()).expect("parse Calendar");
    let program = compile(&LANGUAGE, &tree, Limits::default()).expect("check Calendar");
    let mut bindings = host::bindings(&LANGUAGE);
    bindings.extend(gregorian::bindings(&LANGUAGE, Arc::new(FixtureCalendar)));
    let report = execute(&program, &bindings, Limits::default()).expect("run Calendar");
    println!("{}", serde_json::json!(report.values.iter().map(nitride_cli::datum).collect::<Vec<_>>()));
}
