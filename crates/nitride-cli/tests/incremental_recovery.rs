use nitride_core::*;
fn snapshot(source: &str) -> ParseSnapshot {
    parse_snapshot(
        &nitride_cli::csharp4::LANGUAGE,
        source,
        Limits::default(),
        SnapshotMode::Recovering(RecoveryOptions::default()),
    )
    .unwrap()
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
fn check(source: &str, snapshot: &ParseSnapshot) {
    let full = parse_recovering(
        &nitride_cli::csharp4::LANGUAGE,
        source,
        Limits::default(),
        RecoveryOptions::default(),
    );
    let flat = snapshot.materialize().unwrap();
    assert_eq!(normalized(&flat), normalized(&full.tree));
    assert_eq!(snapshot.diagnostics(), full.diagnostics);
    assert_eq!(
        flat.recovery()
            .map(|r| (&r.repairs, &r.coverage, &r.damaged_nodes)),
        full.tree
            .recovery()
            .map(|r| (&r.repairs, &r.coverage, &r.damaged_nodes))
    );
}
#[test]
fn missing_closer_edit_matches_full_recovery() {
    let source = "class C { int A(){ return 111; } int B(){ return 222; } }";
    let old = snapshot(source);
    let at = source.find("111;").unwrap() as u32 + 3;
    let new = old
        .edit(
            &TextEdit {
                range: [at, at + 1],
                replacement: String::new(),
            },
            Limits::default(),
        )
        .unwrap();
    let mut edited = source.to_owned();
    edited.remove(at as usize);
    check(&edited, &new);
    check(source, &old);
    assert_eq!(new.stats().full_parse_fallbacks, 0);
}
#[test]
fn healthy_sibling_after_error_keeps_identity() {
    let source = "class C { int A(){ return 111 } int B(){ return 222; } }";
    let old = snapshot(source);
    let find = |tree: &ParseSnapshot| {
        let mut work = vec![tree.root()];
        while let Some(node) = work.pop() {
            if node.text() == "int B(){ return 222; }" {
                return node.subtree_id();
            }
            work.extend(node.children().map(|(_, n)| n));
        }
        panic!("missing B")
    };
    let previous = find(&old);
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
    assert_eq!(previous, find(&new));
    check(&source.replacen("111", "1111", 1), &new);
}

#[test]
fn recovered_snapshot_never_executes_host_effects() {
    let language = &nitride_cli::inference::LANGUAGE;
    let source = "let x = ; value(x); let healthy = 3; value(healthy);";
    let tree = parse_snapshot(
        language,
        source,
        Limits::default(),
        SnapshotMode::Recovering(RecoveryOptions::default()),
    )
    .unwrap();
    let analysis = compile_snapshot_recovering(language, &tree, Limits::default());
    assert!(!analysis.diagnostics.is_empty());
    let program = analysis.program.unwrap();
    assert!(
        execute(
            &program,
            &nitride_cli::fixture_bindings_for(language),
            Limits::default()
        )
        .is_err()
    );
    assert!(compile_snapshot(language, &tree, Limits::default()).is_err());
}

#[test]
fn recovery_uses_shared_probes_and_warm_recognition() {
    let source = "class C { int A(){ return 111 } int B(){ return 222; } }";
    let old = snapshot(source);
    assert_eq!(old.stats().full_parse_fallbacks, 0);
    assert_eq!(old.stats().probe_descendants_copied, 0);
    let at = source.find("222").unwrap() as u32;
    let new = old
        .edit(
            &TextEdit {
                range: [at, at + 3],
                replacement: "223".into(),
            },
            Limits::default(),
        )
        .unwrap();
    check(&source.replacen("222", "223", 1), &new);
    assert_eq!(new.stats().full_parse_fallbacks, 0);
    assert!(new.stats().reused_subtrees > 0);
    assert_eq!(new.stats().source_bytes_copied, 3);
}

#[test]
fn swapped_catch_boundary_preserves_full_recovery() {
    let source = "class C {void M(){try { F(); } catch(E e) throw {; } finally {}}}";
    check(source, &snapshot(source));
}

#[test]
fn repeated_recovery_edits_keep_relative_coverage_canonical() {
    let source =
        "class C { int A(){ return 111 } /* gap */ int B(){ return 222; } int D(){return 3;} }";
    let mut old = snapshot(source);
    let mut text = source.to_string();
    let at = source.find("222").unwrap() as u32;
    for i in 0..100 {
        let replacement = if i % 2 == 0 { "2222" } else { "222" };
        let remove = if i % 2 == 0 { 3 } else { 4 };
        let edited = text.replace(
            if remove == 3 { "222;" } else { "2222;" },
            &format!("{replacement};"),
        );
        old = old
            .edit(
                &TextEdit {
                    range: [at, at + remove],
                    replacement: replacement.into(),
                },
                Limits::default(),
            )
            .unwrap();
        check(&edited, &old);
        text = edited;
    }
}

#[test]
fn truncated_deep_if_chain_matches_full_recovery() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/recovery/manifest.json")).unwrap();
    let variant = &manifest["variants"][1536];
    let source = variant["source"].as_str().unwrap();
    let shared = snapshot(source);
    let full = parse_recovering(
        &nitride_cli::csharp4::LANGUAGE,
        source,
        Limits::default(),
        RecoveryOptions::default(),
    );
    assert_eq!(
        shared.diagnostics(),
        full.diagnostics,
        "shared {:?}, full {:?}",
        shared.stats(),
        full.stats
    );
    check(source, &shared);
}

#[test]
fn unchanged_clean_repetition_replays_in_batches() {
    let source = format!(
        "class C {{ int broken(){{return 0 }} {} }}",
        (0..200)
            .map(|i| format!("int M{i}(){{return 222;}} "))
            .collect::<String>()
    );
    let old = snapshot(&source);
    let at = source.find("222").unwrap() as u32;
    let new = old
        .edit(
            &TextEdit {
                range: [at, at + 3],
                replacement: "223".into(),
            },
            Limits::default(),
        )
        .unwrap();
    check(&source.replacen("222", "223", 1), &new);
    assert!(new.stats().recovery_batches > 0);
}

#[test]
fn batching_preserves_pending_prefix_reopen_points() {
    let mut source = "using System;\nusing System.Linq;\n".to_string();
    for i in 0..200 {
        source += &format!(
            "namespace N{i} {{ public class C{i}<T> where T:class {{ int F(int x) {{ return 222{} }} }} }}\n",
            if i == 100 { "" } else { ";" }
        );
    }
    let old = snapshot(&source);
    let at = source.find("222").unwrap() as u32;
    let new = old
        .edit(
            &TextEdit {
                range: [at, at + 3],
                replacement: "223".into(),
            },
            Limits::default(),
        )
        .unwrap();
    check(&source.replacen("222", "223", 1), &new);
    assert!(
        new.stats().recovery_batches > 1,
        "{}",
        new.stats().recovery_batches
    );
}

#[test]
fn successive_damage_and_prefix_edits_preserve_recovery_placement() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/recovery/adversarial.json")).unwrap();
    for index in [217, 443, 940, 1035, 1038, 1126, 1127, 1129, 1266] {
        let variant = &manifest["variants"][index];
        let baseline = manifest["baselines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == variant["baseline"])
            .unwrap();
        let language =
            nitride_cli::recovery_language(baseline["profile"].as_str().unwrap()).unwrap();
        let mut source = baseline["source"].as_str().unwrap().to_string();
        let mut current = parse_snapshot(
            language,
            &source,
            Limits::default(),
            SnapshotMode::Recovering(RecoveryOptions::default()),
        )
        .unwrap();
        let mut simulated = source.clone();
        let mut trace = Vec::new();
        let mut inverse = Vec::new();
        for edit in variant["edits"].as_array().unwrap() {
            let a = edit["start"].as_u64().unwrap() as u32;
            let b = edit["end"].as_u64().unwrap() as u32;
            let replacement = edit["replacement"].as_str().unwrap().to_string();
            inverse.push(TextEdit {
                range: [a, a + replacement.len() as u32],
                replacement: simulated[a as usize..b as usize].into(),
            });
            simulated.replace_range(a as usize..b as usize, &replacement);
            trace.push(TextEdit {
                range: [a, b],
                replacement,
            });
        }
        trace.extend(inverse.into_iter().rev());
        for (step, edit) in trace.iter().enumerate() {
            let [a, b] = edit.range;
            let replacement = edit.replacement.as_str();
            current = current
                .edit(
                    &TextEdit {
                        range: [a, b],
                        replacement: replacement.into(),
                    },
                    Limits::default(),
                )
                .unwrap();
            source.replace_range(a as usize..b as usize, replacement);
            let full = parse_recovering(
                language,
                &source,
                Limits::default(),
                RecoveryOptions::default(),
            );
            let flat = current.materialize().unwrap();
            assert_eq!(
                current.diagnostics(),
                full.diagnostics,
                "case {index} step {step} diagnostics"
            );
            assert_eq!(
                normalized(&flat),
                normalized(&full.tree),
                "case {index} step {step} syntax"
            );
            assert_eq!(
                flat.recovery().map(|r| &r.repairs),
                full.tree.recovery().map(|r| &r.repairs),
                "case {index} step {step} repairs"
            );
            assert_eq!(
                flat.recovery().map(|r| &r.coverage),
                full.tree.recovery().map(|r| &r.coverage),
                "case {index} step {step} coverage"
            );
            assert_eq!(
                flat.recovery().map(|r| &r.damaged_nodes),
                full.tree.recovery().map(|r| &r.damaged_nodes),
                "case {index} step {step} damage"
            );
        }
    }
}

#[test]
fn dense_recovery_does_not_publish_disposable_probe_failures() {
    let source = format!(
        "class C {{ {} }}",
        (0..200)
            .map(|i| format!(
                "int F{i}(int x) {{ if (x > 0) {{ return x * 2; }} else {{ return x }} }}"
            ))
            .collect::<String>()
    );
    let current = snapshot(&source);
    assert_eq!(current.stats().full_parse_fallbacks, 0);
    let storage = current.storage_stats();
    assert!(
        storage.index_bytes <= storage.syntax_bytes,
        "published recovery index {} exceeds syntax {}",
        storage.index_bytes,
        storage.syntax_bytes
    );
    check(&source, &current);
}

#[test]
fn deleting_call_opener_after_query_methods_matches_full_recovery() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/recovery/manifest.json")).unwrap();
    let variant = &manifest["variants"][1712];
    let broken = variant["source"].as_str().unwrap();
    let baseline = manifest["baselines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|base| base["id"] == variant["baseline"])
        .unwrap()["source"]
        .as_str()
        .unwrap();
    let fresh = snapshot(broken);
    let full = parse_recovering(
        &nitride_cli::csharp4::LANGUAGE,
        broken,
        Limits::default(),
        RecoveryOptions::default(),
    );
    assert_eq!(
        fresh.diagnostics(),
        full.diagnostics,
        "fresh recovery diagnostics"
    );
    check(broken, &fresh);
    let new = snapshot(baseline)
        .edit(
            &TextEdit {
                range: [2120, 2121],
                replacement: String::new(),
            },
            Limits::default(),
        )
        .unwrap();
    assert_eq!(
        new.diagnostics(),
        full.diagnostics,
        "incremental recovery diagnostics"
    );
    let exported = new.materialize().unwrap();
    let a = normalized(&exported);
    let b = normalized(&full.tree);
    let first = a.iter().zip(&b).position(|(a, b)| a != b);
    assert!(
        first.is_none(),
        "first mismatch {:?}: shared {:?}, flat {:?}",
        first,
        first.map(|i| &a[i]),
        first.map(|i| &b[i])
    );
    assert_eq!(a.len(), b.len());
    assert_eq!(
        exported
            .recovery()
            .map(|r| (&r.repairs, &r.coverage, &r.damaged_nodes)),
        full.tree
            .recovery()
            .map(|r| (&r.repairs, &r.coverage, &r.damaged_nodes))
    );
}

fn healthy_prefix_with_one_error() -> String {
    let mut source = String::new();
    for i in 0..40 {
        source.push_str(&format!(
            "namespace N{i} {{ class C{i} {{ int F(int x) {{ if (x > {i}) {{ return x * 2; }} return x; }} }} }}\n"
        ));
    }
    source.push_str("namespace Broken { class B { int F() { return 1 } } }\n");
    for i in 0..4 {
        source.push_str(&format!("namespace M{i} {{ class D{i} {{ }} }}\n"));
    }
    source
}

#[test]
fn cold_recovery_reuses_the_strict_attempt_prefix() {
    let source = healthy_prefix_with_one_error();
    let cold = snapshot(&source);
    check(&source, &cold);
    assert_eq!(cold.stats().full_parse_fallbacks, 0);
    assert!(cold.stats().reused_subtrees >= 40, "{:?}", cold.stats());
}

#[test]
fn generated_cold_recovery_reuses_the_strict_attempt_prefix() {
    let source = healthy_prefix_with_one_error();
    let cold = parse_snapshot_generated(
        &nitride_cli::csharp4::PARSER,
        &source,
        Limits::default(),
        SnapshotMode::Recovering(RecoveryOptions::default()),
    )
    .unwrap();
    check(&source, &cold);
    assert_eq!(cold.stats().full_parse_fallbacks, 0);
    assert!(cold.stats().reused_subtrees >= 40, "{:?}", cold.stats());
    // Seeded facts survive into the published index for later edits.
    let at = source.find("return 1").unwrap() as u32 + 7;
    let edited = cold
        .edit(
            &TextEdit {
                range: [at, at + 1],
                replacement: "2".into(),
            },
            Limits::default(),
        )
        .unwrap();
    check(&source.replacen("return 1", "return 2", 1), &edited);
    assert!(edited.stats().reused_subtrees > 0);
}
