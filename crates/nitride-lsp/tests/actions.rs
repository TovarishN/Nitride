mod support;
use nitride_prepare::{Source, grammar_language, prepare_sources_tolerant};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use support::*;

fn lang(path: &str) -> PathBuf {
    repo().join("languages").join(path)
}

fn calls(name: &str) -> PathBuf {
    let dir = scratch(name);
    manifest(
        &dir,
        "nitride.json",
        "Calls",
        ".calls",
        &[lang("calls/Calls.ngr")],
        "Calls.Program",
        Some(&lang("calls/Calls.ncatalog")),
    );
    dir
}

fn whole() -> Value {
    json!({"start":{"line":0,"character":0},"end":{"line":999,"character":0}})
}

fn actions(
    root: &Path,
    path: &Path,
    text: &str,
    range: Value,
    only: Option<Vec<&str>>,
) -> Vec<Value> {
    let mut context = json!({"diagnostics": []});
    if let Some(only) = only {
        context["only"] = json!(only);
    }
    let messages = Session::run(
        root,
        vec![
            open(path, text),
            request(
                5,
                "textDocument/codeAction",
                json!({"textDocument":{"uri":uri(path)},"range":range,"context":context}),
            ),
        ],
    );
    response(&messages, 5)["result"].as_array().unwrap().clone()
}

fn edits(action: &Value, path: &Path) -> Value {
    action["edit"]["changes"][uri(path)].clone()
}

#[test]
fn insert_missing_semicolon() {
    let dir = calls("action-semicolon");
    let path = dir.join("x.calls");
    let found = actions(&dir, &path, "let a=1\nsum(a,a);", whole(), None);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["title"], "Insert `;`");
    assert_eq!(found[0]["kind"], "quickfix");
    assert_eq!(found[0]["isPreferred"], true);
    assert_eq!(
        edits(&found[0], &path),
        json!([{"range":{"start":{"line":0,"character":7},"end":{"line":0,"character":7}},"newText":";"}])
    );
}

#[test]
fn unhelpful_repair_not_offered() {
    let language = grammar_language();
    let broken = "syntax module M {";
    assert!(!nitride_lsp::actions::helps(language, broken, broken));
    assert!(nitride_lsp::actions::helps(
        language,
        broken,
        "syntax module M {}"
    ));
}

#[test]
fn remove_semantic_member() {
    let dir = scratch("action-member");
    let path = lang("calls/Calls.Reference.ngr");
    manifest(
        &dir,
        "nitrogen.json",
        "Calls",
        ".calls",
        std::slice::from_ref(&path),
        "Calls.Program",
        None,
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let line5 = json!({"start":{"line":4,"character":2},"end":{"line":4,"character":2}});
    let found = actions(&dir, &path, &text, line5, Some(vec!["quickfix"]));
    let remove = found
        .iter()
        .find(|a| a["title"] == "Remove C# semantic member")
        .expect("fix");
    assert_eq!(
        edits(remove, &path),
        json!([{"range":{"start":{"line":4,"character":0},"end":{"line":5,"character":0}},"newText":""}])
    );
}

#[test]
fn fix_all_semantic_members() {
    let dir = scratch("action-fix-all");
    let path = lang("calls/Calls.Reference.ngr");
    manifest(
        &dir,
        "nitrogen.json",
        "Calls",
        ".calls",
        std::slice::from_ref(&path),
        "Calls.Program",
        None,
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let found = actions(&dir, &path, &text, whole(), Some(vec!["source.fixAll"]));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["kind"], "source.fixAll");
    let lines = nitride_lsp::lines::LineIndex::new(&text);
    let mut spans: Vec<(u32, u32)> = edits(&found[0], &path)
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let at = |p: &Value| {
                lines.offset(
                    p["line"].as_u64().unwrap() as u32,
                    p["character"].as_u64().unwrap() as u32,
                )
            };
            (at(&e["range"]["start"]), at(&e["range"]["end"]))
        })
        .collect();
    spans.sort_by_key(|s| std::cmp::Reverse(s.0));
    let mut fixed = text.clone();
    for (start, end) in spans {
        fixed.replace_range(start as usize..end as usize, "");
    }
    let source = Source {
        path: "fixed.ngr".into(),
        text: fixed,
    };
    let prepared = prepare_sources_tolerant(
        &[source],
        r#"{"types":[],"operations":[],"statements":{}}"#,
        "Calls.Program",
    )
    .unwrap();
    assert_eq!(prepared.warnings.len(), 0, "{:?}", prepared.warnings);
}

#[test]
fn only_filters_kinds() {
    let dir = calls("action-only");
    let path = dir.join("x.calls");
    assert!(
        actions(
            &dir,
            &path,
            "let a=1\nsum(a,a);",
            whole(),
            Some(vec!["source.fixAll"])
        )
        .is_empty()
    );
    assert_eq!(
        actions(
            &dir,
            &path,
            "let a=1\nsum(a,a);",
            whole(),
            Some(vec!["quickfix"])
        )
        .len(),
        1
    );
}

/// A select-all code action in a file with many repairs stays bounded.
#[test]
fn many_repairs_are_capped() {
    let dir = calls("action-many");
    let path = dir.join("x.calls");
    let text: String = (0..800)
        .map(|i| format!("let a{i}=1\nsum(a{i},1);\n"))
        .collect();
    // The baseline opens and analyses the same file without asking for
    // actions, so the bound holds on slow machines too. Each offered repair
    // re-parses once (about ten); all 800 would be far past the bound.
    let started = std::time::Instant::now();
    let symbols = Session::run(
        &dir,
        vec![
            open(&path, &text),
            request(
                5,
                "textDocument/documentSymbol",
                json!({"textDocument":{"uri":uri(&path)}}),
            ),
        ],
    );
    assert!(response(&symbols, 5)["result"].is_array());
    let baseline = started.elapsed();
    let started = std::time::Instant::now();
    let found = actions(&dir, &path, &text, whole(), Some(vec!["quickfix"]));
    let elapsed = started.elapsed();
    assert!(!found.is_empty());
    assert!(found.len() <= 20, "{}", found.len());
    eprintln!(
        "found {} in {elapsed:?} (baseline {baseline:?})",
        found.len()
    );
    assert!(elapsed < baseline * 30, "{elapsed:?} against {baseline:?}");
}
