use nitride_core::*;
use std::time::Instant;

mod json {
    include!(concat!(env!("OUT_DIR"), "/json.rs"));
}

// Counts live and peak heap bytes for the `heap` mode. Off in timing builds.
#[cfg(feature = "heap")]
mod heap {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    pub static LIVE: AtomicUsize = AtomicUsize::new(0);
    pub static PEAK: AtomicUsize = AtomicUsize::new(0);
    struct Counting;
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let live = LIVE.fetch_add(layout.size(), Relaxed) + layout.size();
            PEAK.fetch_max(live, Relaxed);
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            LIVE.fetch_sub(layout.size(), Relaxed);
            unsafe { System.dealloc(ptr, layout) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: Counting = Counting;
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
        // Parse counters and retained storage of one parse.
        Some("stats") => {
            let source = std::fs::read_to_string(&args[1]).unwrap();
            let snapshot = parse(&source).expect("parse");
            eprintln!("{:#?}\n{:#?}", snapshot.stats(), snapshot.storage_stats());
        }
        // Peak heap during one parse and the heap the snapshot keeps.
        #[cfg(feature = "heap")]
        Some("heap") => {
            use std::sync::atomic::Ordering::Relaxed;
            let source = std::fs::read_to_string(&args[1]).unwrap();
            let before = heap::LIVE.load(Relaxed);
            heap::PEAK.store(before, Relaxed);
            let snapshot = parse(&source).expect("parse");
            println!(
                "{}",
                serde_json::json!({
                    "source_bytes": source.len(),
                    "peak_bytes": heap::PEAK.load(Relaxed) - before,
                    "live_after_bytes": heap::LIVE.load(Relaxed) - before,
                })
            );
            drop(snapshot);
        }
        // Successive one-digit edits, each on the previous snapshot.
        Some("edit") if args.len() == 3 => {
            let mut source = std::fs::read_to_string(&args[1]).unwrap();
            let count: usize = args[2].parse().unwrap();
            let mut snapshot = parse(&source).expect("parse");
            let mut samples = Vec::with_capacity(count);
            for i in 0..count {
                let from = i * source.len() / count;
                let at = from + source[from..].find(|c: char| c.is_ascii_digit()).unwrap();
                let digit = if &source[at..=at] == "1" { "2" } else { "1" };
                let edit = TextEdit {
                    range: [at as u32, at as u32 + 1],
                    replacement: digit.into(),
                };
                let start = Instant::now();
                snapshot = snapshot.edit(&edit, Limits::default()).expect("edit");
                samples.push(start.elapsed().as_nanos() as u64);
                assert_eq!(snapshot.diagnostic_count(), 0);
                source.replace_range(at..=at, digit);
            }
            let first = samples[0];
            samples.sort_unstable();
            println!(
                "{}",
                serde_json::json!({
                    "edits": count, "first_edit_ns": first,
                    "median_edit_ns": samples[samples.len() / 2],
                })
            );
        }
        // `warm` parses of one file, then the first and later parses of another:
        // whether warming up on one input helps a different one.
        Some("cross") if args.len() == 4 => {
            let warmup = std::fs::read_to_string(&args[1]).unwrap();
            let target = std::fs::read_to_string(&args[3]).unwrap();
            let warm: usize = args[2].parse().unwrap();
            for _ in 0..warm {
                std::hint::black_box(parse(&warmup).expect("parse"));
            }
            let mut samples = Vec::with_capacity(11);
            for _ in 0..11 {
                let start = Instant::now();
                let snapshot = parse(&target).expect("parse");
                samples.push(start.elapsed().as_nanos() as u64);
                assert_eq!(snapshot.diagnostic_count(), 0);
            }
            let first = samples[0];
            samples[1..].sort_unstable();
            println!(
                "{}",
                serde_json::json!({ "first_ns": first, "repeat_median_ns": samples[6] })
            );
        }
        // Repeated parses, for profilers.
        Some("loop") => {
            let source = std::fs::read_to_string(&args[1]).unwrap();
            for _ in 0..args[2].parse::<usize>().unwrap() {
                std::hint::black_box(parse(&source).expect("parse"));
            }
        }
        _ => panic!("usage: nitride-json-bench check <file>... | bench <file> <warm-iterations>"),
    }
}
