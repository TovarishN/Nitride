//! Whole-document editor analysis versus a cached snapshot update.
use nitride_lsp::{
    analysis::{self, Analysis},
    registry::Registry,
};
use serde_json::json;
use std::{path::PathBuf, time::Instant};

fn shape(analysis: &Analysis) -> Vec<(u16, [u32; 2], u32, u32)> {
    analysis
        .tree
        .as_ref()
        .map(|tree| {
            tree.nodes
                .iter()
                .map(|n| (n.rule, n.span, n.first_child, n.child_count))
                .collect()
        })
        .unwrap_or_default()
}
fn main() {
    if std::env::var_os("NITRIDE_BENCH_CANCELLATION").is_some() {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let token = nitride_core::CancellationToken::from_flag(flag.clone());
        std::hint::black_box(&flag);
        nitride_core::with_cancellation(&token, run);
        std::hint::black_box(flag);
    } else {
        run();
    }
}
fn run() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "usage: benchmark WORKSPACE SOURCE");
    let workspace = PathBuf::from(&args[0]);
    let path = PathBuf::from(&args[1]);
    let mut source = std::fs::read_to_string(&path).unwrap();
    let mut registry = Registry::default();
    let started = Instant::now();
    registry.reload(&[workspace], &|_| None);
    let load_ns = started.elapsed().as_nanos();
    let started = Instant::now();
    let mut previous =
        analysis::analyze(&registry, &path, &source, &|_| None).expect("served language");
    let initial_ns = started.elapsed().as_nanos();
    let at = source
        .as_bytes()
        .iter()
        .enumerate()
        .skip(source.len() / 2)
        .find(|(_, b)| b.is_ascii_digit())
        .map(|(i, _)| i)
        .expect("digit to edit");
    let original = source.as_bytes()[at];
    let changed = if original == b'1' { b'2' } else { b'1' };
    let mut samples = Vec::new();
    for i in 0..6 {
        source.replace_range(
            at..at + 1,
            std::str::from_utf8(&[if i % 2 == 0 { changed } else { original }]).unwrap(),
        );
        let started = Instant::now();
        let next =
            analysis::analyze_incremental(&registry, &path, &source, &|_| None, Some(&previous))
                .unwrap();
        let incremental_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let fresh = analysis::analyze(&registry, &path, &source, &|_| None).unwrap();
        let fresh_ns = started.elapsed().as_nanos();
        assert_eq!(next.problems, fresh.problems);
        assert_eq!(next.values, fresh.values);
        assert_eq!(shape(&next), shape(&fresh));
        assert_eq!(
            next.tree.as_ref().unwrap().edges,
            fresh.tree.as_ref().unwrap().edges
        );
        let stats = next.snapshot.as_ref().unwrap().stats();
        if i > 0 {
            samples.push(json!({"incremental_ns": incremental_ns, "fresh_ns": fresh_ns,
                "parse_update_ns": next.timings.parse_ns, "materialize_ns": next.timings.materialize_ns,
                "syntax_projection_ns": next.timings.syntax_projection_ns,
                "semantic_ns": next.timings.semantic_ns, "editor_ns": next.timings.editor_ns,
                "reused_subtrees": stats.reused_subtrees, "work": stats.work}));
        }
        previous = next;
    }
    // Check the final editor projection against the established full export,
    // after all timed iterations so this oracle cannot warm later samples.
    previous
        .snapshot
        .as_ref()
        .unwrap()
        .with_tree(|flat| {
            let flat_shape: Vec<_> = flat
                .nodes
                .iter()
                .map(|n| (n.rule, n.span, n.first_child, n.child_count))
                .collect();
            assert_eq!(shape(&previous), flat_shape);
            assert_eq!(previous.tree.as_ref().unwrap().root, flat.root);
            assert_eq!(
                previous.tree.as_ref().unwrap().edges,
                flat.edges.iter().map(|e| e.node).collect::<Vec<_>>()
            );
        })
        .unwrap();
    println!(
        "{}",
        json!({"source_bytes": source.len(), "registry_load_ns": load_ns,
        "initial_analysis_ns": initial_ns, "samples": samples,
        "metric": "in-process analyze API through ready diagnostics/hints/navigation; registry loading separate; parse update includes source-difference preparation; one untimed edit then five samples; fresh/incremental outputs compared outside timings; final editor tree checked against full snapshot export after all timed iterations; syntax_projection_ns measures direct editor projection, separate from full materialize_ns"})
    );
}
