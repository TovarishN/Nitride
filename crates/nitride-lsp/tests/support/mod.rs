//! Scripted in-process sessions against `nitride_lsp::serve`.
#![allow(dead_code)]
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub fn frame(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

/// Splits framed server output into messages.
pub fn unframe(mut bytes: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    while !bytes.is_empty() {
        let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let header = std::str::from_utf8(&bytes[..end]).unwrap();
        let length: usize = header
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let body = &bytes[end + 4..end + 4 + length];
        messages.push(serde_json::from_slice(body).unwrap());
        bytes = &bytes[end + 4 + length..];
    }
    messages
}

/// A `file:` URI as VS Code sends it: `file:///C:/x` on Windows, never the
/// `\\?\` verbatim form `canonicalize` returns there.
pub fn uri(path: &Path) -> String {
    let text = path.to_string_lossy();
    let path = text
        .strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .replace('\\', "/");
    let path = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };
    format!("file://{}", path.replace(' ', "%20"))
}

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .canonicalize()
        .unwrap()
}

/// A fresh, empty directory for one test.
pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nitride-lsp-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

pub fn initialize(id: u64, root: &Path) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{
        "processId":null,"rootUri":uri(root),"capabilities":{},
        "workspaceFolders":[{"uri":uri(root),"name":"root"}]}})
}

pub fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","method":method,"params":params})
}

pub fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

pub fn open(path: &Path, text: &str) -> Value {
    notification(
        "textDocument/didOpen",
        json!({"textDocument":{"uri":uri(path),"languageId":"","version":1,"text":text}}),
    )
}

pub fn change(path: &Path, version: i32, text: &str) -> Value {
    notification(
        "textDocument/didChange",
        json!({"textDocument":{"uri":uri(path),"version":version},"contentChanges":[{"text":text}]}),
    )
}

pub struct Session;

impl Session {
    /// Runs initialize, `messages`, shutdown and exit; returns every server message.
    pub fn run(root: &Path, messages: Vec<Value>) -> Vec<Value> {
        let mut input = frame(&initialize(1, root));
        input.extend(frame(&notification("initialized", json!({}))));
        for message in &messages {
            input.extend(frame(message));
        }
        input.extend(frame(&request(9999, "shutdown", Value::Null)));
        input.extend(frame(&notification("exit", Value::Null)));
        Self::raw(input)
    }

    pub fn raw(input: Vec<u8>) -> Vec<Value> {
        Self::raw_with(input, std::time::Duration::ZERO)
    }

    pub fn raw_with(input: Vec<u8>, debounce: std::time::Duration) -> Vec<Value> {
        let mut output = Vec::new();
        nitride_lsp::serve(
            std::io::Cursor::new(input),
            &mut output,
            nitride_lsp::ServeOptions { debounce },
        )
        .unwrap();
        unframe(&output)
    }
}

/// The response to request `id`.
pub fn response(messages: &[Value], id: u64) -> &Value {
    messages
        .iter()
        .find(|m| m["id"] == id && m.get("method").is_none())
        .unwrap()
}

/// The last diagnostics published for `path`.
pub fn diagnostics(messages: &[Value], path: &Path) -> Value {
    let uri = uri(path);
    messages
        .iter()
        .rfind(|m| m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri)
        .map(|m| m["params"]["diagnostics"].clone())
        .unwrap_or_else(|| panic!("no diagnostics for {uri}"))
}

/// Writes a workspace manifest listing one language from absolute paths.
pub fn manifest(
    dir: &Path,
    file: &str,
    name: &str,
    extension: &str,
    grammars: &[PathBuf],
    start: &str,
    catalog: Option<&Path>,
) {
    let mut language = json!({"name":name,"extensions":[extension],
        "grammars":grammars.iter().map(|g| g.to_string_lossy()).collect::<Vec<_>>(),"start":start});
    if let Some(catalog) = catalog {
        language["catalog"] = json!(catalog.to_string_lossy());
    }
    std::fs::write(dir.join(file), json!({"languages":[language]}).to_string()).unwrap();
}

/// Every diagnostics list published for `path`, in order.
pub fn all_diagnostics(messages: &[Value], path: &Path) -> Vec<Value> {
    let uri = uri(path);
    messages
        .iter()
        .filter(|m| m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri)
        .map(|m| m["params"]["diagnostics"].clone())
        .collect()
}

/// Messages after the response to marker request `id`.
pub fn after(messages: &[Value], id: u64) -> &[Value] {
    let at = messages.iter().position(|m| m["id"] == id).unwrap();
    &messages[at + 1..]
}

pub fn marker(id: u64) -> Value {
    request(id, "nitride/marker", json!({}))
}
