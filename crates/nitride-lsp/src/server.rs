//! The message loop: document sync, debounced analysis and request dispatch.
use crate::analysis::{self, Analysis};
use crate::background;
mod jobs;
mod schedule;
use crate::grammar_index::{self, GrammarIndex};
use crate::lines::LineIndex;
use crate::navigation::{self, GrammarAt};
use crate::registry::Registry;
use crate::{ServeOptions, rpc, structure, tokens, uri};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

enum Incoming {
    Message(Value),
    Malformed(String),
    Wake,
    End,
}

use crate::document::Document;

pub struct Server<W: Write> {
    output: W,
    shutdown: bool,
    pub documents: HashMap<String, Document>,
    /// Path-based language-input classification, refreshed on open/reload.
    input_documents: std::collections::HashSet<String>,
    /// Documents whose analysis is out of date.
    pending: BTreeSet<String>,
    debounce: Duration,
    timing: schedule::Schedule,
    folders: Vec<PathBuf>,
    pub registry: Registry,
    /// Manifests, grammars or catalogs changed since languages were loaded.
    reload: bool,
    pub analyses: HashMap<String, Arc<Analysis>>,
    /// Every workspace grammar, indexed on demand and dropped when languages
    /// or grammar buffers change.
    grammars: Option<GrammarIndex>,
    /// The client colours palette token types (`initializationOptions.paletteTokens`).
    palette: bool,
    library_documents: bool,
    project_create: bool,
    action_resolve: bool,
    jobs: jobs::State,
}

pub fn run(
    input: impl BufRead + Send + 'static,
    output: impl Write,
    options: ServeOptions,
) -> io::Result<()> {
    run_with_analyzer(input, output, options, Box::new(background::Job::analyze))
}

fn run_with_analyzer(
    input: impl BufRead + Send + 'static,
    output: impl Write,
    options: ServeOptions,
    analyze: background::Analyzer,
) -> io::Result<()> {
    let (sender, receiver) = mpsc::channel();
    let (completed_sender, completed_receiver) = mpsc::sync_channel::<background::Completion>(1);
    let input_sender = sender.clone();
    std::thread::spawn(move || {
        let mut input = input;
        while let Ok(Some(message)) = rpc::read(&mut input) {
            let incoming = match message {
                Ok(value) => Incoming::Message(value),
                Err(error) => Incoming::Malformed(error),
            };
            if input_sender.send(incoming).is_err() {
                break;
            }
        }
        let _ = input_sender.send(Incoming::End);
    });
    let mut server = Server::new(output, options.debounce);
    if !options.debounce.is_zero() {
        server.jobs.worker = Some(background::Worker::spawn(
            move |done| {
                let _ = completed_sender.send(done);
                let _ = sender.send(Incoming::Wake);
            },
            analyze,
        ));
    }
    loop {
        // One completion and one input per turn: neither a queued client stream
        // nor a ready batch of small documents can starve the other.
        if let Ok(done) = completed_receiver.try_recv() {
            server.completed(done)?;
            server.pump()?;
            if server.jobs.exit_ready {
                return Ok(());
            }
        }
        server.timing.retain(&server.pending, server.reload);
        let deadline = server
            .timing
            .next(server.reload)
            .filter(|_| !options.debounce.is_zero());
        let incoming = if let Some(deadline) = deadline {
            match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(incoming) => incoming,
                Err(RecvTimeoutError::Timeout) => {
                    server.flush_due()?;
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
            Incoming::Wake => {}
            Incoming::End => {
                if server.terminate() {
                    return Ok(());
                }
            }
        }
        // Give the next queued edit/close/exit a chance to supersede old work,
        // then enforce expiry even if unrelated input keeps the queue occupied.
        if options.debounce.is_zero() {
            server.flush()?;
        } else if server
            .timing
            .next(server.reload)
            .is_some_and(|due| Instant::now() >= due)
        {
            server.flush_due()?;
        }
        server.pump()?;
        if server.jobs.exit_ready {
            return Ok(());
        }
    }
}

impl<W: Write> Drop for Server<W> {
    fn drop(&mut self) {
        self.jobs.cancel_active();
    }
}

impl<W: Write> Server<W> {
    fn new(output: W, debounce: Duration) -> Self {
        Self {
            output,
            shutdown: false,
            documents: HashMap::new(),
            input_documents: Default::default(),
            pending: BTreeSet::new(),
            debounce,
            timing: schedule::Schedule::default(),
            folders: Vec::new(),
            registry: Registry::default(),
            reload: false,
            analyses: HashMap::new(),
            grammars: None,
            palette: false,
            library_documents: false,
            project_create: false,
            action_resolve: false,
            jobs: jobs::State::default(),
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
        if self.jobs.worker.is_some() && method == "shutdown" {
            self.begin_shutdown(id)?;
            return Ok(false);
        }
        if self.defer_request(&message)? {
            return Ok(false);
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
        self.project_create = params["capabilities"]["workspace"]["workspaceEdit"]["documentChanges"]
            == true
            && params["capabilities"]["workspace"]["workspaceEdit"]["resourceOperations"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == "create"));
        self.action_resolve =
            params["capabilities"]["textDocument"]["codeAction"]["resolveSupport"]["properties"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == "edit"));
        self.library_documents =
            params["initializationOptions"]["libraryDocuments"].as_bool() == Some(true);
        self.palette = params["initializationOptions"]["paletteTokens"].as_bool() == Some(true);
        json!({
            "capabilities": {
                "positionEncoding": "utf-16",
                "executeCommandProvider":{"commands":["nitride.createLanguage"]},
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
                "codeActionProvider": {"codeActionKinds": ["quickfix", "source.fixAll"], "resolveProvider":true},
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
        if uri.starts_with("nitride-lib:")
            && matches!(
                method,
                "textDocument/rename"
                    | "textDocument/prepareRename"
                    | "textDocument/formatting"
                    | "textDocument/rangeFormatting"
                    | "textDocument/onTypeFormatting"
                    | "textDocument/codeAction"
            )
        {
            return if method.contains("Rename") || method.ends_with("/rename") {
                Err((
                    INVALID_PARAMS,
                    "bundled library sources are read-only".into(),
                ))
            } else {
                Ok(json!([]))
            };
        }
        match method {
            "codeAction/resolve" => {
                if std::mem::take(&mut self.reload) {
                    self.reload_languages()
                        .map_err(|e| (INVALID_REQUEST, e.to_string()))?;
                }
                Ok(self.project_action_context().resolve_action(params))
            }
            "workspace/executeCommand" => {
                if params["command"] != "nitride.createLanguage" {
                    return Err((INVALID_PARAMS, "unknown command".into()));
                }
                let options = &params["arguments"][0];
                let field = |key| {
                    options[key]
                        .as_str()
                        .ok_or_else(|| (INVALID_PARAMS, format!("missing {key}")))
                };
                let directory = uri::to_path(field("directory")?)
                    .ok_or_else(|| (INVALID_PARAMS, "target requires file URI".into()))?;
                let directory =
                    nitride_prepare::authoring::workspace_target(&directory, &self.folders)
                        .map_err(|e| (INVALID_PARAMS, e.to_string()))?;
                let kind = match options["kind"].as_str().unwrap_or("typed") {
                    "typed" => nitride_prepare::authoring::TemplateKind::Typed,
                    "syntax" => nitride_prepare::authoring::TemplateKind::Syntax,
                    _ => return Err((INVALID_PARAMS, "unknown template kind".into())),
                };
                let plan = nitride_prepare::authoring::plan_language(
                    &nitride_prepare::authoring::TemplateOptions {
                        name: field("name")?.into(),
                        extension: field("extension")?.into(),
                        kind,
                    },
                )
                .map_err(|e| (INVALID_PARAMS, e.to_string()))?;
                if plan.files.iter().any(|file| {
                    self.documents
                        .keys()
                        .filter_map(|u| uri::to_path(u))
                        .any(|p| {
                            crate::registry::same_file(&p, &directory.join(&file.relative_path))
                        })
                }) {
                    return Err((
                        INVALID_PARAMS,
                        "a planned target is already open in the editor".into(),
                    ));
                }
                let report = nitride_prepare::authoring::create_language(&directory, &plan)
                    .map_err(|e| (INVALID_PARAMS, e.to_string()))?;
                Ok(
                    json!({"directory":uri::from_path(&directory),"created":report.created.iter().map(|p|uri::from_path(p)).collect::<Vec<_>>()}),
                )
            }
            "nitride/librarySource" => {
                let source = params["uri"]
                    .as_str()
                    .and_then(nitride_prepare::stdlib::source)
                    .ok_or_else(|| (INVALID_PARAMS, "unknown canonical library URI".into()))?;
                Ok(json!(source.text))
            }
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
            "textDocument/hover" | "textDocument/completion" if uri.ends_with(".nproj") => {
                let analysis = self.current(&uri);
                let Some(analysis) = analysis else {
                    return Ok(Value::Null);
                };
                let offset = analysis.lines.offset(
                    params["position"]["line"].as_u64().unwrap_or(0) as u32,
                    params["position"]["character"].as_u64().unwrap_or(0) as u32,
                ) as usize;
                let text = analysis.lines.text();
                if method.ends_with("hover") {
                    let source = nitride_prepare::Source {
                        path: uri.clone(),
                        text: text.into(),
                    };
                    let references =
                        nitride_prepare::project_references(&source).unwrap_or_default();
                    let library = references
                        .iter()
                        .find(|r| r.span[0] <= offset && offset <= r.span[1])
                        .and_then(|r| r.library.as_ref());
                    return Ok(library.map_or(Value::Null,|l|json!({"contents":{"kind":"markdown","value":format!("Bundled `{}@{}` (offline, read-only)",l.name,l.version)}})));
                }
                let prefix = &text[..offset.min(text.len())];
                let member = prefix
                    .rsplit([';', '{', '}'])
                    .next()
                    .unwrap_or_default()
                    .trim_start();
                let items = if member.starts_with("library ") {
                    nitride_prepare::stdlib::libraries().iter().map(|l| if member.contains(" version ") {json!({"label":l.version,"kind":12,"detail":l.name})}else{json!({"label":l.name,"kind":9,"detail":format!("bundled {}",l.version)})}).collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                Ok(json!(items))
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
                        let mut locations =
                            navigation::references(analysis, &uri, offset, at, include);
                        if !self.library_documents {
                            if let Some(items) = locations.as_array_mut() {
                                items.retain(|l| {
                                    !l["uri"]
                                        .as_str()
                                        .is_some_and(|u| u.starts_with("nitride-lib:"))
                                });
                            }
                        }
                        locations
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
                    _ => {
                        let location = navigation::definition(analysis, &uri, offset, at);
                        if !self.library_documents
                            && location["uri"]
                                .as_str()
                                .is_some_and(|u| u.starts_with("nitride-lib:"))
                        {
                            Value::Null
                        } else {
                            location
                        }
                    }
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
                let (start, end) = (at("start"), at("end"));
                let mut actions = crate::actions::actions(analysis, &uri, start, end, &only);
                if let Some(items) = actions.as_array_mut() {
                    items.extend(
                        self.project_action_context()
                            .actions(&uri, start, end, &only),
                    );
                }
                Ok(actions)
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
                let open = self.analyses.iter().map(|(u, a)| (u.as_str(), a.as_ref()));
                Ok(navigation::workspace_symbols(grammars, open, &query))
            }
            _ => Err((METHOD_NOT_FOUND, "method not found".into())),
        }
    }

    fn project_action_context(&self) -> crate::project_actions::Context<'_> {
        crate::project_actions::Context {
            documents: &self.documents,
            registry: &self.registry,
            roots: &self.folders,
            create: self.project_create,
            resolve: self.action_resolve,
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
        if uri.starts_with("nitride-lib:") {
            let source = nitride_prepare::stdlib::source(uri)?;
            let library = nitride_prepare::stdlib::libraries()
                .iter()
                .find(|l| l.sources.iter().any(|s| s.uri == source.uri))?;
            let mut index = grammar_index::build(&[], &|_| None, &|_| String::new());
            index.add_libraries(&[*library]);
            let file = index.files.iter().position(|f| f.uri == uri)?;
            return Some((offset, Some((index, file))));
        }
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
            let mut set = self.registry.grammar_set(&path);
            let mut libraries = self.registry.libraries_at(&path);
            if self.registry.using_grammar(&path).next().is_none()
                && self.registry.using_catalog(&path).next().is_none()
                && let Some((manifest, entry)) =
                    crate::manifest::language_beside(&path, &|p| buffers.get(p))
            {
                let text = buffers
                    .get(&manifest)
                    .or_else(|| std::fs::read_to_string(&manifest).ok())
                    .unwrap_or_default();
                let source = nitride_prepare::Source {
                    path: manifest.display().to_string(),
                    text,
                };
                libraries = nitride_prepare::resolve_libraries(&source, &entry.libraries)
                    .unwrap_or_default();
                set = entry.grammars.into_iter().chain(entry.catalogs).collect();
            }
            let uri_of = self.uri_of();
            let mut index = grammar_index::build(&set, &|p: &Path| buffers.get(p), &uri_of);
            index.add_libraries(&libraries);
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
        if let Some(library) = &at.library {
            if !self.library_documents {
                return None;
            }
            let source = nitride_prepare::stdlib::find(&library.name, &library.version)?
                .sources
                .first()?;
            return Some(
                json!({"uri":source.uri,"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}}}),
            );
        }
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
        if let Some(source) = nitride_prepare::stdlib::source(uri) {
            if !self.analyses.contains_key(uri) {
                self.analyses
                    .insert(uri.into(), Arc::new(analysis::library(uri, source.text)));
            }
            return self.analyses.get(uri).map(Arc::as_ref);
        }
        // A request can arrive before the debounce elapses: load languages first.
        if std::mem::take(&mut self.reload) {
            self.reload_languages().ok()?;
        }
        if self.pending.remove(uri) {
            // Diagnostics are published as part of this analysis.
            self.analyze(uri).ok()?;
        }
        self.analyses.get(uri).map(Arc::as_ref)
    }

    fn notification(&mut self, method: &str, params: Value) -> io::Result<bool> {
        if self.shutdown && method != "exit" && method != "$/cancelRequest" {
            return Ok(false);
        }
        if params["textDocument"]["uri"]
            .as_str()
            .is_some_and(|u| u.starts_with("nitride-lib:"))
        {
            return Ok(false);
        }
        match method {
            "exit" => return Ok(self.terminate()),
            "$/cancelRequest" => self.cancel_request(&params["id"])?,
            "initialized" if self.reload => self.schedule_languages(),
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
                self.changed_languages(relevant)?;
                if relevant {
                    self.schedule_languages();
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
                self.changed_languages(true)?;
                self.schedule_languages();
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
                    self.changed_document(uri)?;
                    // Grammars and signed grammars are language inputs.
                    let input = language_input(&self.registry, uri);
                    if input {
                        self.input_documents.insert(uri.into());
                    } else {
                        self.input_documents.remove(uri);
                    }
                    let language_changed = input || crate::signed::declares_grammar(text);
                    self.changed_languages(language_changed)?;
                    self.schedule_document(uri);
                    if language_changed {
                        self.schedule_languages();
                    }
                }
            }
            "textDocument/didChange" => {
                let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
                let version = params["textDocument"]["version"]
                    .as_i64()
                    .and_then(|v| i32::try_from(v).ok());
                let input = self.input_documents.contains(uri);
                if let (Some(document), Some(version)) = (self.documents.get_mut(uri), version) {
                    let was_signed = crate::signed::declares_grammar(&document.text);
                    if document.apply(version, &params["contentChanges"]) {
                        let language_changed =
                            input || was_signed || crate::signed::declares_grammar(&document.text);
                        self.pending.insert(uri.into());
                        self.changed_document(uri)?;
                        self.changed_languages(language_changed)?;
                        self.schedule_document(uri);
                        if language_changed {
                            self.schedule_languages();
                        }
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
                self.timing.forget(&uri);
                if closed.is_some() {
                    self.changed_document(&uri)?;
                }
                if self.analyses.remove(&uri).is_some() {
                    self.publish(&uri, vec![])?;
                }
                // The file on disk replaces the closed buffer.
                let reload = self.input_documents.remove(&uri)
                    || language_input(&self.registry, &uri)
                    || closed.is_some_and(|d| crate::signed::declares_grammar(&d.text));
                self.changed_languages(reload)?;
                if reload {
                    self.schedule_languages();
                }
            }
            _ => {}
        }
        Ok(false)
    }

    /// Brings every pending document's analysis up to date.
    fn flush(&mut self) -> io::Result<()> {
        if self.jobs.worker.is_some() {
            return self.expedite(None);
        }
        self.timing.clear();
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
            self.analyses.get(uri).map(Arc::as_ref),
        );
        self.commit_analysis(uri, analysis)
    }

    fn commit_analysis(&mut self, uri: &str, analysis: Option<Analysis>) -> io::Result<()> {
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
                        if uri.ends_with(".nproj") {
                            let source=nitride_prepare::Source {path:uri::to_path(uri).map(|p|p.display().to_string()).unwrap_or_default(),text:analysis.lines.text().into()};
                            let issues=nitride_prepare::authoring::project_issues(&source,&|p|self.buffers().get(p).is_some()||p.exists());
                            if let Some(issue)=issues.iter().find(|i|i.span.map(|b|b as u32)==p.span) {
                                d["code"]=json!(issue.code);
                                d["data"]=json!({"kind":issue.code,"project":uri,"target":issue.target.as_ref().map(|p|uri::from_path(p)),"version":self.documents.get(uri).map(|d|d.version)});
                            }
                        }
                        d
                    })
                    .collect();
                self.analyses.insert(uri.to_owned(), Arc::new(analysis));
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
        self.input_documents = self
            .documents
            .keys()
            .filter(|uri| language_input(&self.registry, uri))
            .cloned()
            .collect();
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
        let mut params = json!({"uri":uri,"diagnostics":diagnostics});
        if let Some(document) = self.documents.get(uri) {
            params["version"] = json!(document.version);
        }
        self.send(
            json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics", "params":params}),
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

pub(crate) fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

/// Whether a document is an input of workspace languages.
fn language_input(registry: &Registry, uri: &str) -> bool {
    let path = uri::to_path(uri).unwrap_or_else(|| PathBuf::from(uri));
    crate::registry::language_input_path(registry, &path)
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
        let deadline = Instant::now() + Duration::from_secs(60);
        server.timing.documents.insert(uri.into(), deadline);
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
            assert_eq!(
                server.timing.documents.get(uri),
                Some(&deadline),
                "{method}"
            );
        }
        assert_eq!(server.documents[uri].text, "ok");
        server
            .notification(
                "textDocument/didChange",
                json!({"textDocument":{"uri":uri,"version":2},
            "contentChanges":[{"text":"updated"}]}),
            )
            .unwrap();
        assert_ne!(server.timing.documents.get(uri), Some(&deadline));
        assert_eq!(server.documents[uri].text, "updated");
    }

    #[test]
    fn initial_reload_waits_for_the_initialized_notification() {
        let mut server = Server::new(Vec::new(), Duration::from_millis(20));
        server.initialize(&json!({}));
        assert!(server.reload);
        assert!(server.timing.reload.is_none());
        server.notification("initialized", json!({})).unwrap();
        assert!(server.timing.reload.is_some());
    }
}

#[cfg(test)]
mod async_tests;
