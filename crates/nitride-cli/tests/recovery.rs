use nitride_core::*;

fn recover(language: &'static Language, source: &str) -> RecoveredParse {
    parse_recovering(
        language,
        source,
        Limits::default(),
        RecoveryOptions::default(),
    )
}

fn shape(tree: &SyntaxTree, root: u32) -> Vec<(u16, [u32; 2], &'static str)> {
    let base = tree.nodes[root as usize].span[0];
    let mut pending = vec![(root, "")];
    let mut result = vec![];
    while let Some((id, label)) = pending.pop() {
        let n = tree.nodes[id as usize];
        result.push((n.rule, [n.span[0] - base, n.span[1] - base], label));
        pending.extend(
            tree.children(id)
                .iter()
                .rev()
                .map(|e| (e.node, tree.label(e))),
        );
    }
    result
}

#[test]
fn csharp_missing_semicolon_preserves_next_method() {
    let language = &nitride_cli::csharp4::LANGUAGE;
    let valid = "class C { int A() { return 1; } int B() { return 2; } }";
    let damaged = "class C { int A() { return 1 } int B() { return 2; } }";
    let strict = parse(language, valid, Limits::default()).unwrap();
    let parsed = recover(language, damaged);
    let return_type = parsed
        .tree
        .nodes
        .iter()
        .find(|n| {
            language.rules[n.rule as usize]
                .name
                .ends_with(".BuiltinTypeName")
                && n.span[0] == 10
        })
        .unwrap();
    assert_eq!(
        return_type.span,
        [10, 13],
        "a failed postfix probe must roll back trailing trivia"
    );
    assert_eq!(
        parsed.diagnostics.len(),
        1,
        "{:?}, {:?}",
        parsed.diagnostics,
        parsed.stats
    );
    assert_eq!(parsed.stats.fallback_count, 0, "{:?}", parsed.diagnostics);
    let method = |tree: &SyntaxTree| {
        tree.nodes
            .iter()
            .enumerate()
            .find(|(_, n)| {
                language.rules[n.rule as usize]
                    .name
                    .ends_with(".MethodDeclaration")
                    && tree.source[n.span[0] as usize..n.span[1] as usize].starts_with("int B")
            })
            .map(|(id, _)| id as u32)
            .unwrap()
    };
    assert_eq!(
        shape(&strict, method(&strict)),
        shape(&parsed.tree, method(&parsed.tree))
    );
}

#[test]
fn missing_operand_keeps_later_typed_statements() {
    let parsed = recover(
        &nitride_cli::inference::LANGUAGE,
        "let x = ; value(x) + 1; 2 + 3;",
    );
    assert_eq!(
        parsed.stats.fallback_count, 0,
        "{:?}, {:?}",
        parsed.diagnostics, parsed.stats
    );
    assert_eq!(parsed.diagnostics.len(), 1, "{:?}", parsed.diagnostics);
    assert!(
        parsed
            .tree
            .nodes
            .iter()
            .any(|n| &parsed.tree.source[n.span[0] as usize..n.span[1] as usize] == "2 + 3")
    );
}

#[test]
fn valid_csharp_recovery_keeps_every_strict_node() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/csharp4-reference.json")).unwrap();
    for case in cases
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["observation"]["ok"] == true)
    {
        let source = case["source"].as_str().unwrap();
        let strict = parse(&nitride_cli::csharp4::LANGUAGE, source, Limits::default()).unwrap();
        let parsed = recover(&nitride_cli::csharp4::LANGUAGE, source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{}: {:?}",
            case["name"],
            parsed.diagnostics
        );
        assert!(parsed.tree.recovery().is_none());
        assert_eq!(
            shape(&strict, strict.root),
            shape(&parsed.tree, parsed.tree.root),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn damaged_initializer_propagates_error_without_hiding_healthy_hir() {
    let language = &nitride_cli::inference::LANGUAGE;
    let parsed = recover(language, "let x = ; value(x) + 1; 2 + 3;");
    let analysis = compile_recovering(language, &parsed, Limits::default());
    assert_eq!(analysis.diagnostics.len(), 1, "{:?}", analysis.diagnostics);
    let program = analysis.program.unwrap();
    assert_eq!(program.statements.len(), 3);
    assert_eq!(program.bindings.len(), 1);
    assert_eq!(
        program.nodes[program.statements[0].value as usize].ty,
        ERROR_TYPE
    );
    assert_eq!(
        program.nodes[program.statements[1].value as usize].ty,
        ERROR_TYPE
    );
    let last = &program.nodes[program.statements[2].value as usize];
    assert_eq!(last.ty, "Core.Scalar");
    assert!(matches!(last.kind, HirKind::Operation { .. }));
}

#[test]
fn recovered_program_cannot_run_even_when_caller_ignores_analysis_diagnostics() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let language = &nitride_cli::inference::LANGUAGE;
    let parsed = recover(language, "2 + 3; let x = ;");
    let program = compile_recovering(language, &parsed, Limits::default())
        .program
        .unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let bindings: Vec<_> = language
        .operations
        .iter()
        .map(|signature| {
            let counter = counter.clone();
            HostBinding::new(*signature, move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(0.0)
            })
        })
        .collect();
    assert!(preflight(&program, &bindings).is_err());
    assert!(execute(&program, &bindings, Limits::default()).is_err());
    assert_eq!(counter.load(Ordering::SeqCst), 0);
    assert!(!program.diagnostics().is_empty());
}

#[test]
fn template_recovery_keeps_valid_calls_after_an_invalid_unused_body() {
    let language = &nitride_cli::functions::LANGUAGE;
    let parsed = recover(
        language,
        "def bad(x: Scalar) = add(x, ); def good(x: Scalar) = add(x, 2); call good(3);",
    );
    let analysis = compile_recovering(language, &parsed, Limits::default());
    assert_eq!(analysis.diagnostics.len(), 1, "{:?}", analysis.diagnostics);
    let program = analysis.program.expect("partial template HIR");
    assert_eq!(program.statements.len(), 1);
    assert_eq!(
        program.nodes[program.statements[0].value as usize].ty,
        "Core.Scalar"
    );
    assert!(matches!(
        program.nodes[program.statements[0].value as usize].kind,
        HirKind::Operation { .. }
    ));
}

#[test]
fn independent_semantic_error_is_reported_after_syntax_damage() {
    let language = &nitride_cli::inference::LANGUAGE;
    let parsed = recover(language, "let x = ; value(x) + 1; duration(1) + 2; 2 + 3;");
    let analysis = compile_recovering(language, &parsed, Limits::default());
    assert_eq!(analysis.diagnostics.len(), 2, "{:?}", analysis.diagnostics);
    assert_eq!(analysis.diagnostics[0].category, "syntax");
    assert_eq!(analysis.diagnostics[1].category, "type");
    assert_eq!(analysis.program.unwrap().statements.len(), 4);
}

#[test]
fn cli_recovery_returns_partial_tree_without_executing() {
    let path = std::env::temp_dir().join(format!("nitride-recovery-{}.txt", std::process::id()));
    std::fs::write(&path, "let x = ; value(x) + 1; 2 + 3;").unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_nitride"))
        .args(["--recover", "inference"])
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        result.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(output["diagnostics"].as_array().unwrap().len(), 1);
    assert_eq!(output["statements"].as_array().unwrap().len(), 3);
    assert!(!output["nodes"].as_array().unwrap().is_empty());
    assert_eq!(output["trace"], serde_json::json!([]));
}

#[test]
fn recovery_propagates_unknown_across_every_typed_profile() {
    for (profile, source) in [
        ("portable", "let x : Scalar = ; x + 1; 2 + 3;"),
        ("namespaces", "let x : Scalar = ; either(x) + 1; 2 + 3;"),
        ("calls", "let x = ; sum(x,1); neg(2);"),
        ("dates", "let x = ; x + 1 days; today;"),
        ("rewrites", "let x = ; twice(x); 2 fortnights;"),
    ] {
        let language = nitride_cli::recovery_language(profile).unwrap();
        let parsed = recover(language, source);
        let analysis = compile_recovering(language, &parsed, Limits::default());
        assert_eq!(
            analysis.diagnostics.len(),
            1,
            "{profile}: {:?}",
            analysis.diagnostics
        );
        let program = analysis.program.unwrap();
        assert_eq!(program.statements.len(), 3, "{profile}");
        for statement in &program.statements[..2] {
            assert_eq!(
                program.nodes[statement.value as usize].ty, ERROR_TYPE,
                "{profile}"
            );
        }
        assert_ne!(
            program.nodes[program.statements[2].value as usize].ty, ERROR_TYPE,
            "{profile}"
        );
    }
}

#[test]
fn missing_declaration_name_creates_no_symbol() {
    let language = &nitride_cli::LANGUAGE;
    let parsed = recover(language, "let : Scalar = 1; 2 + 3;");
    let analysis = compile_recovering(language, &parsed, Limits::default());
    assert_eq!(analysis.diagnostics.len(), 1, "{:?}", analysis.diagnostics);
    let program = analysis.program.unwrap();
    assert_eq!(program.statements[0].symbol, None);
    assert!(program.bindings.is_empty());
}

#[test]
fn syntax_diagnostics_block_value_providers_as_well_as_handlers() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let language = &nitride_cli::dates::LANGUAGE;
    let parsed = recover(language, "today; let x = ;");
    let program = compile_recovering(language, &parsed, Limits::default())
        .program
        .unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let capture = counter.clone();
    let values = [HostValue::new("value", "today", "Dates.Day", move || {
        capture.fetch_add(1, Ordering::SeqCst);
        Ok(1.0)
    })];
    assert!(preflight_with_values(&program, &[], &values).is_err());
    assert!(execute_with_values(&program, &[], &values, Limits::default()).is_err());
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[test]
fn recovery_profiler_counts_metadata_and_never_executes() {
    let path = std::env::temp_dir().join(format!(
        "nitride-recovery-profile-{}.txt",
        std::process::id()
    ));
    std::fs::write(&path, "let x = ; 2 + 3;").unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_nitride-profile"))
        .args(["--recover", "inference"])
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(output["recovery"]["work"].as_u64().unwrap() > 0);
    assert!(
        output["parse"]["retained_bytes"].as_u64().unwrap()
            >= output["syntax_capacity_bytes"].as_u64().unwrap()
    );
    assert_eq!(output["execution"], serde_json::Value::Null);
}

#[test]
fn truncated_file_keeps_completed_members_when_local_cost_is_exhausted() {
    let language = &nitride_cli::csharp4::LANGUAGE;
    let source = "class C { int A() { return 1; } int B() { return 2;";
    let parsed = parse_recovering(
        language,
        source,
        Limits::default(),
        RecoveryOptions {
            max_local_cost: 1,
            ..RecoveryOptions::default()
        },
    );
    assert!(!parsed.diagnostics.is_empty());
    assert!(
        parsed
            .tree
            .nodes
            .iter()
            .any(|n| language.rules[n.rule as usize]
                .name
                .ends_with(".MethodDeclaration")
                && &source[n.span[0] as usize..n.span[1] as usize] == "int A() { return 1; }"),
        "{:?}",
        parsed.diagnostics
    );
}

#[test]
#[ignore = "release-mode recovery work scaling"]
fn recovery_work_stays_linear_on_large_files_with_dense_errors() {
    let language = &nitride_cli::csharp4::LANGUAGE;
    let build = |bytes: usize| {
        let mut source = String::new();
        let mut index = 0;
        while source.len() < bytes {
            source.push_str(if index % 17 == 0 {
                "class C { int M() { return 1 } }\n"
            } else {
                "class C { int M() { return 1; } }\n"
            });
            index += 1;
        }
        source
    };
    let (small, large) = (build(256 * 1024), build(4 * 1024 * 1024));
    let a = recover(language, &small);
    let b = recover(language, &large);
    assert_eq!(a.stats.fallback_count, 0);
    assert_eq!(b.stats.fallback_count, 0);
    assert!(
        (b.stats.work as f64 / large.len() as f64)
            <= 2.0 * (a.stats.work as f64 / small.len() as f64),
        "{:?} {:?}",
        a.stats,
        b.stats
    );
}

#[test]
fn failed_source_check_propagates_unknown_and_retains_independent_values() {
    let language = &nitride_cli::dates::LANGUAGE;
    let parsed = recover(language, "let x = 2026-02-30; x + 1 days; today;");
    let analysis = compile_recovering(language, &parsed, Limits::default());
    assert_eq!(analysis.diagnostics.len(), 1, "{:?}", analysis.diagnostics);
    assert_eq!(analysis.diagnostics[0].code, Some("DATE0001"));
    let program = analysis.program.unwrap();
    for s in &program.statements[..2] {
        assert_eq!(program.nodes[s.value as usize].ty, ERROR_TYPE);
    }
    assert_ne!(
        program.nodes[program.statements[2].value as usize].ty,
        ERROR_TYPE
    );
}

#[test]
fn missing_condition_closer_does_not_reinterpret_earlier_declarations() {
    let source = "class C { static int Main() { var v = new { Foo = \"Bar\" }; if (v.Foo != \"Bar\" return 1; return 0; } }";
    let parsed = recover(&nitride_cli::csharp4::LANGUAGE, source);
    assert_eq!(parsed.diagnostics.len(), 1, "{:?}", parsed.diagnostics);
    assert_eq!(parsed.stats.fallback_count, 0);
    assert_eq!(
        parsed.tree.recovery().unwrap().repairs,
        vec![Repair::Missing {
            expected: Expected::Literal(")"),
            at: source.find("return 1").unwrap() as u32,
        }]
    );
}

#[test]
fn damaged_template_parameters_poison_dependents_and_keep_healthy_calls() {
    let language = &nitride_cli::functions::LANGUAGE;
    for annotation in ["", "Unknown"] {
        let source = format!(
            "def bad(x: {annotation}) = add(x, 1); def good(x: Scalar) = add(x, 2); call bad(3); call good(4);"
        );
        let parsed = recover(language, &source);
        let analysis = compile_recovering(language, &parsed, Limits::default());
        assert_eq!(
            analysis.diagnostics.len(),
            1,
            "{annotation}: {:?}",
            analysis.diagnostics
        );
        let program = analysis.program.expect("partial template HIR");
        assert_eq!(program.statements.len(), 2);
        assert_eq!(
            program.nodes[program.statements[0].value as usize].ty,
            ERROR_TYPE
        );
        assert_eq!(
            program.nodes[program.statements[1].value as usize].ty,
            "Core.Scalar"
        );
    }
}

#[test]
fn missing_template_names_do_not_create_bindings() {
    let language = &nitride_cli::functions::LANGUAGE;
    for source in [
        "def (x: Scalar) = x; call ghost(1); 2;",
        "def f(: Scalar) = x; call f(1); 2;",
    ] {
        let parsed = recover(language, source);
        let analysis = compile_recovering(language, &parsed, Limits::default());
        let program = analysis.program.expect("partial template HIR");
        assert!(!analysis.diagnostics.is_empty());
        let missing = &parsed.tree.recovery().unwrap().damaged_nodes;
        assert!(program.bindings.iter().all(|b| {
            !missing
                .iter()
                .any(|id| parsed.tree.nodes[*id as usize].span[0] == b.symbol)
        }));
        assert_eq!(
            program.nodes[program.statements.last().unwrap().value as usize].ty,
            "Core.Scalar"
        );
    }
}

#[test]
fn truncated_query_keeps_every_completed_earlier_member_and_statement() {
    let case: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/recovery/truncated-query.json"
    ))
    .unwrap();
    let language = &nitride_cli::csharp4::LANGUAGE;
    let original = case["original"].as_str().unwrap();
    let source = case["damaged"].as_str().unwrap();
    let before = parse(language, original, Limits::default()).unwrap();
    let after = recover(language, source);
    for (id, n) in before.nodes.iter().enumerate().filter(|(_, n)| {
        let name = language.rules[n.rule as usize].name;
        n.span[1] as usize <= source.len()
            && (name.ends_with(".MemberDeclaration") || name.ends_with(".StandardStatement"))
    }) {
        let found = after
            .tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, a)| a.rule == n.rule && a.span == n.span);
        assert!(
            found.is_some(),
            "lost {} {:?}: {:?}",
            language.rules[n.rule as usize].name,
            n.span,
            after.diagnostics
        );
        assert_eq!(
            shape(&before, id as u32),
            shape(&after.tree, found.unwrap().0 as u32)
        );
    }
}

#[test]
fn broken_optional_else_reopens_its_strict_prefix() {
    let source = "namespace N { class C { int F(int x) { if (x > 0) { return 1; } else { for (int i = 0; i < x; i++) { x += i; } return x } } int G() { return 2; } } }";
    let parsed = recover(&nitride_cli::csharp4::LANGUAGE, source);
    assert_eq!(
        parsed.tree.recovery().unwrap().repairs,
        vec![Repair::Missing {
            expected: Expected::Literal(";"),
            at: source.find("x }").unwrap() as u32 + 2,
        }],
        "{:?}",
        parsed.diagnostics
    );
}

#[test]
fn deep_damage_on_a_small_thread_retains_every_byte() {
    std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(|| {
            let source = format!(
                "class C {{ void M() {{ {}return 1{} }} }}",
                "{".repeat(1000),
                "}".repeat(1000)
            );
            let parsed = recover(&nitride_cli::csharp4::LANGUAGE, &source);
            assert!(!parsed.diagnostics.is_empty());
            let mut at = 0;
            for part in &parsed.tree.recovery().unwrap().coverage {
                assert_eq!(part.span[0], at);
                assert!(
                    source
                        .get(part.span[0] as usize..part.span[1] as usize)
                        .is_some()
                );
                at = part.span[1];
            }
            assert_eq!(at as usize, source.len());
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn junk_between_nested_closers_preserves_the_inner_statement() {
    let source = format!(
        "class C {{ void M() {{ {}M(); {}@ {} }} }}",
        "{ ".repeat(64),
        "} ".repeat(48),
        "} ".repeat(16)
    );
    let parsed = recover(&nitride_cli::csharp4::LANGUAGE, &source);
    assert_eq!(parsed.stats.fallback_count, 0, "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.tree.recovery().unwrap().repairs,
        vec![Repair::Skipped {
            span: [
                source.find('@').unwrap() as u32,
                source.find('@').unwrap() as u32 + 1
            ],
        }]
    );
    assert!(
        parsed
            .tree
            .nodes
            .iter()
            .any(|n| source[n.span[0] as usize..n.span[1] as usize] == *"M();")
    );
}

#[test]
fn unresolved_call_reports_only_the_primary_binding_error() {
    let language = &nitride_cli::calls::LANGUAGE;
    let parsed = recover(language, "let x = ; absent(1); neg(2);");
    let analysis = compile_recovering(language, &parsed, Limits::default());
    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .map(|d| d.category)
            .collect::<Vec<_>>(),
        ["syntax", "binding"],
        "{:?}",
        analysis.diagnostics
    );
    let program = analysis.program.unwrap();
    assert_eq!(
        program.nodes[program.statements[1].value as usize].ty,
        ERROR_TYPE
    );
    assert_eq!(
        program.nodes[program.statements[2].value as usize].ty,
        "Core.Scalar"
    );
}

#[test]
fn frozen_reviewed_errors_are_local_complete_and_deterministic() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/recovery/adversarial.json")).unwrap();
    for case in manifest["variants"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["group"] == "reviewed")
    {
        let profile = case["baseline"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let language = nitride_cli::recovery_language(profile).unwrap();
        let mut first = nitride_cli::recovery_observe(language, source);
        let mut second = nitride_cli::recovery_observe(language, source);
        first.as_object_mut().unwrap().remove("parse_ns");
        second.as_object_mut().unwrap().remove("parse_ns");
        assert_eq!(first, second, "{profile}");
        assert_eq!(
            first["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|d| d["category"] == "syntax")
                .count(),
            1,
            "{profile}: {}",
            first["diagnostics"]
        );
        let mut at = 0;
        for part in first["coverage"].as_array().unwrap() {
            let start = part["span"][0].as_u64().unwrap() as usize;
            let end = part["span"][1].as_u64().unwrap() as usize;
            assert_eq!(start, at);
            assert!(source.get(start..end).is_some());
            at = end;
        }
        assert_eq!(at, source.len(), "{profile}");
        let nodes = first["nodes"].as_array().unwrap();
        let mut seen = std::collections::HashSet::new();
        let mut pending = vec![first["root"].as_u64().unwrap() as usize];
        while let Some(id) = pending.pop() {
            assert!(seen.insert(id), "shared node in {profile}");
            pending.extend(
                nodes[id]["children"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| e["node"].as_u64().unwrap() as usize),
            );
        }
        assert_eq!(seen.len(), nodes.len(), "detached nodes in {profile}");
    }
}

#[test]
fn damaged_shadowing_preserves_namespace_and_initializer_visibility() {
    let source = "let x = 1; let x = value(x) + ; value(x); 2;";
    let language = &nitride_cli::inference::LANGUAGE;
    let parsed = recover(language, source);
    let analysis = compile_recovering(language, &parsed, Limits::default());
    assert_eq!(analysis.diagnostics.len(), 1, "{:?}", analysis.diagnostics);
    let program = analysis.program.unwrap();
    let symbols: Vec<_> = program.bindings.iter().map(|b| b.symbol).collect();
    assert_eq!(
        symbols,
        [
            source.find('x').unwrap() as u32,
            source.find("x = value").unwrap() as u32
        ]
    );
    assert_eq!(
        program.nodes[program.statements[0].value as usize].ty,
        "Core.Scalar"
    );
    assert_eq!(
        program.nodes[program.statements[1].value as usize].ty,
        ERROR_TYPE
    );
    assert_eq!(
        program.nodes[program.statements[2].value as usize].ty,
        ERROR_TYPE
    );
    assert_eq!(
        program.nodes[program.statements[3].value as usize].ty,
        "Core.Scalar"
    );

    let source =
        "let x : Scalar = 1; rate x : Scalar = 2; let x : Scalar = ; either(x); reverse(x);";
    let language = &nitride_cli::namespaces::LANGUAGE;
    let parsed = recover(language, source);
    let analysis = compile_recovering(language, &parsed, Limits::default());
    assert_eq!(analysis.diagnostics.len(), 1, "{:?}", analysis.diagnostics);
    let program = analysis.program.unwrap();
    assert_eq!(
        program.nodes[program.statements[3].value as usize].ty,
        ERROR_TYPE
    );
    assert_eq!(
        program.nodes[program.statements[4].value as usize].ty,
        "Core.Scalar"
    );
}
