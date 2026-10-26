//! The message loop: document sync, debounced analysis and request dispatch.
use crate::analysis::{self, Analysis};
use crate::grammar_index::{self, GrammarIndex};
use crate::lines::LineIndex;
use crate::navigation::{self, GrammarAt};
use crate::registry::Registry;
use crate::{ServeOptions, manifest, rpc, structure, tokens, uri};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

enum Incoming {
    Message(Value),
    Malformed(String),
}

pub struct Document {
    pub text: String,
    pub version: i32,
}

pub struct Server<W: Write> {
    output: W,
    shutdown: bool,
    pub documents: HashMap<String, Document>,
    /// Documents whose analysis is out of date.
    pending: BTreeSet<String>,
    folders: Vec<PathBuf>,
    pub registry: Registry,
    /// Manifests, grammars or catalogs changed since languages were loaded.
    reload: bool,
    pub analyses: HashMap<String, Analysis>,
    /// Every workspace grammar, indexed on demand and dropped when languages
    /// or grammar buffers change.
    grammars: Option<GrammarIndex>,
}

pub fn run(
    input: impl BufRead + Send + 'static,
    output: impl Write,
    options: ServeOptions,
) -> io::Result<()> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut input = input;
        while let Ok(Some(message)) = rpc::read(&mut input) {
            let incoming = match message {
                Ok(value) => Incoming::Message(value),
                Err(error) => Incoming::Malformed(error),
            };
            if sender.send(incoming).is_err() {
                break;
            }
        }
    });
    let mut server = Server::new(output);
    loop {
        let incoming = if server.pending.is_empty() || options.debounce.is_zero() {
            match receiver.recv() {
                Ok(incoming) => incoming,
                Err(_) => return Ok(()),
            }
        } else {
            match receiver.recv_timeout(options.debounce) {
                Ok(incoming) => incoming,
                Err(RecvTimeoutError::Timeout) => {
                    server.flush()?;
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        };
        match incoming {
            Incoming::Message(message) => {
                if server.handle(message)? {
                    return Ok(());
                }
            }
            Incoming::Malformed(error) => server.error(Value::Null, PARSE_ERROR, &error)?,
        }
        if options.debounce.is_zero() {
            server.flush()?;
        }
    }
}

impl<W: Write> Server<W> {
    fn new(output: W) -> Self {
        Self {
            output,
            shutdown: false,
            documents: HashMap::new(),
            pending: BTreeSet::new(),
            folders: Vec::new(),
            registry: Registry::default(),
            reload: false,
            analyses: HashMap::new(),
            grammars: None,
        }
    }

    fn send(&mut self, message: Value) -> io::Result<()> {
        rpc::write(&mut self.output, &message)
    }

    fn error(&mut self, id: Value, code: i64, message: &str) -> io::Result<()> {
        self.send(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
    }

    /// Handles one message; true after `exit`.
    fn handle(&mut self, message: Value) -> io::Result<bool> {
        let Some(method) = message["method"].as_str().map(str::to_owned) else {
            return Ok(false); // A response to a request of ours.
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = message.get("id").cloned() else {
            return self.notification(&method, params);
        };
        if self.shutdown {
            return self
                .error(id, INVALID_REQUEST, "server is shutting down")
                .map(|_| false);
        }
        let result = match method.as_str() {
            "initialize" => Ok(self.initialize(&params)),
            "shutdown" => {
                self.flush()?;
                self.shutdown = true;
                Ok(Value::Null)
            }
            _ => self.request(&method, &params),
        };
        match result {
            Ok(result) => self.send(json!({"jsonrpc":"2.0","id":id,"result":result}))?,
            Err((code, message)) => self.error(id, code, &message)?,
        }
        Ok(false)
    }

    fn initialize(&mut self, params: &Value) -> Value {
        let folders = params["workspaceFolders"]
            .as_array()
            .map(|folders| folders.iter().filter_map(|f| f["uri"].as_str()).collect())
            .unwrap_or_else(|| params["rootUri"].as_str().into_iter().collect::<Vec<_>>());
        self.folders = folders.into_iter().filter_map(uri::to_path).collect();
        self.reload = true;
        json!({
            "capabilities": {
                "positionEncoding": "utf-16",
                "textDocumentSync": 1,
                "workspace": {"workspaceFolders": {"supported": true, "changeNotifications": true}},
                "semanticTokensProvider": {
                    "legend": {"tokenTypes": tokens::TYPES, "tokenModifiers": tokens::MODIFIERS},
                    "full": true,
                },
                "documentSymbolProvider": true,
                "foldingRangeProvider": true,
                "selectionRangeProvider": true,
                "hoverProvider": true,
                "definitionProvider": true,
                "declarationProvider": true,
                "referencesProvider": true,
                "documentHighlightProvider": true,
                "renameProvider": {"prepareProvider": true},
                "workspaceSymbolProvider": true,
                "inlayHintProvider": true,
                "completionProvider": {"triggerCharacters": ["."]},
                "signatureHelpProvider": {"triggerCharacters": ["(", ","]},
            },
            "serverInfo": {"name": "nitride", "version": env!("CARGO_PKG_VERSION")},
        })
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        let uri = params["textDocument"]["uri"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        match method {
            "textDocument/semanticTokens/full" => {
                let data = self.current(&uri).map(tokens::encode).unwrap_or_default();
                Ok(json!({ "data": data }))
            }
            "textDocument/documentSymbol" => {
                Ok(self.current(&uri).map_or(json!([]), structure::symbols))
            }
            "textDocument/foldingRange" => {
                Ok(self.current(&uri).map_or(json!([]), structure::folds))
            }
            "textDocument/selectionRange" => {
                let positions = params["positions"].as_array().cloned().unwrap_or_default();
                Ok(self
                    .current(&uri)
                    .map_or(json!([]), |a| structure::selections(a, &positions)))
            }
            "textDocument/hover"
            | "textDocument/definition"
            | "textDocument/declaration"
            | "textDocument/references"
            | "textDocument/documentHighlight"
            | "textDocument/prepareRename"
            | "textDocument/rename" => {
                let Some((offset, grammar)) = self.located(&uri, params) else {
                    return Ok(match method {
                        "textDocument/references" | "textDocument/documentHighlight" => json!([]),
                        _ => Value::Null,
                    });
                };
                let analysis = &self.analyses[&uri];
                let at = grammar
                    .as_ref()
                    .map(|(index, file)| GrammarAt { index, file: *file });
                Ok(match method {
                    "textDocument/hover" => navigation::hover(analysis, offset, at),
                    "textDocument/references" => {
                        let include = params["context"]["includeDeclaration"]
                            .as_bool()
                            .unwrap_or(false);
                        navigation::references(analysis, &uri, offset, at, include)
                    }
                    "textDocument/documentHighlight" => {
                        navigation::highlights(analysis, &uri, offset, at)
                    }
                    "textDocument/prepareRename" => {
                        navigation::prepare_rename(analysis, offset, at)?
                    }
                    "textDocument/rename" => {
                        let name = params["newName"].as_str().unwrap_or_default();
                        navigation::rename(analysis, &uri, offset, at, name)?
                    }
                    _ => navigation::definition(analysis, &uri, offset, at),
                })
            }
            "textDocument/completion" => {
                let Some((offset, grammar)) = self.located(&uri, params) else {
                    return Ok(json!([]));
                };
                let analysis = &self.analyses[&uri];
                let at = grammar
                    .as_ref()
                    .map(|(index, file)| GrammarAt { index, file: *file });
                Ok(crate::completion::items(analysis, offset, at))
            }
            "textDocument/signatureHelp" => {
                let Some((offset, _)) = self.located(&uri, params) else {
                    return Ok(Value::Null);
                };
                let analysis = &self.analyses[&uri];
                let Some(tree) = &analysis.tree else {
                    return Ok(Value::Null);
                };
                let mut leaves: Vec<[u32; 2]> = tree
                    .nodes
                    .iter()
                    .filter(|n| n.child_count == 0 && n.span[0] < n.span[1])
                    .map(|n| n.span)
                    .collect();
                leaves.sort_unstable();
                Ok(crate::signatures::help(
                    analysis.lines.text(),
                    offset,
                    tree.grammar.language(),
                    &leaves,
                ))
            }
            "textDocument/inlayHint" => {
                let Some(analysis) = self.current(&uri) else {
                    return Ok(json!([]));
                };
                let position = |key: &str| {
                    let p = &params["range"][key];
                    let line = u32::try_from(p["line"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
                    let character =
                        u32::try_from(p["character"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
                    analysis.lines.offset(line, character)
                };
                let (start, end) = (position("start"), position("end"));
                let hints: Vec<Value> = analysis
                    .values
                    .iter()
                    .filter(|(at, _)| start <= *at && *at <= end)
                    .map(|(at, label)| {
                        let (line, character) = analysis.lines.position(*at);
                        json!({"position":{"line":line,"character":character},"label":label,"paddingLeft":true})
                    })
                    .collect();
                Ok(Value::Array(hints))
            }
            "workspace/symbol" => {
                let query = params["query"].as_str().unwrap_or_default().to_owned();
                if std::mem::take(&mut self.reload) {
                    self.reload_languages()
                        .map_err(|e| (INVALID_REQUEST, e.to_string()))?;
                }
                for uri in std::mem::take(&mut self.pending) {
                    self.analyze(&uri)
                        .map_err(|e| (INVALID_REQUEST, e.to_string()))?;
                }
                if self.grammars.is_none() {
                    let mut paths: Vec<PathBuf> = Vec::new();
                    for language in self.registry.languages() {
                        for grammar in &language.entry.grammars {
                            if !paths.iter().any(|p| crate::registry::same_file(p, grammar)) {
                                paths.push(grammar.clone());
                            }
                        }
                    }
                    let buffers = self.buffers();
                    let uri_of = self.uri_of();
                    self.grammars = Some(grammar_index::build(
                        &paths,
                        &|p: &Path| buffers.get(p),
                        &uri_of,
                    ));
                }
                let grammars = self.grammars.as_ref().expect("just built");
                let open = self.analyses.iter().map(|(u, a)| (u.as_str(), a));
                Ok(navigation::workspace_symbols(grammars, open, &query))
            }
            _ => Err((METHOD_NOT_FOUND, "method not found".into())),
        }
    }

    /// The byte offset of the request's position in an up-to-date analysis,
    /// with the grammar index when the document is a grammar.
    fn located(
        &mut self,
        uri: &str,
        params: &Value,
    ) -> Option<(u32, Option<(GrammarIndex, usize)>)> {
        let line = params["position"]["line"].as_u64()? as u32;
        let character = params["position"]["character"].as_u64()? as u32;
        let analysis = self.current(uri)?;
        if line >= analysis.lines.line_count() {
            return None;
        }
        let offset = analysis.lines.offset(line, character);
        let path = uri::to_path(uri)?;
        let grammar = path.extension().is_some_and(|e| e == "ngr").then(|| {
            let buffers = self.buffers();
            let set = self.registry.grammar_set(&path);
            let uri_of = self.uri_of();
            let index = grammar_index::build(&set, &|p: &Path| buffers.get(p), &uri_of);
            let file = index.file(&path)?;
            Some((index, file))
        });
        match grammar {
            Some(None) => None,
            Some(Some(grammar)) => Some((offset, Some(grammar))),
            None => Some((offset, None)),
        }
    }

    /// The document's analysis, brought up to date first if an edit is pending.
    fn current(&mut self, uri: &str) -> Option<&Analysis> {
        // A request can arrive before the debounce elapses: load languages first.
        if std::mem::take(&mut self.reload) {
            self.reload_languages().ok()?;
        }
        if self.pending.remove(uri) {
            // Diagnostics are published as part of this analysis.
            self.analyze(uri).ok()?;
        }
        self.analyses.get(uri)
    }

    fn notification(&mut self, method: &str, params: Value) -> io::Result<bool> {
        match method {
            "exit" => return Ok(true),
            "workspace/didChangeWatchedFiles" => self.reload = true,
            "workspace/didChangeWorkspaceFolders" => {
                let folders = |key: &str| -> Vec<PathBuf> {
                    params["event"][key]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|f| f["uri"].as_str().and_then(uri::to_path))
                        .collect()
                };
                let removed = folders("removed");
                self.folders.retain(|f| !removed.contains(f));
                self.folders.extend(folders("added"));
                self.reload = true;
            }
            "textDocument/didOpen" => {
                let document = &params["textDocument"];
                if let (Some(uri), Some(text)) =
                    (document["uri"].as_str(), document["text"].as_str())
                {
                    let version = document["version"].as_i64().unwrap_or(0) as i32;
                    self.documents.insert(
                        uri.into(),
                        Document {
                            text: text.into(),
                            version,
                        },
                    );
                    self.pending.insert(uri.into());
                    self.reload |= language_input(uri);
                }
            }
            "textDocument/didChange" => {
                let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
                let text = params["contentChanges"]
                    .as_array()
                    .and_then(|changes| changes.last())
                    .and_then(|change| change["text"].as_str());
                if let (Some(document), Some(text)) = (self.documents.get_mut(uri), text) {
                    document.text = text.into();
                    document.version =
                        params["textDocument"]["version"].as_i64().unwrap_or(0) as i32;
                    self.pending.insert(uri.into());
                    self.reload |= language_input(uri);
                }
            }
            "textDocument/didClose" => {
                let uri = params["textDocument"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                self.documents.remove(&uri);
                self.pending.remove(&uri);
                if self.analyses.remove(&uri).is_some() {
                    self.publish(&uri, vec![])?;
                }
                // The file on disk replaces the closed buffer.
                self.reload |= language_input(&uri);
            }
            _ => {}
        }
        Ok(false)
    }

    /// Brings every pending document's analysis up to date.
    fn flush(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.reload) {
            self.reload_languages()?;
        }
        for uri in std::mem::take(&mut self.pending) {
            self.analyze(&uri)?;
        }
        Ok(())
    }

    /// Re-analyses one open document and publishes its diagnostics.
    fn analyze(&mut self, uri: &str) -> io::Result<()> {
        let (Some(document), Some(path)) = (self.documents.get(uri), uri::to_path(uri)) else {
            return Ok(());
        };
        let buffers = self.buffers();
        let analysis = analysis::analyze(&self.registry, &path, &document.text, &|p: &Path| {
            buffers.get(p)
        });
        match analysis {
            Some(analysis) => {
                let diagnostics = analysis
                    .problems
                    .iter()
                    .map(|p| {
                        let mut d = diagnostic(&analysis.lines, p.span, p.severity, &p.message);
                        if let Some(code) = &p.code {
                            d["code"] = json!(code);
                        }
                        d
                    })
                    .collect();
                self.analyses.insert(uri.to_owned(), analysis);
                self.publish(uri, diagnostics)
            }
            None if self.analyses.remove(uri).is_some() => self.publish(uri, vec![]),
            None => Ok(()),
        }
    }

    /// Names a file for the client: the URI it opened the file under, else
    /// the normalized path's.
    fn uri_of(&self) -> Box<dyn Fn(&Path) -> String> {
        let open: Vec<(PathBuf, String)> = self
            .documents
            .keys()
            .filter_map(|u| Some((uri::to_path(u)?, u.clone())))
            .collect();
        Box::new(move |path: &Path| {
            open.iter()
                .find(|(p, _)| crate::registry::same_file(p, path))
                .map_or_else(|| grammar_index::default_uri(path), |(_, u)| u.clone())
        })
    }

    /// Open buffers by path.
    fn buffers(&self) -> Buffers {
        Buffers(
            self.documents
                .iter()
                .filter_map(|(u, d)| Some((canonical(&uri::to_path(u)?), d.text.clone())))
                .collect(),
        )
    }

    fn reload_languages(&mut self) -> io::Result<()> {
        self.grammars = None;
        let buffers = self.buffers();
        self.registry
            .reload(&self.folders, &|path: &Path| buffers.get(path));
        let mut published = Vec::new();
        for manifest in self.registry.manifests() {
            let text = buffers
                .get(&manifest.path)
                .or_else(|| std::fs::read_to_string(&manifest.path).ok())
                .unwrap_or_default();
            let lines = LineIndex::new(&text);
            let diagnostics: Vec<_> = manifest
                .errors
                .iter()
                .map(|e| diagnostic(&lines, e.span, 1, &e.message))
                .collect();
            published.push((uri::from_path(&manifest.path), diagnostics));
        }
        for (uri, diagnostics) in published {
            self.publish(&uri, diagnostics)?;
        }
        // Every open document may now belong to a different language.
        self.pending.extend(self.documents.keys().cloned());
        Ok(())
    }

    pub fn publish(&mut self, uri: &str, diagnostics: Vec<Value>) -> io::Result<()> {
        self.send(
            json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics",
            "params":{"uri":uri,"diagnostics":diagnostics}}),
        )
    }
}

/// Open editor buffers, keyed so that every spelling of a file finds it: a
/// manifest's `sub/../x.ngr`, or `C:\\x` against a `file:///c%3A/x` URI.
struct Buffers(HashMap<PathBuf, String>);

impl Buffers {
    fn get(&self, path: &Path) -> Option<String> {
        self.0.get(&canonical(path)).cloned()
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

/// Whether a document is an input of workspace languages.
fn language_input(uri: &str) -> bool {
    let name = uri.rsplit('/').next().unwrap_or_default();
    manifest::NAMES.contains(&name) || name.ends_with(".ngr") || name.ends_with(".json")
}

pub fn range(lines: &LineIndex, span: [u32; 2]) -> Value {
    let (sl, sc) = lines.position(span[0]);
    let (el, ec) = lines.position(span[1].max(span[0]));
    json!({"start":{"line":sl,"character":sc},"end":{"line":el,"character":ec}})
}

/// An LSP diagnostic; severity 1 is an error, 2 a warning.
pub fn diagnostic(lines: &LineIndex, span: [u32; 2], severity: u8, message: &str) -> Value {
    json!({"range":range(lines, span),"severity":severity,"source":"nitride","message":message})
}
