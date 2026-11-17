mod support;
use serde_json::{Value, json};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;
use support::*;

// Slow responses keep unrelated input queued past the debounce interval without
// relying on an exact latency assertion or a socket/interactive client.
struct SlowOutput<F> {
    bytes: Vec<u8>,
    pending: Vec<u8>,
    on_flush: F,
}
impl<F: FnMut(&Value)> Write for SlowOutput<F> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        self.pending.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        for message in unframe(&std::mem::take(&mut self.pending)) {
            (self.on_flush)(&message);
        }
        std::thread::sleep(Duration::from_millis(3));
        Ok(())
    }
}
fn workspace(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = scratch(name);
    let grammar = root.join("Simple.ngr");
    std::fs::write(&grammar, "syntax module Simple { syntax P = \"ok\"; }").unwrap();
    manifest(
        &root,
        "nitride.nproj",
        "Simple",
        ".simple",
        std::slice::from_ref(&grammar),
        "Simple.P",
        None,
    );
    let path = root.join("x.simple");
    (root, grammar, path)
}
fn run(root: &std::path::Path, messages: Vec<Value>, on_flush: impl FnMut(&Value)) -> Vec<Value> {
    run_with(root, messages, Duration::from_millis(20), on_flush)
}
fn run_with(
    root: &std::path::Path,
    messages: Vec<Value>,
    debounce: Duration,
    on_flush: impl FnMut(&Value),
) -> Vec<Value> {
    let mut input = frame(&initialize(1, root));
    input.extend(frame(&notification("initialized", json!({}))));
    for message in messages {
        input.extend(frame(&message));
    }
    input.extend(frame(&request(9999, "shutdown", Value::Null)));
    input.extend(frame(&notification("exit", Value::Null)));
    let mut output = SlowOutput {
        bytes: vec![],
        pending: vec![],
        on_flush,
    };
    nitride_lsp::serve(
        std::io::Cursor::new(input),
        &mut output,
        nitride_lsp::ServeOptions { debounce },
    )
    .unwrap();
    unframe(&output.bytes)
}
fn traffic() -> Vec<Value> {
    (100..180)
        .map(|id| request(id, "nitride/unrelated", json!({})))
        .collect()
}
fn assert_error_before_traffic_ends(messages: &[Value], path: &std::path::Path) {
    let target = uri(path);
    let published = messages
        .iter()
        .position(|m| {
            m["method"] == "textDocument/publishDiagnostics"
                && m["params"]["uri"] == target
                && m["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(|d| !d.is_empty())
        })
        .expect("program error diagnostics");
    let last_request = messages.iter().position(|m| m["id"] == 179).unwrap();
    assert!(
        published < last_request,
        "diagnostics waited until unrelated traffic ended"
    );
}
#[test]
fn unrelated_messages_do_not_postpone_pending_diagnostics() {
    let (root, _, path) = workspace("debounce-unrelated");
    let mut input = vec![open(&path, "wrong")];
    input.extend(traffic());
    let messages = run(&root, input, |_| {});
    assert_error_before_traffic_ends(&messages, &path);
}
#[test]
fn watched_language_change_runs_without_a_pending_document() {
    let (root, grammar, path) = workspace("debounce-watch");
    let mut input = vec![
        open(&path, "ok"),
        request(
            2,
            "textDocument/documentSymbol",
            json!({"textDocument":{"uri":uri(&path)}}),
        ),
        notification(
            "workspace/didChangeWatchedFiles",
            json!({"changes":[{"uri":uri(&grammar),"type":2}]}),
        ),
    ];
    input.extend(traffic());
    let messages = run(&root, input, |message| {
        if message["id"] == 2 {
            std::fs::write(&grammar, "syntax module Simple { syntax P = \"changed\"; }").unwrap();
        }
    });
    assert_error_before_traffic_ends(&messages, &path);
}

#[test]
fn closing_a_pending_document_cancels_its_diagnostics() {
    let (root, _, path) = workspace("debounce-close");
    let mut input = vec![
        open(&path, "wrong"),
        notification(
            "textDocument/didClose",
            json!({"textDocument":{"uri":uri(&path)}}),
        ),
    ];
    input.extend(traffic());
    let messages = run_with(&root, input, Duration::from_secs(5), |_| {});
    let target = uri(&path);
    assert!(
        !messages
            .iter()
            .any(|m| m["method"] == "textDocument/publishDiagnostics"
                && m["params"]["uri"] == target)
    );
}

fn xml_rule_workspace(name: &str) -> (PathBuf, PathBuf) {
    let root = scratch(name);
    manifest(
        &root,
        "nitride.nproj",
        "Xml",
        ".xml",
        &[repo().join("languages/xml/Xml.ngr")],
        "Xml.Document",
        None,
    );
    let nested = root.join("nested");
    std::fs::create_dir(&nested).unwrap();
    (root, nested.join("sample.xml"))
}

#[test]
fn background_worker_reports_rules_for_a_file_that_was_never_opened() {
    let (root, path) = xml_rule_workspace("closed-rule-worker");
    std::fs::write(&path, "<root><a></b></root>").unwrap();
    let messages = run(&root, traffic(), |_| {});
    assert_error_before_traffic_ends(&messages, &path);
    assert!(
        diagnostics(&messages, &path)
            .as_array()
            .unwrap()
            .iter()
            .any(|problem| problem["code"] == "XML0001" && problem["severity"] == 1)
    );
}

#[test]
fn adding_and_removing_a_rule_updates_unopened_files() {
    let (root, grammar, path) = workspace("closed-rule-reload");
    let plain = r#"syntax module Simple { syntax P = "o" "k"; }"#;
    let checked = r#"syntax module Simple {
        token Letter = ['a'..'z'];
        syntax P = First:Letter Second:Letter {
            check SIMPLE001 First.Text == Second.Text : $"expected '{First.Text}'" at Second;
        }
    }"#;
    std::fs::write(&path, "ok").unwrap();
    let changed = notification(
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":uri(&grammar),"type":2}]}),
    );
    let messages = run_with(
        &root,
        vec![marker(100), changed.clone(), marker(101), changed],
        Duration::ZERO,
        |message| match message["id"].as_u64() {
            Some(100) => std::fs::write(&grammar, checked).unwrap(),
            Some(101) => std::fs::write(&grammar, plain).unwrap(),
            _ => (),
        },
    );
    let published = all_diagnostics(&messages, &path);
    assert_eq!(published.len(), 2, "{published:?}");
    assert_eq!(published[0][0]["code"], "SIMPLE001");
    assert_eq!(published[1], json!([]));
}

#[test]
fn closed_rule_diagnostics_follow_saved_file_lifecycle() {
    let (root, path) = xml_rule_workspace("closed-rule-lifecycle");
    let bad = "<credentials><password>example</password></credentials>";
    let clean = "<credentials/>";
    std::fs::write(&path, bad).unwrap();
    let watched = |kind| {
        notification(
            "workspace/didChangeWatchedFiles",
            json!({"changes":[{"uri":uri(&path),"type":kind}]}),
        )
    };
    let messages = run_with(
        &root,
        vec![
            open(&path, clean),
            notification(
                "textDocument/didClose",
                json!({"textDocument":{"uri":uri(&path)}}),
            ),
            marker(100),
            watched(2),
            marker(101),
            watched(2),
            marker(102),
            watched(3),
            marker(103),
            watched(1),
        ],
        Duration::ZERO,
        |message| match message["id"].as_u64() {
            Some(100) => std::fs::write(&path, clean).unwrap(),
            Some(101 | 103) => std::fs::write(&path, bad).unwrap(),
            Some(102) => std::fs::remove_file(&path).unwrap(),
            _ => (),
        },
    );
    let published = all_diagnostics(&messages, &path);
    assert_eq!(published.len(), 7, "{published:?}");
    for i in [0, 2, 4, 6] {
        assert_eq!(published[i][0]["code"], "XML1001");
        assert_eq!(published[i][0]["severity"], 2);
    }
    for i in [1, 3, 5] {
        assert_eq!(published[i], json!([]));
    }
}

#[test]
fn continuous_edits_to_one_document_do_not_postpone_another() {
    let (root, _, quiet) = workspace("debounce-independent-documents");
    let busy = root.join("busy.simple");
    let mut input = vec![open(&quiet, "wrong"), open(&busy, "wrong")];
    for version in 2..82 {
        input.push(change(&busy, version, "wrong"));
        input.push(request(
            (version + 98) as u64,
            "nitride/unrelated",
            json!({}),
        ));
    }
    let messages = run(&root, input, |_| {});
    assert_error_before_traffic_ends(&messages, &quiet);
}

#[test]
fn ordinary_json_edits_and_watches_do_not_reset_other_documents() {
    let (root, _, quiet) = workspace("debounce-json-data");
    let busy = root.join("data.json");
    std::fs::write(&busy, "{}").unwrap();
    let mut input = vec![open(&quiet, "wrong"), open(&busy, "{}")];
    for version in 2..82 {
        input.push(change(&busy, version, "{}"));
        input.push(notification(
            "workspace/didChangeWatchedFiles",
            json!({"changes":[{"uri":uri(&busy),"type":2}]}),
        ));
        input.push(request(
            (version + 98) as u64,
            "nitride/unrelated",
            json!({}),
        ));
    }
    let messages = run(&root, input, |_| {});
    assert_error_before_traffic_ends(&messages, &quiet);
}
