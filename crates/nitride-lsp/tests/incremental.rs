mod support;
use nitride_lsp::{analysis, document::Document, lines::LineIndex, registry::Registry};
use serde_json::json;
use support::*;

#[test]
fn edit_offsets_reject_surrogate_splits_and_out_of_range_positions() {
    let lines = LineIndex::new("a😀b\r\nx");
    assert_eq!(lines.checked_offset(0, 1), Some(1));
    assert_eq!(lines.checked_offset(0, 2), None);
    assert_eq!(lines.checked_offset(0, 3), Some(5));
    assert_eq!(lines.checked_offset(0, 4), Some(6));
    assert_eq!(lines.checked_offset(0, 5), None);
    assert_eq!(lines.checked_offset(1, 0), Some(8));
    assert_eq!(lines.checked_offset(2, 0), None);
}

#[test]
fn document_changes_are_ordered_transactional_and_versioned() {
    let mut doc = Document {
        text: "a😀b\r\nx".into(),
        version: 1,
    };
    assert!(doc.apply(
        2,
        &json!([
            {"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"text":"é"},
            {"range":{"start":{"line":0,"character":2},"end":{"line":1,"character":1}},"text":"!"}
        ])
    ));
    assert_eq!(doc.text, "aé!");
    assert_eq!(doc.version, 2);
    assert!(!doc.apply(
        3,
        &json!([
            {"text":"😀"},
            {"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":2}},"text":"bad"}
        ])
    ));
    assert_eq!(doc.text, "aé!");
    assert_eq!(doc.version, 2);
    assert!(!doc.apply(2, &json!([{"text":"stale"}])));
    assert!(doc.apply(4, &json!([{"text":"whole"}])));
    assert_eq!(doc.text, "whole");
}

#[test]
fn incremental_analysis_matches_fresh_checks_and_reuses_untouched_syntax() {
    let dir = scratch("incremental-analysis");
    manifest(
        &dir,
        "nitride.nproj",
        "Portable",
        ".portable",
        &[repo().join("languages/portable/Portable.ngr")],
        "Portable.Program",
        Some(&repo().join("languages/portable/Portable.ncatalog")),
    );
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    let path = dir.join("sample.portable");
    let mut source = "1 + 2;\n".repeat(200);
    let mut previous = analysis::analyze(&registry, &path, &source, &|_| None).unwrap();
    assert!(previous.snapshot.is_some());
    for replacement in ["3", "", "4"] {
        let at = source.find("+").unwrap() + 2;
        let end = source[at..].find(';').unwrap() + at;
        source.replace_range(at..end, replacement);
        let next =
            analysis::analyze_incremental(&registry, &path, &source, &|_| None, Some(&previous))
                .unwrap();
        let fresh = analysis::analyze(&registry, &path, &source, &|_| None).unwrap();
        assert_eq!(next.problems, fresh.problems);
        assert_eq!(next.values, fresh.values);
        assert!(next.snapshot.as_ref().unwrap().stats().reused_subtrees > 0);
        assert_eq!(
            next.tree.as_ref().unwrap().nodes.len(),
            fresh.tree.as_ref().unwrap().nodes.len()
        );
        previous = next;
    }
}

#[test]
fn reload_invalidates_cached_language_without_invalidating_old_readers() {
    let dir = scratch("incremental-reload");
    let grammar = dir.join("Test.ngr");
    std::fs::write(&grammar, "syntax module Test { syntax Program = Items:Item*; syntax Item = Value:Number \";\"; token Number = ['0'..'9']+; }").unwrap();
    manifest(
        &dir,
        "nitride.nproj",
        "Test",
        ".test",
        std::slice::from_ref(&grammar),
        "Test.Program",
        None,
    );
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    let path = dir.join("sample.test");
    let previous = analysis::analyze(&registry, &path, "1;2;3;", &|_| None).unwrap();
    let old_owner = registry.languages()[0]
        .current
        .as_ref()
        .unwrap()
        .language
        .clone();
    let weak = std::sync::Arc::downgrade(&old_owner);
    drop(old_owner);
    std::fs::write(&grammar, "syntax module Test { syntax Program = Items:Item*; syntax Item = Value:Number \";\"; token Number = ['0'..'9']+; } // reloaded").unwrap();
    registry.reload(&[dir], &|_| None);
    let next =
        analysis::analyze_incremental(&registry, &path, "1;2;4;", &|_| None, Some(&previous))
            .unwrap();
    assert_eq!(next.snapshot.as_ref().unwrap().stats().reused_subtrees, 0);
    assert_eq!(
        previous
            .snapshot
            .as_ref()
            .unwrap()
            .with_tree(|tree| tree.source.to_string())
            .unwrap(),
        "1;2;3;"
    );
    drop(previous);
    assert!(weak.upgrade().is_none());
}

#[test]
fn wire_ranged_changes_update_hints_and_stale_changes_do_not_overwrite_them() {
    let dir = scratch("incremental-wire");
    manifest(
        &dir,
        "nitride.nproj",
        "Portable",
        ".portable",
        &[repo().join("languages/portable/Portable.ngr")],
        "Portable.Program",
        Some(&repo().join("languages/portable/Portable.ncatalog")),
    );
    let path = dir.join("sample.portable");
    let messages = Session::run(
        &dir,
        vec![
            open(&path, "1 + 2;"),
            notification(
                "textDocument/didChange",
                json!({"textDocument":{"uri":uri(&path),"version":2},"contentChanges":[{"range":{"start":{"line":0,"character":4},"end":{"line":0,"character":5}},"text":"3"}]}),
            ),
            change(&path, 1, "broken"),
            request(
                7,
                "textDocument/inlayHint",
                json!({"textDocument":{"uri":uri(&path)},"range":{"start":{"line":0,"character":0},"end":{"line":1,"character":0}}}),
            ),
        ],
    );
    assert_eq!(diagnostics(&messages, &path), json!([]));
    assert!(response(&messages, 7)["result"].to_string().contains("= 4"));
}

#[test]
fn trivia_policy_changes_reload_editor_languages_and_invalidate_snapshots() {
    let dir = scratch("incremental-trivia");
    manifest(
        &dir,
        "nitride.nproj",
        "Json",
        ".json",
        &[repo().join("languages/json/Json.ngr")],
        "Json.Document",
        None,
    );
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&dir), &|_| None);
    let path = dir.join("sample.json");
    let source = "[1, /* comment */ 2]";
    let previous = analysis::analyze(&registry, &path, source, &|_| None).unwrap();
    assert!(previous.problems.is_empty());
    let project = language_file(&dir, "Json");
    let text = std::fs::read_to_string(&project)
        .unwrap()
        .replace("start Json.Document;", "start Json.Document; trivia json;");
    std::fs::write(project, text).unwrap();
    registry.reload(&[dir], &|_| None);
    let next = analysis::analyze_incremental(&registry, &path, source, &|_| None, Some(&previous))
        .unwrap();
    assert!(!next.problems.is_empty());
    assert_eq!(next.snapshot.as_ref().unwrap().stats().reused_subtrees, 0);
}

#[test]
fn calendar_duration_edits_preserve_navigation_and_update_native_hints() {
    let dir = scratch("calendar-incremental");
    manifest(
        &dir,
        "nitride.nproj",
        "Calendar",
        ".calendar",
        &[repo().join("languages/calendar/Calendar.ngr")],
        "Calendar.Program",
        Some(&repo().join("languages/calendar/Calendar.ncatalog")),
    );
    let path = dir.join("sample.calendar");
    let messages = Session::run(
        &dir,
        vec![
            open(&path, "let sprint = 2 weeks; sprint in days;"),
            notification(
                "textDocument/didChange",
                json!({"textDocument":{"uri":uri(&path),"version":2},"contentChanges":[{"range":{"start":{"line":0,"character":13},"end":{"line":0,"character":14}},"text":"3"}]}),
            ),
            request(
                7,
                "textDocument/inlayHint",
                json!({"textDocument":{"uri":uri(&path)},"range":{"start":{"line":0,"character":0},"end":{"line":1,"character":0}}}),
            ),
            request(
                8,
                "textDocument/definition",
                json!({"textDocument":{"uri":uri(&path)},"position":{"line":0,"character":24}}),
            ),
        ],
    );
    assert_eq!(diagnostics(&messages, &path), json!([]));
    assert!(
        response(&messages, 7)["result"]
            .to_string()
            .contains("= 21")
    );
    assert!(
        response(&messages, 8)["result"]
            .to_string()
            .contains("range")
    );
}

#[test]
fn empty_changes_advance_version_and_keep_same_version_updates_stale() {
    let mut document = Document {
        text: "1;".into(),
        version: 1,
    };
    assert!(document.apply(2, &json!([])));
    assert_eq!(document.version, 2);
    assert_eq!(document.text, "1;");
    assert!(!document.apply(2, &json!([{"text":"stale"}])));
}
