//! The compiled `languages/context` parser agrees with the interpreter, across
//! strict, recovering and edited snapshots, without stale context.
use nitride_cli::context::{LANGUAGE, PARSER};
use nitride_core::*;

/// Rule name and span of every node, in node order.
fn shape(tree: &SyntaxTree<'_>) -> Vec<(&'static str, [u32; 2])> {
    tree.nodes
        .iter()
        .map(|n| (LANGUAGE.rules[n.rule as usize].name, n.span))
        .collect()
}

fn generated(source: &str, mode: SnapshotMode) -> SyntaxTree<'static> {
    parse_snapshot_generated(&PARSER, source, Limits::default(), mode)
        .unwrap()
        .materialize()
        .unwrap()
}

const STRICT: SnapshotMode = SnapshotMode::Strict(ParseOptions {
    strict_comments: false,
    token_children: false,
    alias_nodes: false,
});

#[test]
fn generated_parses_match_the_interpreter() {
    let sample = include_str!("../../../languages/context/sample.context");
    let flat = parse(&LANGUAGE, sample, Limits::default()).unwrap();
    assert_eq!(shape(&generated(sample, STRICT)), shape(&flat));
    let selected = |name: &str| shape(&flat).iter().filter(|(n, _)| *n == name).count();
    assert_eq!(selected("Context.Enabled"), 7);
    assert_eq!(selected("Context.Disabled"), 4);
    assert!(LANGUAGE.rules.iter().any(|r| r.origin.is_some()));
    assert!(
        flat.nodes
            .iter()
            .all(|n| LANGUAGE.rules[n.rule as usize].origin.is_none())
    );
}

#[test]
fn generated_edits_flip_a_header_without_stale_context() {
    for (from, to) in [
        ("a x n x a (n x)", "n x n x a (n x)"),
        ("n x n x a (n x)", "n x n x n (n x)"),
        ("n x + x + x", "a x + x + x"),
        ("a (r [x])", "n (r [x])"),
    ] {
        let at = from
            .bytes()
            .zip(to.bytes())
            .position(|(a, b)| a != b)
            .unwrap() as u32;
        let base = parse_snapshot_generated(&PARSER, from, Limits::default(), STRICT).unwrap();
        let edited = base
            .edit(
                &TextEdit {
                    range: [at, at + 1],
                    replacement: to[at as usize..at as usize + 1].into(),
                },
                Limits::default(),
            )
            .unwrap();
        let full = parse(&LANGUAGE, to, Limits::default()).unwrap();
        assert_eq!(shape(&edited.materialize().unwrap()), shape(&full), "{to}");
    }
}

#[test]
fn generated_recovery_keeps_each_selection_inside_its_header() {
    let source = "a x n ? n x a (x";
    let tree = generated(source, SnapshotMode::Recovering(RecoveryOptions::default()));
    let nodes = shape(&tree);
    let inside = |inner: &str, outer: &str| {
        nodes.iter().filter(|(n, _)| *n == inner).all(|(_, s)| {
            nodes
                .iter()
                .any(|(n, o)| *n == outer && o[0] <= s[0] && s[1] <= o[1])
        })
    };
    assert!(inside("Context.Enabled", "Context.Async"));
    assert!(inside("Context.Disabled", "Context.Ordinary"));
    let flat = parse_recovering(
        &LANGUAGE,
        source,
        Limits::default(),
        RecoveryOptions::default(),
    );
    assert!(!flat.diagnostics.is_empty());
    assert_eq!(shape(&flat.tree), nodes);
}
