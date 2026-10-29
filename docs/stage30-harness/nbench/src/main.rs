//! args: cold|warm strict|recover <source>
//! cold: one full parse in this process. warm: 2 warmups, 5 timed parses, each dropped before the next.
use nitride_core::*;
use std::time::Instant;

fn parse(source: &str, recover: bool) -> ParseSnapshot {
    let mode = if recover {
        SnapshotMode::Recovering(RecoveryOptions::default())
    } else {
        SnapshotMode::Strict(ParseOptions::default())
    };
    parse_snapshot_generated(&nitride_cli::csharp4::PARSER, source, Limits::default(), mode)
        .expect("parse")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (warm, recover) = (args[0] == "warm", args[1] == "recover");
    let source = std::fs::read_to_string(&args[2]).unwrap();
    let (warmups, runs) = match (args.get(3), args.get(4)) {
        (Some(w), Some(r)) if warm => (w.parse().unwrap(), r.parse().unwrap()),
        _ if warm => (2, 5),
        _ => (0, 1),
    };
    for _ in 0..warmups {
        drop(parse(&source, recover));
    }
    let mut times = Vec::new();
    let mut retained = 0;
    let mut diagnostics = 0;
    for _ in 0..runs {
        let start = Instant::now();
        let snapshot = parse(&source, recover);
        times.push(start.elapsed().as_secs_f64());
        retained = snapshot.storage_stats().physical_retained_bytes;
        diagnostics = snapshot.diagnostics().len();
        drop(snapshot);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"seconds\":{},\"samples\":{:?},\"retained_bytes\":{},\"diagnostics\":{}}}",
        times[times.len() / 2], times, retained, diagnostics
    );
}
