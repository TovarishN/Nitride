use nitride_core::*;
fn compare(language: &'static Language, source: &str) {
    let old = parse(language, source, Limits::default()).unwrap();
    let shared = parse_snapshot(
        language,
        source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let exported = shared.materialize().unwrap();
    assert_eq!(old.source, exported.source);
    assert_eq!(old.nodes.len(), exported.nodes.len());
    for (id, (a, b)) in old.nodes.iter().zip(&exported.nodes).enumerate() {
        assert_eq!((a.rule, a.span), (b.rule, b.span), "node {id}");
        let children = |tree: &SyntaxTree<'static>| {
            tree.children(id as u32)
                .iter()
                .map(|e| (tree.label(e), e.node))
                .collect::<Vec<_>>()
        };
        assert_eq!(children(&old), children(&exported));
    }
}
#[test]
fn shared_parsing_preserves_compact_trees() {
    compare(&nitride_cli::LANGUAGE, "1 + 2 * 3;\n");
    compare(&nitride_cli::power::LANGUAGE, "let x : Scalar = 2; x ^ 3;");
    compare(
        &nitride_cli::xml::LANGUAGE,
        "  <root><item key=\"a\">value</item><item/></root>  ",
    );
    compare(
        &nitride_cli::csharp4::LANGUAGE,
        "class C { int F() { return (((1+2)*(3+4))); } }",
    );
}
#[test]
fn memo_hit_reuses_root_without_descendant_copy() {
    let snapshot = parse_snapshot(
        &nitride_cli::csharp4::LANGUAGE,
        "class C { int F() { return (((1+2)*(3+4))); } }",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    assert!(snapshot.stats().memo_hits > 0);
    assert_eq!(snapshot.stats().memo_descendants_copied, 0);
    assert_eq!(snapshot.stats().winner_descendants_copied, 0);
}
#[test]
fn cross_piece_token_and_comment_preserve_spans() {
    let source = format!("{}<root/>/* comment */", " ".repeat(64 * 1024 - 2));
    compare(&nitride_cli::xml::LANGUAGE, &source);
}

#[test]
fn shared_parser_matches_entire_frozen_csharp_corpus() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/csharp4-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let old = parse(&nitride_cli::csharp4::LANGUAGE, source, Limits::default());
        let shared = parse_snapshot(
            &nitride_cli::csharp4::LANGUAGE,
            source,
            Limits::default(),
            SnapshotMode::Strict(ParseOptions::default()),
        );
        match (old, shared) {
            (Ok(_), Ok(_)) => compare(&nitride_cli::csharp4::LANGUAGE, source),
            (Err(old), Err(shared)) => assert_eq!(old, shared, "{}", case["name"]),
            _ => panic!("acceptance mismatch: {}", case["name"]),
        }
    }
}
#[test]
fn shared_parser_preserves_all_parse_modes() {
    let language = &nitride_cli::csharp4::LANGUAGE;
    let source = "class Abcdef { int ghijkl; }";
    for bits in 0..8 {
        let options = ParseOptions {
            strict_comments: bits & 1 != 0,
            token_children: bits & 2 != 0,
            alias_nodes: bits & 4 != 0,
        };
        let old = parse_with_options(language, source, Limits::default(), options).unwrap();
        let snapshot = parse_snapshot(
            language,
            source,
            Limits::default(),
            SnapshotMode::Strict(options),
        )
        .unwrap();
        let flat = snapshot.materialize().unwrap();
        let normalize = |tree: SyntaxTree| {
            tree.nodes
                .into_iter()
                .map(|n| (n.rule, n.span, n.child_count))
                .collect::<Vec<_>>()
        };
        assert_eq!(normalize(old), normalize(flat), "mode {bits}");
    }
}

#[test]
fn warm_piece_reader_matches_cold_contiguous_reader() {
    let language = &nitride_cli::xml::LANGUAGE;
    let source = format!("{}<root/>/* comment */", " ".repeat(64 * 1024 - 2));
    let old = parse_snapshot(
        language,
        &source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let warm = old
        .edit(
            &TextEdit {
                range: [0, 1],
                replacement: " ".into(),
            },
            Limits::default(),
        )
        .unwrap()
        .materialize()
        .unwrap();
    let cold = parse(language, &source, Limits::default()).unwrap();
    assert_eq!(
        warm.nodes
            .iter()
            .map(|n| (n.rule, n.span))
            .collect::<Vec<_>>(),
        cold.nodes
            .iter()
            .map(|n| (n.rule, n.span))
            .collect::<Vec<_>>()
    );
}

#[test]
fn physical_storage_counts_shared_readers_once() {
    let snapshot = parse_snapshot(
        &nitride_cli::xml::LANGUAGE,
        "<root><item/></root>",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let cloned = snapshot.clone();
    let single = snapshot.storage_stats().physical_retained_bytes;
    assert!(single > snapshot.storage_stats().syntax_payload_bytes);
    assert_eq!(shared_storage_bytes(&[&snapshot, &cloned]), single);
}

#[test]
fn short_unchanged_subtrees_are_reused_across_edits() {
    let language = &nitride_cli::xml::LANGUAGE;
    let old = parse_snapshot(
        language,
        "<r><a/><b/></r>",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let find = |snapshot: &ParseSnapshot| {
        let mut work = vec![snapshot.root()];
        while let Some(n) = work.pop() {
            if n.text() == "<b/>" {
                return n.subtree_id();
            }
            work.extend(n.children().map(|(_, c)| c));
        }
        panic!("missing b")
    };
    let prior = find(&old);
    let edited = old
        .edit(
            &TextEdit {
                range: [4, 5],
                replacement: "c".into(),
            },
            Limits::default(),
        )
        .unwrap();
    assert_eq!(prior, find(&edited));
}

#[test]
fn shared_parser_reads_the_grammar_self_source_in_every_mode() {
    let language = &nitride_cli::grammar_fixture::LANGUAGE;
    let source = include_str!("../../../languages/grammar/Grammar.ngr");
    for bits in 0..8 {
        let options = ParseOptions {
            strict_comments: bits & 1 != 0,
            token_children: bits & 2 != 0,
            alias_nodes: bits & 4 != 0,
        };
        let old = parse_with_options(language, source, Limits::default(), options).unwrap();
        let shared = parse_snapshot(
            language,
            source,
            Limits::default(),
            SnapshotMode::Strict(options),
        )
        .unwrap()
        .materialize()
        .unwrap();
        assert_eq!(old.root, shared.root, "mode {bits}");
        assert_eq!(
            old.nodes
                .iter()
                .map(|n| (n.rule, n.span, n.child_count))
                .collect::<Vec<_>>(),
            shared
                .nodes
                .iter()
                .map(|n| (n.rule, n.span, n.child_count))
                .collect::<Vec<_>>(),
            "mode {bits}"
        );
        for id in 0..old.nodes.len() as u32 {
            assert!(
                old.children(id)
                    .iter()
                    .map(|e| (old.label(e), e.node))
                    .eq(shared
                        .children(id)
                        .iter()
                        .map(|e| (shared.label(e), e.node))),
                "mode {bits}, node {id}"
            );
        }
    }
}
