//! Main-thread ownership, freshness barriers and one outstanding worker ticket.
use super::*;
use crate::background::{Completion, Job, Outcome, Stamp, Worker};
use std::sync::atomic::{AtomicBool, Ordering};

const REQUEST_CANCELLED: i64 = -32800;
const CONTENT_MODIFIED: i64 = -32801;
const SERVER_CANCELLED: i64 = -32802;
const INTERNAL_ERROR: i64 = -32603;
const MAX_WAITING: usize = 256;

struct Active {
    ticket: u64,
    uri: String,
    stamp: Stamp,
    cancelled: Arc<AtomicBool>,
}
struct Waiting {
    message: Value,
    target: Option<String>,
    stamp: Option<Stamp>,
    revision: u64,
}
#[derive(Default)]
pub(super) struct State {
    pub worker: Option<Worker>,
    active: Option<Active>,
    epochs: HashMap<String, u64>,
    workspace: u64,
    revision: u64,
    ticket: u64,
    pub ready: bool,
    waiting: Vec<Waiting>,
    shutdown_id: Option<Value>,
    exit_after_shutdown: bool,
    pub exit_ready: bool,
}
impl State {
    fn changed(&mut self) -> u64 {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("analysis generation capacity");
        self.revision
    }
    pub fn cancel_active(&self) {
        if let Some(active) = &self.active {
            active.cancelled.store(true, Ordering::Relaxed);
        }
    }
}

impl<W: Write> Server<W> {
    fn stamp(&self, uri: &str) -> Option<Stamp> {
        self.documents.contains_key(uri).then(|| Stamp {
            document: *self.jobs.epochs.get(uri).unwrap_or(&0),
            workspace: self.jobs.workspace,
        })
    }
    pub(super) fn changed_document(&mut self, uri: &str) -> io::Result<()> {
        if self.jobs.worker.is_none() {
            return Ok(());
        }
        let epoch = self.jobs.changed();
        if self.documents.contains_key(uri) {
            self.jobs.epochs.insert(uri.to_owned(), epoch);
        } else {
            self.jobs.epochs.remove(uri);
        }
        if self.jobs.active.as_ref().is_some_and(|a| a.uri == uri) {
            self.jobs.cancel_active();
        }
        self.reject_waiting(CONTENT_MODIFIED, "document changed", |w| {
            w.target.as_deref().is_none_or(|target| target == uri)
        })
    }
    pub(super) fn changed_languages(&mut self, changed: bool) -> io::Result<()> {
        if !changed {
            return Ok(());
        }
        self.reload = true;
        if self.jobs.worker.is_some() {
            self.jobs.workspace = self.jobs.changed();
            self.jobs.cancel_active();
            self.pending.extend(self.documents.keys().cloned());
            self.reject_waiting(CONTENT_MODIFIED, "workspace changed", |_| true)?;
        }
        Ok(())
    }
    fn reject_waiting(
        &mut self,
        code: i64,
        reason: &str,
        reject: impl Fn(&Waiting) -> bool,
    ) -> io::Result<()> {
        let mut keep = Vec::new();
        for waiting in std::mem::take(&mut self.jobs.waiting) {
            if reject(&waiting) {
                self.error(waiting.message["id"].clone(), code, reason)?;
            } else {
                keep.push(waiting);
            }
        }
        self.jobs.waiting = keep;
        Ok(())
    }
    pub(super) fn cancel_request(&mut self, id: &Value) -> io::Result<()> {
        if !id.is_number() && !id.is_string() {
            return Ok(());
        }
        self.reject_waiting(REQUEST_CANCELLED, "request cancelled", |w| {
            w.message["id"] == *id
        })
    }
    fn unsettled(&self, target: Option<&str>) -> bool {
        self.reload
            || match target {
                Some(uri) => {
                    self.pending.contains(uri)
                        || self.jobs.active.as_ref().is_some_and(|a| a.uri == uri)
                }
                None => !self.pending.is_empty() || self.jobs.active.is_some(),
            }
    }
    pub(super) fn defer_request(&mut self, message: &Value) -> io::Result<bool> {
        if self.jobs.worker.is_none() {
            return Ok(false);
        }
        let method = message["method"].as_str().unwrap_or_default();
        let target = if method == "workspace/symbol" {
            None
        } else if document_method(method) {
            let uri = message["params"]["textDocument"]["uri"]
                .as_str()
                .unwrap_or_default();
            if !self.documents.contains_key(uri) {
                return Ok(false);
            }
            Some(uri.to_owned())
        } else {
            return Ok(false);
        };
        if !self.unsettled(target.as_deref()) {
            return Ok(false);
        }
        if self.jobs.waiting.len() >= MAX_WAITING {
            self.error(
                message["id"].clone(),
                SERVER_CANCELLED,
                "too many requests awaiting analysis",
            )?;
            return Ok(true);
        }
        let stamp = target.as_deref().and_then(|uri| self.stamp(uri));
        self.jobs.waiting.push(Waiting {
            message: message.clone(),
            target,
            stamp,
            revision: self.jobs.revision,
        });
        self.flush()?;
        Ok(true)
    }
    fn answer_settled(&mut self) -> io::Result<()> {
        for waiting in std::mem::take(&mut self.jobs.waiting) {
            let stale = match waiting.target.as_deref() {
                Some(uri) => self.stamp(uri) != waiting.stamp,
                None => self.jobs.revision != waiting.revision,
            };
            if stale {
                self.error(
                    waiting.message["id"].clone(),
                    CONTENT_MODIFIED,
                    "analysis input changed",
                )?;
            } else if self.unsettled(waiting.target.as_deref()) {
                self.jobs.waiting.push(waiting);
            } else {
                let id = waiting.message["id"].clone();
                let method = waiting.message["method"].as_str().unwrap_or_default();
                let params = waiting
                    .message
                    .get("params")
                    .cloned()
                    .unwrap_or(Value::Null);
                match self.request(method, &params) {
                    Ok(result) => self.send(json!({"jsonrpc":"2.0","id":id,"result":result}))?,
                    Err((code, message)) => self.error(id, code, &message)?,
                }
            }
        }
        Ok(())
    }
    pub(super) fn pump(&mut self) -> io::Result<()> {
        if self.jobs.worker.is_none() {
            return Ok(());
        }
        self.answer_settled()?;
        while self.jobs.active.is_none() {
            let requested = self
                .jobs
                .waiting
                .iter()
                .filter_map(|w| w.target.as_ref())
                .find(|uri| self.pending.contains(*uri))
                .cloned();
            let all = self.jobs.ready || self.jobs.waiting.iter().any(|w| w.target.is_none());
            let next = requested.or_else(|| all.then(|| self.pending.first().cloned()).flatten());
            if let Some(uri) = next {
                self.pending.remove(&uri);
                if let (Some(document), Some(path), Some(stamp)) = (
                    self.documents.get(&uri),
                    uri::to_path(&uri),
                    self.stamp(&uri),
                ) {
                    self.jobs.ticket = self
                        .jobs
                        .ticket
                        .checked_add(1)
                        .expect("analysis ticket capacity");
                    let ticket = self.jobs.ticket;
                    let cancelled = Arc::new(AtomicBool::new(false));
                    let job = Job {
                        ticket,
                        uri: uri.clone(),
                        stamp,
                        path,
                        text: document.text.clone(),
                        registry: self.registry.clone(),
                        buffers: self.buffers().0,
                        previous: self.analyses.get(&uri).cloned(),
                        cancelled: cancelled.clone(),
                    };
                    self.jobs.active = Some(Active {
                        ticket,
                        uri,
                        stamp,
                        cancelled,
                    });
                    if !self.jobs.worker.as_ref().unwrap().submit(job) {
                        return Err(io::Error::other("analysis worker unavailable"));
                    }
                }
            } else {
                break;
            }
        }
        // Skipped URIs have no completion wake. Settle their requests now,
        // including a workspace barrier when the entire batch was unsupported.
        self.answer_settled()?;
        if self.jobs.shutdown_id.is_some()
            && !self.reload
            && self.pending.is_empty()
            && self.jobs.active.is_none()
            && self.jobs.waiting.is_empty()
        {
            let id = self.jobs.shutdown_id.take().unwrap();
            self.send(json!({"jsonrpc":"2.0","id":id,"result":null}))?;
            self.jobs.exit_ready = self.jobs.exit_after_shutdown;
        }
        Ok(())
    }
    pub(super) fn completed(&mut self, done: Completion) -> io::Result<()> {
        if !self
            .jobs
            .active
            .as_ref()
            .is_some_and(|a| a.ticket == done.ticket && a.uri == done.uri && a.stamp == done.stamp)
        {
            return Ok(());
        }
        let active = self.jobs.active.take().unwrap();
        if self.stamp(&done.uri) != Some(done.stamp) || active.cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        match done.outcome {
            Outcome::Analyzed(result) => {
                self.commit_analysis(&done.uri, result.map(|analysis| *analysis))
            }
            Outcome::Cancelled => Ok(()),
            Outcome::Failed => {
                self.analyses.remove(&done.uri);
                self.reject_waiting(INTERNAL_ERROR, "background analysis failed", |w| {
                    w.target.as_deref().is_none_or(|uri| uri == done.uri)
                })?;
                self.publish(
                    &done.uri,
                    vec![diagnostic(
                        &LineIndex::new(""),
                        [0, 0],
                        1,
                        "background analysis failed",
                    )],
                )
            }
        }
    }
    pub(super) fn begin_shutdown(&mut self, id: Value) -> io::Result<()> {
        self.shutdown = true;
        self.jobs.shutdown_id = Some(id);
        self.flush()
    }
    /// Once shutdown is accepted, preserve its drain barrier even for a
    /// pipelined exit/EOF. A bare exit/EOF still terminates immediately.
    pub(super) fn terminate(&mut self) -> bool {
        if self.jobs.shutdown_id.is_some() {
            self.jobs.exit_after_shutdown = true;
            false
        } else {
            true
        }
    }
}
fn document_method(method: &str) -> bool {
    matches!(
        method,
        "textDocument/semanticTokens/full"
            | "textDocument/documentSymbol"
            | "textDocument/foldingRange"
            | "textDocument/selectionRange"
            | "textDocument/definition"
            | "textDocument/declaration"
            | "textDocument/hover"
            | "textDocument/references"
            | "textDocument/documentHighlight"
            | "textDocument/prepareRename"
            | "textDocument/rename"
            | "textDocument/completion"
            | "textDocument/signatureHelp"
            | "textDocument/formatting"
            | "textDocument/rangeFormatting"
            | "textDocument/onTypeFormatting"
            | "textDocument/codeAction"
            | "textDocument/inlayHint"
    )
}
