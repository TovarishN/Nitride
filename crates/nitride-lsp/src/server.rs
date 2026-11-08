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
use std::time::{Duration, Instant};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

enum Incoming {
    Message(Value),
    Malformed(String),
}

use crate::document::Document;

pub struct Server<W: Write> {
    output: W,
    shutdown: bool,
    pub documents: HashMap<String, Document>,
    /// Documents whose analysis is out of date.
    pending: BTreeSet<String>,
    debounce: Duration,
    analysis_deadline: Option<Instant>,
    folders: Vec<PathBuf>,
    pub registry: Registry,
    /// Manifests, grammars or catalogs changed since languages were loaded.
    reload: bool,
    pub analyses: HashMap<String, Analysis>,
    /// Every workspace grammar, indexed on demand and dropped when languages
    /// or grammar buffers change.
    grammars: Option<GrammarIndex>,
    /// The client colours palette token types (`initializationOptions.paletteTokens`).
    palette: bool,
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
    let mut server = Server::new(output, options.debounce);
    loop {
        if server.pending.is_empty() && !server.reload {
            server.analysis_deadline = None;
        }
        let deadline = server
            .analysis_deadline
            .filter(|_| !options.debounce.is_zero());
        let incoming = if let Some(deadline) = deadline {
            match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(incoming) => incoming,
                Err(RecvTimeoutError::Timeout) => {
                    server.flush()?;
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        } else {
            match receiver.recv() {
                Ok(incoming) => incoming,
                Err(_) => return Ok(()),
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
        // Give the next queued edit/close/exit a chance to supersede old work,
        // then enforce expiry even if unrelated input keeps the queue occupied.
        if options.debounce.is_zero()
            || server
                .analysis_deadline
                .is_some_and(|due| Instant::now() >= due)
        {
            server.flush()?;
        }
    }
}

impl<W: Write> Server<W> {
    fn new(output: W, debounce: Duration) -> Self {
        Self {
            output,
            shutdown: false,
            documents: HashMap::new(),
            pending: BTreeSet::new(),
            debounce,
            analysis_deadline: None,
            folders: Vec::new(),
            registry: Registry::default(),
            reload: false,
            analyses: HashMap::new(),
            grammars: None,
            palette: false,
        }
    }

    fn schedule(&mut self) {
        self.analysis_deadline = Instant::now().checked_add(self.debounce);
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
        self.palette = params["initializationOptions"]["paletteTokens"].as_bool() == Some(true);
        json!({
            "capabilities": {
                "positionEncoding": "utf-16",
                "textDocumentSync": 2,
                "workspace": {"workspaceFolders": {"supported": true, "changeNotifications": true}},
                "semanticTokensProvider": {
                    "legend": {"tokenTypes": tokens::legend(self.palette), "tokenModifiers": tokens::MODIFIERS},
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
                "codeActionProvider": {"codeActionKinds": ["quickfix", "source.fixAll"]},
                "completionProvider": {"triggerCharacters": ["."]},
                "signatureHelpProvider": {"triggerCharacters": ["(", ","]},
                "documentFormattingProvider": true,
                "documentRangeFormattingProvider": true,
                "documentOnTypeFormattingProvider": {"firstTriggerCharacter": "}"},
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
                let palette = self.palette;
                let data = self
                    .current(&uri)
                    .map(|a| tokens::encode(a, palette))
                    .unwrap_or_default();
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
            "textDocument/definition" | "textDocument/declaration" if uri.ends_with(".nproj") => {
                Ok(self.project_definition(&uri, params).unwrap_or(Value::Null))
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
            "textDocument/formatting"
            | "textDocument/rangeFormatting"
            | "textDocument/onTypeFormatting" => {
                let Some(analysis) = self.current(&uri) else {
                    return Ok(json!([]));
                };
                let line =
                    |p: &Value| u32::try_from(p["line"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
                let (first, last) = match method {
                    "textDocument/rangeFormatting" => (
                        line(&params["range"]["start"]),
                        line(&params["range"]["end"]),
                    ),
                    "textDocument/onTypeFormatting" => {
                        (line(&params["position"]), line(&params["position"]))
                    }
                    _ => (0, u32::MAX),
                };
                let tab_size =
                    u32::try_from(params["options"]["tabSize"].as_u64().unwrap_or(2)).unwrap_or(2);
                let spaces = params["options"]["insertSpaces"].as_bool().unwrap_or(true);
                // Typing `}` reindents its line only; the file end is left alone.
                let end_of_file = method != "textDocument/onTypeFormatting";
                Ok(Value::Array(crate::format::edits(
                    analysis,
                    first,
                    last,
                    tab_size,
                    spaces,
                    end_of_file,
                )))
            }
            "textDocument/codeAction" => {
                let Some(analysis) = self.current(&uri) else {
                    return Ok(json!([]));
                };
                let at = |key: &str| {
                    let p = &params["range"][key];
                    let line = u32::try_from(p["line"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
                    let character =
                        u32::try_from(p["character"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
                    analysis.lines.offset(line, character)
                };
                let only: Vec<String> = params["context"]["only"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|k| k.as_str().map(str::to_owned))
                    .collect();
                Ok(crate::actions::actions(
                    analysis,
                    &uri,
                    at("start"),
                    at("end"),
                    &only,
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
            "nitride/languageExtensions" => {
                // File extensions that languages claim (from manifests and signed
                // grammars); the client routes those files here.
                if std::mem::take(&mut self.reload) {
                    self.reload_languages()
                        .map_err(|e| (INVALID_REQUEST, e.to_string()))?;
                }
                let mut extensions: Vec<&str> = self
                    .registry
                    .languages()
                    .iter()
                    .flat_map(|l| l.entry.extensions.iter().map(String::as_str))
                    .collect();
                extensions.sort_unstable();
                extensions.dedup();
                Ok(json!(extensions))
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
                        let entry = &language.entry;
                        for grammar in entry.grammars.iter().chain(&entry.catalogs) {
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
        // A grammar or catalog by name, a grammar by signature, or a signed
        // grammar's declaring file.
        let is_grammar = path
            .extension()
            .is_some_and(|e| e == "ngr" || e == "ncatalog")
            || !analysis.outline.is_empty()
            || self.registry.signed_at(&path).is_some();
        let grammar = is_grammar.then(|| {
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
    /// The file, language file or start rule named at a position of a `.nproj`.
    fn project_definition(&mut self, uri: &str, params: &Value) -> Option<Value> {
        let line = params["position"]["line"].as_u64()? as u32;
        let character = params["position"]["character"].as_u64()? as u32;
        let offset = self.current(uri)?.lines.offset(line, character) as usize;
        let path = uri::to_path(uri)?;
        let buffers = self.buffers();
        let source = nitride_prepare::Source {
            path: path.to_string_lossy().into_owned(),
            text: buffers.get(&path)?,
        };
        let references = nitride_prepare::project_references(&source).ok()?;
        let at = references
            .iter()
            .find(|r| r.span[0] <= offset && offset <= r.span[1])?;
        let uri_of = self.uri_of();
        if let Some(target) = &at.target {
            let start = json!({"line":0,"character":0});
            return Some(json!({"uri":uri_of(target),"range":{"start":start,"end":start}}));
        }
        let grammars: Vec<PathBuf> = references
            .iter()
            .filter(|r| r.kind == "grammar")
            .filter_map(|r| r.target.clone())
            .collect();
        let index = grammar_index::build(&grammars, &|p: &Path| buffers.get(p), &uri_of);
        let rule = index.definition(&at.text, "")?;
        let file = &index.files[rule.file];
        let span = rule.item.name_span.map(|v| v as u32);
        Some(json!({"uri":file.uri,"range":range(&LineIndex::new(&file.text), span)}))
    }

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
            "initialized" if self.reload => self.schedule(),
            "workspace/didChangeWatchedFiles" => {
                // The client watches every file; only some change the languages.
                let changes = params["changes"].as_array().cloned().unwrap_or_default();
                let relevant = changes.iter().any(|change| {
                    let path = change["uri"].as_str().and_then(uri::to_path);
                    let kind = change["type"].as_u64().unwrap_or(2);
                    path.is_some_and(|p| {
                        crate::registry::watched_change_matters(&self.registry, &p, kind)
                    })
                });
                self.reload |= relevant;
                if relevant {
                    self.schedule();
                }
            }
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
                self.schedule();
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
                    // Grammars and signed grammars are language inputs.
                    self.reload |= language_input(uri) || crate::signed::declares_grammar(text);
                    self.schedule();
                }
            }
            "textDocument/didChange" => {
                let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
                let version = params["textDocument"]["version"]
                    .as_i64()
                    .and_then(|v| i32::try_from(v).ok());
                if let (Some(document), Some(version)) = (self.documents.get_mut(uri), version) {
                    let was_signed = crate::signed::declares_grammar(&document.text);
                    if document.apply(version, &params["contentChanges"]) {
                        self.pending.insert(uri.into());
                        self.reload |= language_input(uri)
                            || was_signed
                            || crate::signed::declares_grammar(&document.text);
                        self.schedule();
                    }
                }
            }
            "textDocument/didClose" => {
                let uri = params["textDocument"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let closed = self.documents.remove(&uri);
                self.pending.remove(&uri);
                if self.analyses.remove(&uri).is_some() {
                    self.publish(&uri, vec![])?;
                }
                // The file on disk replaces the closed buffer.
                let reload = language_input(&uri)
                    || closed.is_some_and(|d| crate::signed::declares_grammar(&d.text));
                self.reload |= reload;
                if reload {
                    self.schedule();
                }
            }
            _ => {}
        }
        Ok(false)
    }

    /// Brings every pending document's analysis up to date.
    fn flush(&mut self) -> io::Result<()> {
        self.analysis_deadline = None;
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
        let analysis = analysis::analyze_incremental(
            &self.registry,
            &path,
            &document.text,
            &|p: &Path| buffers.get(p),
            self.analyses.get(uri),
        );
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
        let open_paths: Vec<PathBuf> = self
            .documents
            .keys()
            .filter_map(|u| uri::to_path(u))
            .collect();
        let changed =
            self.registry
                .reload_with(&self.folders, &|path: &Path| buffers.get(path), &open_paths);
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
        if changed {
            self.pending.extend(self.documents.keys().cloned());
        }
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
    manifest::NAMES.contains(&name)
        || name.ends_with(".ngr")
        || name.ends_with(".ncatalog")
        || name.ends_with(".nbind")
        || name.ends_with(".nproj")
        || name.ends_with(".json")
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

#[cfg(test)]
mod scheduling_tests {
    use super::*;

    #[test]
    fn unrelated_stale_and_invalid_notifications_keep_the_edit_deadline() {
        let mut server = Server::new(Vec::new(), Duration::from_millis(20));
        let uri = "file:///document.simple";
        server
            .notification(
                "textDocument/didOpen",
                json!({"textDocument":{
            "uri":uri,"version":1,"text":"ok"}}),
            )
            .unwrap();
        let deadline = Some(Instant::now() + Duration::from_secs(60));
        server.analysis_deadline = deadline;
        for (method, params) in [
            ("$/cancelRequest", json!({"id":7})),
            (
                "workspace/didChangeWatchedFiles",
                json!({"changes":[{"uri":"file:///unrelated.txt","type":2}]}),
            ),
            (
                "textDocument/didChange",
                json!({"textDocument":{"uri":uri,"version":1},"contentChanges":[{"text":"stale"}]}),
            ),
            (
                "textDocument/didChange",
                json!({"textDocument":{"uri":uri,"version":2},"contentChanges":[{
                "range":{"start":{"line":2,"character":0},"end":{"line":2,"character":1}},"text":"invalid"}]}),
            ),
        ] {
            server.notification(method, params).unwrap();
            assert_eq!(server.analysis_deadline, deadline, "{method}");
        }
        assert_eq!(server.documents[uri].text, "ok");
        server
            .notification(
                "textDocument/didChange",
                json!({"textDocument":{"uri":uri,"version":2},
            "contentChanges":[{"text":"updated"}]}),
            )
            .unwrap();
        assert_ne!(server.analysis_deadline, deadline);
        assert_eq!(server.documents[uri].text, "updated");
    }

    #[test]
    fn initial_reload_waits_for_the_initialized_notification() {
        let mut server = Server::new(Vec::new(), Duration::from_millis(20));
        server.initialize(&json!({}));
        assert!(server.reload);
        assert!(server.analysis_deadline.is_none());
        server.notification("initialized", json!({})).unwrap();
        assert!(server.analysis_deadline.is_some());
    }
}
