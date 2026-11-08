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
