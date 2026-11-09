//! Direct snapshot-to-editor projection for syntax without semantic metadata.
use super::*;

fn eligible(loaded: &Loaded) -> bool {
    loaded.language.language().profile == Profile::SyntaxOnly
        && loaded.language.language().rule_checks.is_empty()
        && loaded.prepared.declarations.is_empty()
        && loaded.prepared.references.is_empty()
}

pub(super) fn analyze(
    loaded: Arc<Loaded>,
    text: &str,
    snapshot: &nitride_codegen::RuntimeSnapshot,
    limits: Limits,
) -> Result<Analysis, Diagnostic> {
    check_cancellation([0, 0])?;
    if eligible(&loaded)
        && let Ok(result) = project(loaded.clone(), text, snapshot, limits)
    {
        return Ok(result);
    }
    check_cancellation([0, 0])?;
    let diagnostics = snapshot.diagnostics();
    snapshot.with_analyzed_timed(limits, |tree, checked, times| {
        let started = std::time::Instant::now();
        let mut result = program_analysis(loaded, text, tree, &diagnostics, checked);
        result.timings = AnalysisTimings {
            materialize_ns: times.materialize_ns,
            semantic_ns: times.semantic_ns,
            editor_ns: started.elapsed().as_nanos(),
            ..AnalysisTimings::default()
        };
        result
    })
}

fn project(
    loaded: Arc<Loaded>,
    text: &str,
    snapshot: &nitride_codegen::RuntimeSnapshot,
    limits: Limits,
) -> Result<Analysis, Diagnostic> {
    let started = std::time::Instant::now();
    let (tree, problems, repairs) = snapshot.with_snapshot(|view| {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let root = view.visit_postorder(limits.work, |_, node, children| {
            nodes.push(node);
            edges.extend_from_slice(children);
        })?;
        let mut problems = Vec::new();
        for diagnostic in view.diagnostics() {
            let problem = Problem {
                span: diagnostic.span,
                severity: match diagnostic.severity {
                    Severity::Error => 1,
                    Severity::Warning => 2,
                },
                message: diagnostic.message.clone(),
                code: diagnostic.code.as_deref().map(str::to_owned),
            };
            if !problems.contains(&problem) {
                problems.push(problem);
            }
        }
        let repairs = view
            .recovery()
            .map(|r| repair_fixes(r.repairs()))
            .unwrap_or_default();
        Ok::<_, Diagnostic>((
            Tree {
                grammar: Grammar::Workspace(loaded.clone()),
                nodes,
                edges,
                root,
            },
            problems,
            repairs,
        ))
    })?;
    let syntax_projection_ns = started.elapsed().as_nanos();
    let started = std::time::Instant::now();
    let mut result = empty(text);
    result.tree = Some(tree);
    result.problems = problems;
    result.repairs = repairs;
    result.roles = loaded.roles.clone();
    result.timings.syntax_projection_ns = syntax_projection_ns;
    result.timings.editor_ns = started.elapsed().as_nanos();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nitride_codegen::{LoadedLanguage, RuntimeSnapshot};

    fn loaded() -> Arc<Loaded> {
        let source = Source {
            path: "Projection.ngr".into(),
            text: r#"
syntax module Projection {
  token Word = ['a'..'z' 'é']+;
  syntax Item = "{" Value:Word ";" "}";
  syntax Root = Items:Item*;
}"#
            .into(),
        };
        let prepared = nitride_prepare::prepare_sources_detailed(
            &[source],
            EMPTY_CATALOG,
            "Projection.Root",
            "syntax",
        )
        .unwrap();
        let language = Arc::new(LoadedLanguage::load(&prepared.data.to_string()).unwrap());
        let roles = Arc::new(crate::roles::roles(language.language(), &|_| false));
        Arc::new(Loaded {
            language,
            prepared,
            components: vec![],
            roles,
        })
    }

    fn oracle(loaded: Arc<Loaded>, text: &str, snapshot: &RuntimeSnapshot) -> Analysis {
        let diagnostics = snapshot.diagnostics();
        snapshot
            .with_analyzed(Limits::default(), |tree, checked| {
                program_analysis(loaded, text, tree, &diagnostics, checked)
            })
            .unwrap()
    }

    fn equivalent(a: &Analysis, b: &Analysis) {
        let shape = |a: &Analysis| {
            a.tree
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .map(|n| (n.rule, n.span, n.first_child, n.child_count))
                .collect::<Vec<_>>()
        };
        assert_eq!(shape(a), shape(b));
        assert_eq!(
            a.tree.as_ref().unwrap().edges,
            b.tree.as_ref().unwrap().edges
        );
        assert_eq!(a.tree.as_ref().unwrap().root, b.tree.as_ref().unwrap().root);
        assert_eq!(a.problems, b.problems);
        assert_eq!(a.repairs, b.repairs);
        assert_eq!(a.declarations, b.declarations);
        assert_eq!(a.values, b.values);
        assert_eq!(a.index.definitions, b.index.definitions);
        assert_eq!(a.index.references, b.index.references);
        assert_eq!(a.index.builtins, b.index.builtins);
        assert_eq!(a.index.types, b.index.types);
        for palette in [false, true] {
            assert_eq!(
                crate::tokens::encode(a, palette),
                crate::tokens::encode(b, palette)
            );
        }
        assert_eq!(crate::structure::folds(a), crate::structure::folds(b));
        assert_eq!(crate::structure::symbols(a), crate::structure::symbols(b));
        let positions = [serde_json::json!({"line":1,"character":2})];
        assert_eq!(
            crate::structure::selections(a, &positions),
            crate::structure::selections(b, &positions)
        );
        assert_eq!(
            crate::format::edits(a, 0, 100, 2, true, true),
            crate::format::edits(b, 0, 100, 2, true, true)
        );
        assert_eq!(
            crate::completion::items(a, 0, None),
            crate::completion::items(b, 0, None)
        );
    }

    #[test]
    fn direct_projection_matches_materialized_editor_features_after_edits() {
        let loaded = loaded();
        assert!(eligible(&loaded));
        let mut snapshot = RuntimeSnapshot::parse(
            loaded.language.clone(),
            "{é;}\n{beta;}",
            Limits::default(),
            SnapshotMode::Recovering(RecoveryOptions::default()),
        )
        .unwrap();
        for source in [
            "{é;}\n{beta;}",
            "{é}\n{beta;}",
            "{é;@}\n{beta;}",
            "{é;}\n{gamma;}",
            "{é;}\n{gamma;}",
        ] {
            let before = snapshot.with_tree(|tree| tree.source.clone()).unwrap();
            if let Some(edit) = crate::document::difference(&before, source) {
                snapshot = snapshot.edit(&edit, Limits::default()).unwrap();
            }
            let result = analyze(loaded.clone(), source, &snapshot, Limits::default()).unwrap();
            assert!(result.timings.syntax_projection_ns > 0);
            assert_eq!(result.timings.materialize_ns, 0);
            equivalent(&result, &oracle(loaded.clone(), source, &snapshot));
        }
    }

    #[test]
    fn declarations_and_references_use_materialized_metadata_path() {
        for references in [false, true] {
            let mut loaded = loaded();
            let prepared = &mut Arc::get_mut(&mut loaded).unwrap().prepared;
            if references {
                prepared.references.push(nitride_prepare::ReferenceClause {
                    rule: "Projection.Item".into(),
                    field: "Value".into(),
                    kinds: vec!["value".into()],
                });
            } else {
                prepared.declarations.push(nitride_prepare::Declaration {
                    rule: "Projection.Item".into(),
                    field: "Value".into(),
                    kinds: vec!["value".into()],
                });
            }
            assert!(!eligible(&loaded));
            let text = "{alpha;}";
            let snapshot = RuntimeSnapshot::parse(
                loaded.language.clone(),
                text,
                Limits::default(),
                SnapshotMode::Recovering(RecoveryOptions::default()),
            )
            .unwrap();
            let result = analyze(loaded.clone(), text, &snapshot, Limits::default()).unwrap();
            assert_eq!(result.timings.syntax_projection_ns, 0);
            equivalent(&result, &oracle(loaded, text, &snapshot));
            if !references {
                assert_eq!(result.declarations[0].name, "alpha");
            }
        }
    }

    #[test]
    fn typed_and_structurally_checked_languages_keep_the_established_path() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut registry = Registry::default();
        registry.reload(&[root], &|_| None);
        for (name, text) in [("portable", "let x : Scalar = 1; x;"), ("xml", "<a></b>")] {
            let path = std::path::PathBuf::from(format!("sample.{name}"));
            let loaded = registry.for_path(&path).unwrap().current.clone().unwrap();
            assert!(!eligible(&loaded));
            let snapshot = RuntimeSnapshot::parse(
                loaded.language.clone(),
                text,
                Limits::default(),
                SnapshotMode::Recovering(RecoveryOptions::default()),
            )
            .unwrap();
            let result = analyze(loaded.clone(), text, &snapshot, Limits::default()).unwrap();
            assert_eq!(result.timings.syntax_projection_ns, 0);
            equivalent(&result, &oracle(loaded, text, &snapshot));
            if name == "xml" {
                assert!(
                    result
                        .problems
                        .iter()
                        .any(|p| p.code.as_deref() == Some("XML0001"))
                );
            }
        }
    }

    #[test]
    fn projection_budget_failure_uses_established_tree_without_partial_output() {
        let loaded = loaded();
        let source = "{alpha;}\n{beta;}";
        let snapshot = RuntimeSnapshot::parse(
            loaded.language.clone(),
            source,
            Limits::default(),
            SnapshotMode::Recovering(RecoveryOptions::default()),
        )
        .unwrap();
        let result = analyze(
            loaded.clone(),
            source,
            &snapshot,
            Limits {
                work: 7,
                ..Limits::default()
            },
        )
        .unwrap();
        assert_eq!(result.timings.syntax_projection_ns, 0);
        equivalent(&result, &oracle(loaded, source, &snapshot));
    }
}
