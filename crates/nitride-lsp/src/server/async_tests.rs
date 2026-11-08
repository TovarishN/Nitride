use super::*;
use std::cell::RefCell;
use std::io::{BufReader, Cursor, Read};
use std::sync::mpsc;

struct Input {
    receiver: mpsc::Receiver<Vec<u8>>,
    current: Cursor<Vec<u8>>,
}
impl Read for Input {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            let read = self.current.read(bytes)?;
            if read != 0 {
                return Ok(read);
            }
            match self.receiver.recv() {
                Ok(next) => self.current = Cursor::new(next),
                Err(_) => return Ok(0),
            }
        }
    }
}
struct Output {
    sender: mpsc::Sender<Value>,
    pending: Vec<u8>,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut bytes = Cursor::new(std::mem::take(&mut self.pending));
        while let Some(message) = rpc::read(&mut bytes)? {
            self.sender
                .send(message.unwrap())
                .map_err(|_| io::ErrorKind::BrokenPipe)?;
        }
        Ok(())
    }
}
struct Client {
    input: Option<mpsc::Sender<Vec<u8>>>,
    output: mpsc::Receiver<Value>,
    thread: Option<std::thread::JoinHandle<io::Result<()>>>,
    done: mpsc::Receiver<()>,
    seen: RefCell<Vec<Value>>,
}
impl Client {
    fn start(analyze: crate::background::Analyzer) -> Self {
        let (input, receiver) = mpsc::channel();
        let (sender, output) = mpsc::channel();
        let (finished, done) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let result = run_with_analyzer(
                BufReader::new(Input {
                    receiver,
                    current: Cursor::new(vec![]),
                }),
                Output {
                    sender,
                    pending: vec![],
                },
                ServeOptions {
                    debounce: Duration::from_millis(5),
                },
                analyze,
            );
            let _ = finished.send(());
            result
        });
        let client = Self {
            input: Some(input),
            output,
            thread: Some(thread),
            done,
            seen: RefCell::new(vec![]),
        };
        client.send(json!({"id":1,"method":"initialize","params":{}}));
        client.response(1);
        client.send(json!({"method":"initialized","params":{}}));
        client
    }
    fn send(&self, mut message: Value) {
        message["jsonrpc"] = json!("2.0");
        let body = message.to_string();
        self.input
            .as_ref()
            .unwrap()
            .send(format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes())
            .unwrap();
    }
    fn next(&self) -> Value {
        let message = self.output.recv_timeout(Duration::from_secs(5)).unwrap();
        self.seen.borrow_mut().push(message.clone());
        message
    }
    fn response(&self, id: u64) -> Value {
        self.response_value(json!(id))
    }
    fn response_value(&self, id: Value) -> Value {
        loop {
            let message = self.next();
            if message["id"] == id {
                return message;
            }
        }
    }
    fn open(&self, text: &str) {
        self.send(
            json!({"method":"textDocument/didOpen","params":{"textDocument":{
            "uri":"file:///background.nproj","version":1,"text":text}}}),
        );
    }
    fn change(&self, text: &str) {
        self.send(
            json!({"method":"textDocument/didChange","params":{"textDocument":{
            "uri":"file:///background.nproj","version":2},"contentChanges":[{"text":text}]}}),
        );
    }
    fn diagnostic(&self) -> Value {
        loop {
            let message = self.next();
            if message["method"] == "textDocument/publishDiagnostics"
                && message["params"]["uri"] == "file:///background.nproj"
            {
                return message;
            }
        }
    }
    fn exit(self) {
        self.send(json!({"method":"exit"}));
        self.finish();
    }
    fn finish(mut self) {
        self.input.take();
        self.done.recv_timeout(Duration::from_secs(5)).unwrap();
        self.thread.take().unwrap().join().unwrap().unwrap();
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if let Some(input) = self.input.take() {
            let body = r#"{"jsonrpc":"2.0","method":"exit"}"#;
            let _ =
                input.send(format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes());
        }
    }
}
fn gate() -> (
    crate::background::Analyzer,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
) {
    let (started, start) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut first = true;
    let analyze = Box::new(move |job: &crate::background::Job| {
        if first {
            first = false;
            started.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        job.analyze()
    });
    (analyze, start, release)
}
#[test]
fn unrelated_request_is_answered_while_analysis_is_blocked() {
    let (analyze, started, release) = gate();
    let client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    client.send(json!({"id":2,"method":"nitride/unrelated"}));
    assert_eq!(client.response(2)["error"]["code"], METHOD_NOT_FOUND);
    release.send(()).unwrap();
    assert!(
        client.diagnostic()["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.exit();
}
#[test]
fn newer_edit_rejects_in_flight_diagnostics() {
    let (analyze, started, release) = gate();
    let client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    client.change("workspace {");
    client.send(json!({"id":2,"method":"nitride/unrelated"}));
    client.response(2);
    release.send(()).unwrap();
    let diagnostic = client.diagnostic();
    assert_eq!(diagnostic["params"]["version"], 2);
    assert!(
        !diagnostic["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.exit();
}
#[test]
fn close_reopen_with_same_version_rejects_old_incarnation() {
    let (analyze, started, release) = gate();
    let client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    client.send(json!({"method":"textDocument/didClose","params":{"textDocument":{"uri":"file:///background.nproj"}}}));
    client.open("workspace {");
    client.send(json!({"id":2,"method":"nitride/unrelated"}));
    client.response(2);
    release.send(()).unwrap();
    let diagnostic = client.diagnostic();
    assert_eq!(diagnostic["params"]["version"], 1);
    assert!(
        !diagnostic["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.exit();
}

fn symbols(id: Value) -> Value {
    json!({"id":id,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"file:///background.nproj"}}})
}
#[test]
fn cancellation_distinguishes_numeric_and_string_ids_and_answers_once() {
    let (analyze, started, release) = gate();
    let client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    client.send(symbols(json!(2)));
    client.send(symbols(json!("2")));
    client.send(json!({"method":"$/cancelRequest","params":{"id":2}}));
    assert_eq!(client.response(2)["error"]["code"], -32800);
    client.send(json!({"id":3,"method":"nitride/unrelated"}));
    client.response(3);
    assert!(!client.seen.borrow().iter().any(|m| m["id"] == "2"));
    release.send(()).unwrap();
    assert!(client.response_value(json!("2"))["result"].is_array());
    client.send(json!({"id":4,"method":"nitride/unrelated"}));
    client.response(4);
    assert_eq!(
        client.seen.borrow().iter().filter(|m| m["id"] == 2).count(),
        1
    );
    client.exit();
}
#[test]
fn edited_deferred_request_is_modified_and_next_request_gets_fresh_results() {
    let (analyze, started, release) = gate();
    let client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    client.send(symbols(json!(2)));
    client.change("workspace {");
    assert_eq!(client.response(2)["error"]["code"], -32801);
    client.send(symbols(json!(3)));
    release.send(()).unwrap();
    assert!(client.response(3).get("result").is_some());
    let seen = client.seen.borrow();
    assert!(
        seen.iter()
            .filter(|m| m["method"] == "textDocument/publishDiagnostics"
                && m["params"]["uri"] == "file:///background.nproj")
            .all(|m| m["params"]["version"] == 2)
    );
    drop(seen);
    client.exit();
}
#[test]
fn workspace_wait_is_invalidated_by_an_edit() {
    let (analyze, started, release) = gate();
    let client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    client.send(json!({"id":2,"method":"workspace/symbol","params":{"query":""}}));
    client.change("workspace {");
    assert_eq!(client.response(2)["error"]["code"], -32801);
    release.send(()).unwrap();
    client.exit();
}
#[test]
fn unserved_document_requests_settle_with_empty_results() {
    let client = Client::start(Box::new(crate::background::Job::analyze));
    client.send(json!({"method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///unknown.unserved","text":"anything","version":1}}}));
    client.send(json!({"id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"file:///unknown.unserved"}}}));
    assert_eq!(client.response(2)["result"], json!([]));
    client.exit();
}
#[test]
fn shutdown_drains_accepted_work_even_with_pipelined_exit() {
    let (analyze, started, release) = gate();
    let client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    client.send(symbols(json!(2)));
    client.send(json!({"id":9,"method":"shutdown"}));
    client.send(json!({"id":10,"method":"nitride/unrelated"}));
    assert_eq!(client.response(10)["error"]["code"], INVALID_REQUEST);
    assert!(!client.seen.borrow().iter().any(|m| m["id"] == 9));
    client.send(json!({"method":"exit"}));
    release.send(()).unwrap();
    assert_eq!(client.response(9)["result"], Value::Null);
    assert!(
        client
            .seen
            .borrow()
            .iter()
            .any(|m| m["id"] == 2 && m.get("result").is_some())
    );
    client.finish();
}
#[test]
fn eof_terminates_without_waiting_for_in_flight_work() {
    let (analyze, started, release) = gate();
    let mut client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    client.input.take();
    client.done.recv_timeout(Duration::from_secs(3)).unwrap();
    release.send(()).unwrap();
    client.thread.take().unwrap().join().unwrap().unwrap();
}
#[test]
fn deferred_request_limit_is_bounded() {
    let (analyze, started, release) = gate();
    let client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    for id in 1000..1257 {
        client.send(symbols(json!(id)));
    }
    assert_eq!(client.response(1256)["error"]["code"], -32802);
    release.send(()).unwrap();
    client.response(1255);
    assert_eq!(
        client
            .seen
            .borrow()
            .iter()
            .filter(|m| m.get("result").is_some()
                && m["id"]
                    .as_u64()
                    .is_some_and(|id| (1000..1256).contains(&id)))
            .count(),
        256
    );
    client.exit();
}
#[test]
#[cfg(panic = "unwind")]
fn failed_worker_completes_waiters_and_keeps_server_alive() {
    let client = Client::start(Box::new(|_| panic!("gated worker failure")));
    client.open("workspace {}");
    client.send(symbols(json!(2)));
    assert_eq!(client.response(2)["error"]["code"], -32603);
    client.send(json!({"id":3,"method":"nitride/unrelated"}));
    assert_eq!(client.response(3)["error"]["code"], METHOD_NOT_FOUND);
    client.exit();
}

#[test]
fn language_invalidation_rejects_old_work_before_replacement_finishes() {
    let (started, start) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut count = 0;
    let analyze = Box::new(move |job: &crate::background::Job| {
        count += 1;
        if count <= 2 {
            started.send(count).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        job.analyze()
    });
    let client = Client::start(analyze);
    client.open("workspace {}");
    assert_eq!(start.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
    client.send(symbols(json!(2)));
    client.send(
        json!({"method":"workspace/didChangeWorkspaceFolders","params":{"event":{
        "added":[{"uri":"file:///changed-workspace","name":"changed"}],"removed":[]}}}),
    );
    assert_eq!(client.response(2)["error"]["code"], -32801);
    release.send(()).unwrap();
    assert_eq!(start.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
    client.send(json!({"id":3,"method":"nitride/unrelated"}));
    client.response(3);
    assert!(
        !client
            .seen
            .borrow()
            .iter()
            .any(|m| m["method"] == "textDocument/publishDiagnostics"
                && m["params"]["uri"] == "file:///background.nproj")
    );
    release.send(()).unwrap();
    assert_eq!(client.diagnostic()["params"]["version"], 1);
    client.exit();
}
#[test]
fn stale_and_invalid_edits_leave_in_flight_work_and_waiters_valid() {
    let (analyze, started, release) = gate();
    let client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    client.send(symbols(json!(2)));
    client.send(
        json!({"method":"textDocument/didChange","params":{"textDocument":{
        "uri":"file:///background.nproj","version":1},"contentChanges":[{"text":"stale"}]}}),
    );
    client.send(
        json!({"method":"textDocument/didChange","params":{"textDocument":{
        "uri":"file:///background.nproj","version":2},"contentChanges":[{"range":{
            "start":{"line":9,"character":0},"end":{"line":9,"character":1}},"text":"invalid"}]}}),
    );
    client.send(json!({"id":3,"method":"nitride/unrelated"}));
    client.response(3);
    assert!(!client.seen.borrow().iter().any(|m| m["id"] == 2));
    release.send(()).unwrap();
    assert!(client.response(2)["result"].is_array());
    assert!(
        client
            .seen
            .borrow()
            .iter()
            .filter(|m| m["method"] == "textDocument/publishDiagnostics"
                && m["params"]["uri"] == "file:///background.nproj")
            .all(|m| m["params"]["version"] == 1)
    );
    client.exit();
}
#[test]
fn workspace_symbols_wait_for_all_current_documents() {
    let (analyze, started, release) = gate();
    let client = Client::start(analyze);
    client.open("workspace {}");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    client.send(
        json!({"method":"textDocument/didOpen","params":{"textDocument":{
        "uri":"file:///second.nproj","version":1,"text":"workspace {}"}}}),
    );
    client.send(json!({"id":2,"method":"workspace/symbol","params":{"query":""}}));
    release.send(()).unwrap();
    assert!(client.response(2)["result"].is_array());
    for uri in ["file:///background.nproj", "file:///second.nproj"] {
        assert!(
            client
                .seen
                .borrow()
                .iter()
                .any(|m| m["method"] == "textDocument/publishDiagnostics"
                    && m["params"]["uri"] == uri)
        );
    }
    client.exit();
}

#[test]
fn unsupported_uri_request_settles_without_worker_or_extra_input() {
    let client = Client::start(Box::new(crate::background::Job::analyze));
    client.send(
        json!({"method":"textDocument/didOpen","params":{"textDocument":{
        "uri":"untitled:Buffer","version":1,"text":"anything"}}}),
    );
    client.send(json!({"id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"untitled:Buffer"}}}));
    assert_eq!(client.response(2)["result"], json!([]));
    client.exit();
}
#[test]
fn unsupported_uri_does_not_strand_other_documents_or_shutdown() {
    let client = Client::start(Box::new(crate::background::Job::analyze));
    client.send(
        json!({"method":"textDocument/didOpen","params":{"textDocument":{
        "uri":"data:text/plain,virtual","version":1,"text":"anything"}}}),
    );
    client.open("workspace {}");
    client.send(json!({"id":2,"method":"workspace/symbol","params":{"query":""}}));
    assert!(client.response(2)["result"].is_array());
    client.send(json!({"id":9,"method":"shutdown"}));
    assert_eq!(client.response(9)["result"], Value::Null);
    client.exit();
}

#[test]
fn unsupported_uris_in_due_batch_do_not_strand_valid_documents() {
    let (send, receive) = mpsc::channel();
    let mut server = Server::new(Vec::new(), Duration::from_millis(5));
    server.jobs.worker = Some(crate::background::Worker::spawn(
        move |done| {
            send.send(done).unwrap();
        },
        Box::new(crate::background::Job::analyze),
    ));
    for uri in ["data:a", "data:b", "data:c", "file:///background.nproj"] {
        server
            .notification(
                "textDocument/didOpen",
                json!({"textDocument":{
            "uri":uri,"version":1,"text":"workspace {}"}}),
            )
            .unwrap();
    }
    // This is the timeout path: no later request or completion exists to
    // accidentally give an incomplete pump another turn.
    server.flush().unwrap();
    let done = receive
        .recv_timeout(Duration::from_secs(3))
        .expect("valid document starts after skipped URIs");
    assert_eq!(done.uri, "file:///background.nproj");
    server.completed(done).unwrap();
    assert!(server.analyses.contains_key("file:///background.nproj"));
    assert!(server.pending.is_empty());
}

#[test]
fn target_request_does_not_start_another_documents_quiet_work() {
    let (send, receive) = mpsc::channel();
    let mut server = Server::new(Vec::new(), Duration::from_secs(60));
    server.jobs.worker = Some(crate::background::Worker::spawn(
        move |done| {
            let _ = send.send(done);
        },
        Box::new(crate::background::Job::analyze),
    ));
    for uri in ["file:///a.nproj", "file:///b.nproj"] {
        server
            .notification(
                "textDocument/didOpen",
                json!({"textDocument":{
            "uri":uri,"version":1,"text":"workspace {}"}}),
            )
            .unwrap();
    }
    server
        .handle(
            json!({"id":7,"method":"textDocument/documentSymbol","params":{
        "textDocument":{"uri":"file:///a.nproj"}}}),
        )
        .unwrap();
    let done = receive.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(done.uri, "file:///a.nproj");
    server.completed(done).unwrap();
    server.pump().unwrap();
    assert!(
        server.pending.contains("file:///b.nproj"),
        "a request for A started B before B's own quiet deadline"
    );
}

#[test]
fn registry_reload_refreshes_open_json_input_classification() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "nitride-json-input-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("Lang")).unwrap();
    std::fs::write(root.join("nitride.nproj"), "workspace { include Lang; }").unwrap();
    std::fs::write(
        root.join("Test.ngr"),
        "syntax module Test { syntax P = \"ok\"; }",
    )
    .unwrap();
    std::fs::write(root.join("Lang/Lang.nproj"),"language Test { extensions .test; grammar ../Test.ngr; catalog ../types.json; start Test.P; }").unwrap();
    let json = root.join("types.json");
    std::fs::write(&json, "{broken").unwrap();
    let uri = uri::from_path(&json);
    let mut server = Server::new(Vec::new(), Duration::from_millis(100));
    server.folders.push(root.clone());
    server
        .notification(
            "textDocument/didOpen",
            json!({"textDocument":{"uri":uri,"version":1,"text":"{broken"}}),
        )
        .unwrap();
    assert!(!server.input_documents.contains(&uri));
    server.reload_languages().unwrap();
    assert!(server.input_documents.contains(&uri));
    server.reload = false;
    server.notification("textDocument/didChange",json!({"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"{still broken"}]})).unwrap();
    assert!(server.reload);
    assert!(server.timing.reload.is_some());
    std::fs::remove_dir_all(root).unwrap();
    assert!(crate::registry::watched_change_matters(
        &server.registry,
        &json,
        3
    ));
}
