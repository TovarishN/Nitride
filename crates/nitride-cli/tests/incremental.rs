use nitride_core::*;
fn snapshot(source: &str) -> ParseSnapshot {
    parse_snapshot(
        &nitride_cli::csharp4::LANGUAGE,
        source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap()
}
fn find<'a>(snapshot: &'a ParseSnapshot, text: &str) -> NodeView<'a> {
    let mut pending = vec![snapshot.root()];
    while let Some(node) = pending.pop() {
        if node.text() == text {
            return node;
        }
        pending.extend(node.children().map(|(_, n)| n));
    }
    panic!("missing node {text}")
}
type NormalizedNode = (u16, [u32; 2], Vec<(&'static str, u32)>);
fn normalized(tree: &SyntaxTree<'static>) -> Vec<NormalizedNode> {
    tree.nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (
                n.rule,
                n.span,
                tree.children(i as u32)
                    .iter()
                    .map(|e| (tree.label(e), e.node))
                    .collect(),
            )
        })
        .collect()
}
#[test]
fn editing_one_method_shares_untouched_methods() {
    let source = "class C { int A(){ return 111; } int B(){ return 222; } int D(){ return 333; } }";
    let old = snapshot(source);
    let before = find(&old, "int B(){ return 222; }");
    let at = source.find("111").unwrap() as u32;
    let new = old
        .edit(
            &TextEdit {
                range: [at, at + 3],
                replacement: "1111".into(),
            },
            Limits::default(),
        )
        .unwrap();
    let after = find(&new, "int B(){ return 222; }");
    assert_eq!(before.subtree_id(), after.subtree_id());
    assert_eq!(after.span()[0], before.span()[0] + 1);
    assert_eq!(before.text(), "int B(){ return 222; }");
    assert!(new.stats().reused_subtrees > 0);
    let edited = source.replacen("111", "1111", 1);
    let full = parse(&nitride_cli::csharp4::LANGUAGE, &edited, Limits::default()).unwrap();
    assert_eq!(normalized(&new.materialize().unwrap()), normalized(&full));
}
#[test]
fn invalid_edit_is_non_mutating() {
    let old = snapshot("class C { string s=\"😀\"; }");
    let source = old.materialize().unwrap().source;
    let at = source.find('😀').unwrap() as u32;
    assert!(
        old.edit(
            &TextEdit {
                range: [at + 1, at + 2],
                replacement: "x".into()
            },
            Limits::default()
        )
        .is_err()
    );
    assert_eq!(old.materialize().unwrap().source, source);
}
#[test]
fn cross_boundary_comment_changes_tokenization() {
    let source = "class C { int A(){ return 111; } int B(){ return 222; } }";
    let old = snapshot(source);
    let at = source.find("int B").unwrap() as u32;
    let result = old.edit(
        &TextEdit {
            range: [at, at],
            replacement: "//".into(),
        },
        Limits::default(),
    );
    let edited = format!("{}//{}", &source[..at as usize], &source[at as usize..]);
    let full = parse(&nitride_cli::csharp4::LANGUAGE, &edited, Limits::default());
    assert_eq!(result.err(), full.err());
}

#[test]
fn seeded_edits_match_fresh_strict_parsing() {
    let language = &nitride_cli::csharp4::LANGUAGE;
    let mut text = "class C { int A(){ return 111; } int B(){ return 222; } }".to_owned();
    let mut current = snapshot(&text);
    let mut seed = 20261025_u64;
    for _ in 0..1000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let at = (seed as usize) % (text.len() + 1);
        let remove = usize::from(seed & 1 != 0 && at < text.len());
        let insertion = ["1", " ", "/", "*", ";", "}", "(", "", "\n"][(seed >> 32) as usize % 9];
        let edit = TextEdit {
            range: [at as u32, (at + remove) as u32],
            replacement: insertion.into(),
        };
        let mut edited = text.clone();
        edited.replace_range(at..at + remove, insertion);
        let warm = current.edit(&edit, Limits::default());
        let full = parse(language, &edited, Limits::default());
        match (warm, full) {
            (Ok(warm), Ok(full)) => {
                assert_eq!(
                    normalized(&warm.materialize().unwrap()),
                    normalized(&full),
                    "edit {at}: {insertion:?}"
                );
                current = warm;
                text = edited;
            }
            (Err(warm), Err(full)) => assert_eq!(warm, full, "edit {at}: {insertion:?}"),
            _ => panic!("acceptance mismatch at {at} inserting {insertion:?}: {edited}"),
        }
    }
}

#[test]
#[ignore = "release-mode 100,000 edits each with zero/eight retained readers"]
fn bounded_reader_window_does_not_retain_edit_history() {
    let source = "class C { int A(){ return 111; } int B(){ return 222; } int D(){ return 333; } }";
    for window in [0, 8] {
        let original = snapshot(source);
        let bound = original.storage_stats().physical_retained_bytes * 3 / 2
            + 8 * 1024 * 1024
            + 2 * 64 * 1024;
        let at = source.find("111").unwrap() as u32;
        let mut current = original.clone();
        let mut readers = std::collections::VecDeque::new();
        let mut maintenance = 0;
        for step in 0..100_000 {
            readers.push_back(current.clone());
            if readers.len() > window {
                readers.pop_front();
            }
            current = current
                .edit(
                    &TextEdit {
                        range: [at, at + 3],
                        replacement: if step % 2 == 0 { "112" } else { "111" }.into(),
                    },
                    Limits::default(),
                )
                .unwrap();
            maintenance += current.stats().maintenance_passes;
        }
        let refs: Vec<_> = readers.iter().chain(std::iter::once(&current)).collect();
        let union = shared_storage_bytes(&refs);
        drop(refs);
        drop(readers);
        assert!(
            current.storage_stats().physical_retained_bytes <= bound,
            "retained {} bytes, bound {bound}",
            current.storage_stats().physical_retained_bytes
        );
        assert_eq!(original.materialize().unwrap().source, source);
        println!(
            "{}",
            serde_json::json!({"window":window,"edits":100_000,"latest_bytes":current.storage_stats().physical_retained_bytes,"fresh_bytes":original.storage_stats().physical_retained_bytes,"bound_bytes":bound,"reader_union_bytes":union,"maintenance_passes":maintenance})
        );
    }
}

#[test]
fn snapshot_semantics_match_flat_and_execute() {
    let language = &nitride_cli::power::LANGUAGE;
    let source = "let abcdefgh : Scalar = 2; abcdefgh ^ 3;";
    let snapshot = parse_snapshot(
        language,
        source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let compiled = compile_snapshot(language, &snapshot, Limits::default()).unwrap();
    assert_eq!(
        execute(
            &compiled,
            &nitride_cli::fixture_bindings_for(language),
            Limits::default()
        )
        .unwrap()
        .values,
        vec![8.0]
    );
}
#[test]
fn scope_edit_rebinds_shared_call() {
    let language = &nitride_cli::functions::LANGUAGE;
    let source = "def sum(a: Scalar, b: Scalar)=add(a,b); call sum(1111,2222);";
    let old = parse_snapshot(
        language,
        source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    compile_snapshot(language, &old, Limits::default()).unwrap();
    let previous = find(&old, "call sum(1111,2222);").subtree_id();
    let at = source.find("sum").unwrap() as u32;
    let new = old
        .edit(
            &TextEdit {
                range: [at, at + 3],
                replacement: "foo".into(),
            },
            Limits::default(),
        )
        .unwrap();
    assert_eq!(previous, find(&new, "call sum(1111,2222);").subtree_id());
    let source = source.replacen("sum", "foo", 1);
    let tree = parse(language, &source, Limits::default()).unwrap();
    assert_eq!(
        compile_snapshot(language, &new, Limits::default()).err(),
        compile(language, &tree, Limits::default()).err()
    );
    assert!(compile_snapshot(language, &new, Limits::default()).is_err());
}
#[test]
fn cross_piece_names_bind_without_flattening_document() {
    let language = &nitride_cli::power::LANGUAGE;
    let source = format!(
        "{}let abcdefgh : Scalar = 2; abcdefgh ^ 3;",
        " ".repeat(64 * 1024 - 6)
    );
    let tree = parse_snapshot(
        language,
        &source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    let compiled = compile_snapshot(language, &tree, Limits::default()).unwrap();
    assert_eq!(
        execute(
            &compiled,
            &nitride_cli::fixture_bindings_for(language),
            Limits::default()
        )
        .unwrap()
        .values,
        vec![8.0]
    );
}
#[test]
fn snapshot_compilation_rejects_other_language() {
    let tree = parse_snapshot(
        &nitride_cli::power::LANGUAGE,
        "2;",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    assert_eq!(
        compile_snapshot(&nitride_cli::LANGUAGE, &tree, Limits::default()).unwrap_err()[0].category,
        "capability"
    );
}

#[test]
fn every_typed_fixture_preserves_hir_bindings_and_origins() {
    fn signature(p: &Program) -> String {
        format!(
            "{:?}",
            (
                &p.nodes,
                &p.args,
                &p.statements,
                &p.bindings,
                &p.builtin_bindings,
                &p.origin_spans,
                &p.origin_ranges,
                &p.origin_rewrites,
                &p.text
            )
        )
    }
    for (profile, fixture) in [
        ("portable", include_str!("../../../fixtures/reference.json")),
        (
            "power",
            include_str!("../../../fixtures/power-reference.json"),
        ),
        (
            "calls",
            include_str!("../../../fixtures/calls-reference.json"),
        ),
        (
            "functions",
            include_str!("../../../fixtures/functions-reference.json"),
        ),
        (
            "namespaces",
            include_str!("../../../fixtures/namespaces-reference.json"),
        ),
        (
            "inference",
            include_str!("../../../fixtures/inference-reference.json"),
        ),
        (
            "rewrites",
            include_str!("../../../fixtures/rewrites-reference.json"),
        ),
        (
            "dates",
            include_str!("../../../fixtures/dates-reference.json"),
        ),
    ] {
        let language = nitride_cli::recovery_language(profile).unwrap();
        let cases: serde_json::Value = serde_json::from_str(fixture).unwrap();
        for case in cases.as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let Ok(flat) = parse(language, source, Limits::default()) else {
                continue;
            };
            let shared = parse_snapshot(
                language,
                source,
                Limits::default(),
                SnapshotMode::Strict(ParseOptions::default()),
            )
            .unwrap();
            let before = compile(language, &flat, Limits::default());
            let after = compile_snapshot(language, &shared, Limits::default());
            match (before, after) {
                (Ok(a), Ok(b)) => {
                    assert_eq!(signature(&a), signature(&b), "{profile}: {}", case["name"])
                }
                (Err(a), Err(b)) => assert_eq!(a, b, "{profile}: {}", case["name"]),
                _ => panic!("compile acceptance mismatch {profile}: {}", case["name"]),
            }
        }
    }
}

#[test]
fn precedence_edits_and_warm_frame_caps_match_fresh_parsing() {
    let language = &nitride_cli::LANGUAGE;
    let source = "1 + 2 * (3 + 4); 5 + 6;";
    let old = parse_snapshot(
        language,
        source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    for replacement in ["*", "-", "+", "/", "", "**"] {
        let mut edited = source.to_string();
        edited.replace_range(2..3, replacement);
        let warm = old.edit(
            &TextEdit {
                range: [2, 3],
                replacement: replacement.into(),
            },
            Limits::default(),
        );
        let fresh = parse(language, &edited, Limits::default());
        match (warm, fresh) {
            (Ok(a), Ok(b)) => assert_eq!(normalized(&a.materialize().unwrap()), normalized(&b)),
            (Err(a), Err(b)) => assert_eq!(a, b),
            _ => panic!("acceptance {replacement}"),
        }
    }
    let limits = Limits {
        parser_frames: 3,
        ..Limits::default()
    };
    let warm = old
        .edit(
            &TextEdit {
                range: [20, 21],
                replacement: "6".into(),
            },
            limits,
        )
        .err()
        .unwrap();
    assert_eq!(warm.category, "limit");
    assert!(
        parse_snapshot(
            language,
            source,
            limits,
            SnapshotMode::Strict(ParseOptions::default())
        )
        .is_err()
    );
}

#[test]
fn old_readers_remain_valid_while_another_thread_edits() {
    let source = "class C { int A(){ return 111; } int B(){ return 222; } }";
    let original = snapshot(source);
    let at = source.find("111").unwrap() as u32;
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let old = &original;
            scope.spawn(move || {
                for _ in 0..100 {
                    assert_eq!(old.materialize().unwrap().source, source);
                }
            });
        }
        let mut latest = original.clone();
        for i in 0..100 {
            latest = latest
                .edit(
                    &TextEdit {
                        range: [at, at + 3],
                        replacement: if i % 2 == 0 { "112" } else { "111" }.into(),
                    },
                    Limits::default(),
                )
                .unwrap();
        }
    });
}

#[test]
fn deliberately_retained_history_counts_unique_storage() {
    let source = "class C { int M(){return 111;} }";
    let mut current = snapshot(source);
    let at = source.find("111").unwrap() as u32;
    let mut history = Vec::new();
    for i in 0..1000 {
        history.push(current.clone());
        current = current
            .edit(
                &TextEdit {
                    range: [at, at + 3],
                    replacement: if i % 2 == 0 { "112" } else { "111" }.into(),
                },
                Limits::default(),
            )
            .unwrap();
    }
    let readers: Vec<_> = history.iter().chain(std::iter::once(&current)).collect();
    let physical = shared_storage_bytes(&readers);
    let logical: usize = readers
        .iter()
        .map(|s| s.storage_stats().physical_retained_bytes)
        .sum();
    assert!(physical > current.storage_stats().physical_retained_bytes);
    assert!(physical < logical);
    assert_eq!(history[0].materialize().unwrap().source, source);
    println!(
        "{}",
        serde_json::json!({"history_snapshots":readers.len(),"unique_bytes":physical,"logical_sum_bytes":logical,"latest_bytes":current.storage_stats().physical_retained_bytes})
    );
}

#[test]
fn seeded_edits_match_fresh_parsing_in_every_fixture_profile() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/recovery/adversarial.json")).unwrap();
    for baseline in manifest["baselines"].as_array().unwrap() {
        let profile = baseline["profile"].as_str().unwrap();
        let language = nitride_cli::recovery_language(profile).unwrap();
        let mut source = baseline["source"].as_str().unwrap().to_string();
        let mut current = parse_snapshot(
            language,
            &source,
            Limits::default(),
            SnapshotMode::Strict(ParseOptions::default()),
        )
        .unwrap();
        let mut seed = 20261025_u64;
        for step in 0..1000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let boundaries: Vec<_> = source
                .char_indices()
                .map(|(at, _)| at)
                .chain(std::iter::once(source.len()))
                .collect();
            let index = seed as usize % boundaries.len();
            let at = boundaries[index];
            let end = if seed & 1 != 0 {
                boundaries.get(index + 1).copied().unwrap_or(at)
            } else {
                at
            };
            let replacement = ["1", " ", "/", "*", ";", "}", "(", "", "\n", "λ", "😀"]
                [(seed >> 32) as usize % 11];
            if source.len() + replacement.len() > 1024 {
                continue;
            }
            let mut changed = source.clone();
            changed.replace_range(at..end, replacement);
            let warm = current.edit(
                &TextEdit {
                    range: [at as u32, end as u32],
                    replacement: replacement.into(),
                },
                Limits::default(),
            );
            let fresh = parse(language, &changed, Limits::default());
            match (warm, fresh) {
                (Ok(a), Ok(b)) => {
                    assert_eq!(
                        normalized(&a.materialize().unwrap()),
                        normalized(&b),
                        "{profile} step {step}"
                    );
                    current = a;
                    source = changed;
                }
                (Err(a), Err(b)) => assert_eq!(a, b, "{profile} step {step}: {changed:?}"),
                _ => panic!("acceptance mismatch {profile} step {step}: {changed:?}"),
            }
        }
    }
}
