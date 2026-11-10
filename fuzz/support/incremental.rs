//! Shared by the coverage-guided target and the ordinary CI smoke test.
use nitride_core::*;
use std::collections::VecDeque;

type NodeShape = (u16, [u32; 2], Vec<(String, u32)>);

fn shape(tree: &SyntaxTree<'_>) -> Vec<NodeShape> {
    tree.nodes
        .iter()
        .enumerate()
        .map(|(id, node)| {
            (
                node.rule,
                node.span,
                tree.children(id as u32)
                    .iter()
                    .map(|edge| (tree.label(edge).to_owned(), edge.node))
                    .collect(),
            )
        })
        .collect()
}

fn compare_trees(a: &SyntaxTree<'_>, b: &SyntaxTree<'_>) {
    assert_eq!(a.source, b.source);
    assert_eq!(a.root, b.root);
    assert_eq!(shape(a), shape(b));
    assert_eq!(
        a.recovery()
            .map(|r| (&r.repairs, &r.coverage, &r.damaged_nodes)),
        b.recovery()
            .map(|r| (&r.repairs, &r.coverage, &r.damaged_nodes)),
    );
}

fn limited(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.category == "limit")
}

fn compare(
    language: &'static Language<'static>,
    source: &str,
    snapshot: &ParseSnapshot,
    mode: SnapshotMode,
    limits: Limits,
) {
    let actual = snapshot.materialize().unwrap();
    assert_eq!(actual.source, source);
    match mode {
        SnapshotMode::Strict(options) => {
            let full = parse_with_options(language, source, limits, options);
            match full {
                Ok(full) => compare_trees(&actual, &full),
                Err(error) => assert_eq!(error.category, "limit", "{error:?}"),
            }
        }
        SnapshotMode::Recovering(options) => {
            let full = parse_recovering(language, source, limits, options);
            // Reuse changes work spent; resource-limit results need not match.
            if !limited(snapshot.diagnostics()) && !limited(&full.diagnostics) {
                compare_trees(&actual, &full.tree);
                assert_eq!(snapshot.diagnostics(), full.diagnostics);
            }
        }
    }
}

/// Decode one bounded document followed by at most sixteen edits. Return the
/// number of attempted edits so the CI test can reject an inert corpus.
pub fn exercise(data: &[u8]) -> usize {
    let Some((&flags, data)) = data.split_first() else {
        return 0;
    };
    let Some((&source_len, data)) = data.split_first() else {
        return 0;
    };
    let source_len = usize::from(source_len).min(data.len());
    let (source, mut data) = data.split_at(source_len);
    let mut source = String::from_utf8_lossy(source).into_owned();
    let (language, parser) = match flags & 3 {
        0 => (&nitride_cli::LANGUAGE, &nitride_cli::PARSER),
        1 => (
            &nitride_cli::json_language::LANGUAGE,
            &nitride_cli::json_language::PARSER,
        ),
        2 => (&nitride_cli::xml::LANGUAGE, &nitride_cli::xml::PARSER),
        _ => (
            &nitride_cli::csharp4::LANGUAGE,
            &nitride_cli::csharp4::PARSER,
        ),
    };
    let mode = if flags & 4 == 0 {
        SnapshotMode::Strict(ParseOptions {
            strict_comments: true,
            ..Default::default()
        })
    } else {
        SnapshotMode::Recovering(RecoveryOptions {
            max_states: 128,
            max_local_bytes: 512,
            ..Default::default()
        })
    };
    let limits = Limits {
        work: 1_000_000,
        parser_work_per_byte: 0,
        parser_frames: 4096,
        parser_spill_stack_bytes: 8 * 1024 * 1024,
        ..Default::default()
    };
    let initial = if flags & 8 == 0 {
        parse_snapshot(language, &source, limits, mode)
    } else {
        parse_snapshot_generated(parser, &source, limits, mode)
    };
    let mut current = match initial {
        Ok(snapshot) => snapshot,
        Err(error) => {
            if let SnapshotMode::Strict(options) = mode {
                let full = parse_with_options(language, &source, limits, options);
                if error.category != "limit"
                    && !full.as_ref().err().is_some_and(|e| e.category == "limit")
                {
                    assert_eq!(Some(error), full.err());
                }
            } else {
                assert_eq!(error.category, "limit", "{error:?}");
            }
            return 0;
        }
    };
    compare(language, &source, &current, mode, limits);
    let mut readers = VecDeque::new();
    let mut attempts = 0;
    while data.len() >= 4 && attempts < 16 {
        let at = usize::from(u16::from_le_bytes([data[0], data[1]]));
        let remove = usize::from(data[2]);
        let insert_len = usize::from(data[3] % 25).min(data.len() - 4);
        let replacement = String::from_utf8_lossy(&data[4..4 + insert_len]).into_owned();
        data = &data[4 + insert_len..];
        let boundaries: Vec<_> = source
            .char_indices()
            .map(|(at, _)| at)
            .chain(std::iter::once(source.len()))
            .collect();
        let index = at % boundaries.len();
        let start = boundaries[index];
        let end = boundaries[(index + remove).min(boundaries.len() - 1)];
        let edit = TextEdit {
            range: [start as u32, end as u32],
            replacement,
        };
        let mut changed = source.clone();
        changed.replace_range(start..end, &edit.replacement);
        readers.push_back((current.clone(), source.clone()));
        if readers.len() > 2 {
            readers.pop_front();
        }
        match current.edit(&edit, limits) {
            Ok(next) => {
                compare(language, &changed, &next, mode, limits);
                current = next;
                source = changed;
            }
            Err(error) => match mode {
                SnapshotMode::Strict(options) => {
                    let full = parse_with_options(language, &changed, limits, options);
                    if error.category != "limit"
                        && !full.as_ref().err().is_some_and(|e| e.category == "limit")
                    {
                        assert_eq!(Some(error), full.err());
                    }
                }
                SnapshotMode::Recovering(_) => {
                    assert_eq!(error.category, "limit", "{error:?}");
                }
            },
        }
        // Retained readers must stay unchanged even after later failed edits.
        for (reader, text) in &readers {
            compare(language, text, reader, mode, limits);
        }
        attempts += 1;
    }
    attempts
}
