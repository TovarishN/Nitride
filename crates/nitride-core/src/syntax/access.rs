//! Semantic occurrence indexing is explicit work, independent of parse/edit.
//! Token text crossing source pieces is cached on demand; the source is not flattened.
use super::shared::Occurrences;
use crate::*;
use std::{borrow::Cow, sync::OnceLock};

pub(crate) trait SyntaxAccess {
    fn language(&self) -> &'static Language;
    fn root(&self) -> u32;
    fn nodes(&self) -> &[Node];
    fn edges(&self) -> &[Edge];
    fn label(&self, edge: &Edge) -> &'static str;
    fn text(&self, id: u32) -> Option<&str>;
    fn valid_span(&self, span: [u32; 2]) -> bool;
    fn recovery(&self) -> Option<&RecoveryData>;
    fn children(&self, id: u32) -> &[Edge] {
        let Some(node) = self.nodes().get(id as usize) else {
            return &[];
        };
        let Some(end) = node.first_child.checked_add(node.child_count) else {
            return &[];
        };
        self.edges()
            .get(node.first_child as usize..end as usize)
            .unwrap_or(&[])
    }
    fn field(&self, id: u32, label: &str) -> Option<u32> {
        self.children(id)
            .iter()
            .find(|e| self.label(e) == label)
            .map(|e| e.node)
    }
}
impl SyntaxAccess for SyntaxTree {
    fn language(&self) -> &'static Language {
        self.language
    }
    fn root(&self) -> u32 {
        self.root
    }
    fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    fn edges(&self) -> &[Edge] {
        &self.edges
    }
    fn label(&self, edge: &Edge) -> &'static str {
        SyntaxTree::label(self, edge)
    }
    fn text(&self, id: u32) -> Option<&str> {
        SyntaxTree::text(self, id)
    }
    fn valid_span(&self, span: [u32; 2]) -> bool {
        self.source
            .get(span[0] as usize..span[1] as usize)
            .is_some()
    }
    fn recovery(&self) -> Option<&RecoveryData> {
        SyntaxTree::recovery(self)
    }
}
struct SnapshotOccurrences<'a> {
    snapshot: &'a ParseSnapshot,
    indexed: Occurrences,
    text: Vec<OnceLock<Box<str>>>,
    recovery: Option<Box<RecoveryData>>,
}
impl<'a> SnapshotOccurrences<'a> {
    fn new(snapshot: &'a ParseSnapshot, limits: Limits) -> Result<Self, Diagnostic> {
        let indexed = snapshot.indexed(limits.work)?;
        let recovery = snapshot.recovery().map(|r| {
            Box::new(RecoveryData {
                repairs: r.repairs().to_vec(),
                coverage: r.coverage().collect(),
                diagnostics: snapshot.diagnostics().to_vec(),
                damaged_nodes: indexed.damaged_nodes.clone(),
            })
        });
        let text = (0..indexed.nodes.len()).map(|_| OnceLock::new()).collect();
        Ok(Self {
            snapshot,
            indexed,
            text,
            recovery,
        })
    }
}
impl SyntaxAccess for SnapshotOccurrences<'_> {
    fn language(&self) -> &'static Language {
        self.snapshot.language
    }
    fn root(&self) -> u32 {
        self.indexed.root
    }
    fn nodes(&self) -> &[Node] {
        &self.indexed.nodes
    }
    fn edges(&self) -> &[Edge] {
        &self.indexed.edges
    }
    fn label(&self, edge: &Edge) -> &'static str {
        self.snapshot
            .arena
            .labels
            .get(edge.label as usize)
            .copied()
            .unwrap_or("")
    }
    fn text(&self, id: u32) -> Option<&str> {
        let node = self.indexed.nodes.get(id as usize)?;
        match self.snapshot.source.slice(node.span).ok()? {
            Cow::Borrowed(s) => Some(s),
            Cow::Owned(s) => Some(
                self.text[id as usize]
                    .get_or_init(|| s.into_boxed_str())
                    .as_ref(),
            ),
        }
    }
    fn valid_span(&self, span: [u32; 2]) -> bool {
        self.snapshot.source.valid_span(span)
    }
    fn recovery(&self) -> Option<&RecoveryData> {
        self.recovery.as_deref()
    }
}
fn language_error() -> Diagnostic {
    Diagnostic::new(
        "capability",
        [0, 0],
        "snapshot belongs to a different language",
    )
}
pub fn compile_snapshot(
    language: &'static Language,
    snapshot: &ParseSnapshot,
    limits: Limits,
) -> Result<Program, Vec<Diagnostic>> {
    if !std::ptr::eq(language, snapshot.language) {
        return Err(vec![language_error()]);
    }
    if snapshot.recovery().is_some() || !snapshot.diagnostics().is_empty() {
        return Err(snapshot.diagnostics().to_vec());
    }
    if language.profile == Profile::SyntaxOnly {
        return Err(vec![Diagnostic::new(
            "capability",
            [0, 0],
            "syntax-only language does not admit semantic checking or execution",
        )]);
    }
    let indexed = SnapshotOccurrences::new(snapshot, limits).map_err(|e| vec![e])?;
    crate::semantic::compile_mode(language, &indexed, limits, false).map_err(|e| vec![e])
}
pub fn compile_snapshot_recovering(
    language: &'static Language,
    snapshot: &ParseSnapshot,
    limits: Limits,
) -> RecoveredAnalysis {
    let mut diagnostics = snapshot.diagnostics().to_vec();
    if !std::ptr::eq(language, snapshot.language) {
        diagnostics.push(language_error());
        return RecoveredAnalysis {
            program: None,
            diagnostics,
        };
    }
    if language.profile == Profile::SyntaxOnly {
        return RecoveredAnalysis {
            program: None,
            diagnostics,
        };
    }
    let result = SnapshotOccurrences::new(snapshot, limits)
        .and_then(|indexed| crate::semantic::compile_mode(language, &indexed, limits, true));
    match result {
        Ok(program) => RecoveredAnalysis {
            diagnostics: program.diagnostics().to_vec(),
            program: Some(program),
        },
        Err(error) => {
            diagnostics.push(error);
            RecoveredAnalysis {
                program: None,
                diagnostics,
            }
        }
    }
}
