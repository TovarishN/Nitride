//! The real `nitride lsp` binary over stdio, against this repository's nitride.json.
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

fn frame(message: Value) -> Vec<u8> {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

fn unframe(mut bytes: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
        let header = std::str::from_utf8(&bytes[..end]).unwrap();
        let length: usize = header
            .trim_start_matches("Content-Length: ")
            .trim()
            .parse()
            .unwrap();
        messages.push(serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap());
        bytes = &bytes[end + 4 + length..];
    }
    messages
}

#[test]
fn binary_session() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root_uri = format!("file://{}", root.display());
    let sample = root.join("languages/power/sample.powercalc");
    let uri = format!("file://{}", sample.display());
    let text = std::fs::read_to_string(&sample).unwrap();
    let document = json!({"textDocument":{"uri":uri}});
    let mut input = Vec::new();
    for message in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":root_uri,"capabilities":{},"workspaceFolders":[{"uri":root_uri,"name":"nitride"}]}}),
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"","version":1,"text":text}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/semanticTokens/full","params":document}),
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/documentSymbol","params":document}),
        json!({"jsonrpc":"2.0","id":5,"method":"textDocument/hover","params":{"textDocument":{"uri":uri},"position":{"line":1,"character":0}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"textDocument/definition","params":{"textDocument":{"uri":uri},"position":{"line":1,"character":0}}}),
        json!({"jsonrpc":"2.0","id":7,"method":"textDocument/completion","params":{"textDocument":{"uri":uri},"position":{"line":1,"character":0}}}),
        json!({"jsonrpc":"2.0","id":8,"method":"textDocument/inlayHint","params":{"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":9,"character":0}}}}),
        json!({"jsonrpc":"2.0","id":9,"method":"textDocument/formatting","params":{"textDocument":{"uri":uri},"options":{"tabSize":2,"insertSpaces":true}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"shutdown"}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        input.extend(frame(message));
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_nitride"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    assert!(child.wait().unwrap().success());
    let messages = unframe(&output);
    let published = messages
        .iter()
        .find(|m| m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri)
        .expect("diagnostics for the sample");
    assert_eq!(published["params"]["diagnostics"], json!([]));
    let tokens = messages.iter().find(|m| m["id"] == 2).unwrap();
    assert!(!tokens["result"]["data"].as_array().unwrap().is_empty());
    let symbols = messages.iter().find(|m| m["id"] == 3).unwrap();
    assert_eq!(symbols["result"][0]["name"], "x");
    let hover = messages.iter().find(|m| m["id"] == 5).unwrap();
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("Scalar"),
        "{hover}"
    );
    let definition = messages.iter().find(|m| m["id"] == 6).unwrap();
    assert_eq!(
        definition["result"]["range"]["start"],
        json!({"line":0,"character":4})
    );
    let completion = messages.iter().find(|m| m["id"] == 7).unwrap();
    let labels: Vec<_> = completion["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].clone())
        .collect();
    assert!(labels.contains(&json!("let")), "{labels:?}");
    let hints = messages.iter().find(|m| m["id"] == 8).unwrap();
    assert_eq!(hints["result"][0]["label"], "= 512");
    let formatting = messages.iter().find(|m| m["id"] == 9).unwrap();
    assert!(formatting["result"].is_array());
}
