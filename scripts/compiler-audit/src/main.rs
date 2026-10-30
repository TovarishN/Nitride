use nitride_core::*;
use std::time::Instant;

mod csharp4 {
    include!(concat!(env!("OUT_DIR"), "/csharp4.rs"));
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() == 2,
        "usage: nitride-compiler-audit strict|recover <source>"
    );
    let mode = match args[0].as_str() {
        "strict" => SnapshotMode::Strict(ParseOptions::default()),
        "recover" => SnapshotMode::Recovering(RecoveryOptions::default()),
        _ => panic!("expected strict or recover"),
    };
    let source = std::fs::read_to_string(&args[1]).unwrap();
    let start = Instant::now();
    let snapshot = parse_snapshot_generated(&csharp4::PARSER, &source, Limits::default(), mode)
        .expect("parse snapshot");
    let elapsed_ns = start.elapsed().as_nanos();
    // Export and fingerprint outside the timed region. Include every node,
    // labeled edge, diagnostic and recovery record, not just node counts.
    let tree = snapshot.materialize().unwrap();
    let mut hash = 0xcbf29ce484222325u64;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            hash = (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3);
        }
    };
    feed(&tree.root.to_le_bytes());
    for (id, node) in tree.nodes.iter().enumerate() {
        feed(&node.rule.to_le_bytes());
        feed(&node.span[0].to_le_bytes());
        feed(&node.span[1].to_le_bytes());
        feed(&node.child_count.to_le_bytes());
        for edge in tree.children(id as u32) {
            feed(&edge.node.to_le_bytes());
            feed(tree.label(edge).as_bytes());
            feed(&[0xff]);
        }
    }
    feed(format!("{:?}", snapshot.diagnostics()).as_bytes());
    feed(format!("{:?}", tree.recovery()).as_bytes());
    println!(
        "{}",
        serde_json::json!({
            "elapsed_ns": elapsed_ns, "fingerprint": format!("{hash:016x}"),
            "source_bytes": source.len(), "nodes": tree.nodes.len(),
            "diagnostics": snapshot.diagnostic_count(), "work": snapshot.stats().work,
            "native_lexical_calls": snapshot.stats().native_lexical_calls,
            "native_syntax_calls": snapshot.stats().native_syntax_calls,
            "fallbacks": snapshot.stats().full_parse_fallbacks,
        })
    );
}
