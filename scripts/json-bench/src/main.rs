use nitride_core::*;
use std::time::Instant;

mod json {
    include!(concat!(env!("OUT_DIR"), "/json.rs"));
}

fn parse(source: &str) -> Result<ParseSnapshot, Diagnostic> {
    parse_snapshot_generated(
        &json::PARSER,
        source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // One line per file: whether the whole file is accepted.
        Some("check") => {
            for path in &args[1..] {
                let bytes = std::fs::read(path).unwrap();
                let verdict = match std::str::from_utf8(&bytes) {
                    Err(_) => "reject-utf8".to_string(),
                    Ok(source) => match parse(source) {
                        Ok(s) if s.diagnostic_count() == 0 => "accept".to_string(),
                        Ok(_) => "reject".to_string(),
                        Err(d) => format!("reject-{}", d.category),
                    },
                };
                println!("{verdict}\t{path}");
            }
        }
        // First parse in this process, then `warm` repeated parses.
        Some("bench") if args.len() == 3 => {
            let source = std::fs::read_to_string(&args[1]).unwrap();
            let warm: usize = args[2].parse().unwrap();
            let mut samples = Vec::with_capacity(warm + 1);
            let mut nodes = 0;
            for _ in 0..=warm {
                let start = Instant::now();
                let snapshot = parse(&source).expect("parse");
                samples.push(start.elapsed().as_nanos() as u64);
                assert_eq!(snapshot.diagnostic_count(), 0);
                nodes = snapshot.materialize().unwrap().nodes.len();
            }
            let cold = samples[0];
            let mut rest = samples[1..].to_vec();
            rest.sort_unstable();
            println!(
                "{}",
                serde_json::json!({
                    "engine": "nitride", "source_bytes": source.len(), "nodes": nodes,
                    "cold_ns": cold, "warm_median_ns": rest.get(rest.len() / 2),
                    "warm_min_ns": rest.first(),
                })
            );
        }
        _ => panic!("usage: nitride-json-bench check <file>... | bench <file> <warm-iterations>"),
    }
}
