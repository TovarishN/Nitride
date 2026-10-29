//! Immutable structure is separate from its occurrence in a source snapshot.
use super::{SourceText, SubtreeId, arena::PublishedArena, sidecars::DiagnosticStore};
use crate::{Diagnostic, Edge, Language, Node, ParseOptions, RecoveryOptions, SyntaxTree};
use std::{borrow::Cow, sync::Arc};

#[derive(Debug, Clone, Copy)]
pub enum SnapshotMode {
    Strict(ParseOptions),
    Recovering(RecoveryOptions),
}
/// Counters for the parse/edit operation that produced this snapshot. Explicit
/// later materialization does not mutate these immutable operation counters.
/// If full_parse_fallbacks is nonzero, copy counters
/// exclude uninstrumented legacy parser work; the profiler reports them as unknown.
#[derive(Debug, Default, Clone)]
pub struct SnapshotStats {
    pub work: u64,
    pub memo_hits: u64,
    /// Replayed outer lexical recognitions in the bounded parse-local cache.
    pub lexical_cache_hits: u64,
    /// Parse-local ordinary-choice selector preparations.
    pub choice_plan_builds: u64,
    /// Ordinary alternatives rejected by conservative first-primitive selectors.
    pub choice_skips: u64,
    /// Outer lexical recognitions executed by generated scanners.
    pub native_lexical_calls: u64,
    /// Logical expression/rule checks performed by generated scanners.
    pub native_lexical_work: u64,
    /// Generated ordinary syntax-body invocations.
    pub native_syntax_calls: u64,
    /// Generated infix-tail invocations; winner selection stays in the runtime.
    pub native_syntax_tails: u64,
    /// Native trivia exits avoiding proven-impossible comment comparisons.
    pub trivia_fast_skips: u64,
    pub memo_descendants_copied: u64,
    pub winner_descendants_copied: u64,
    pub probe_descendants_copied: u64,
    pub export_nodes_copied: u64,
    pub reused_subtrees: u64,
    /// New-source bytes not covered by any reused recognition, counted once.
    /// Work also includes lookahead/backtracking reads into already covered text.
    pub reparsed_bytes: u64,
    pub source_bytes_copied: u64,
    pub syntax_bytes_copied: u64,
    pub full_parse_fallbacks: u64,
    pub recovery_batches: u64,
    /// Budget units for recovery sidecar indexing, ordering and source coverage.
    pub recovery_metadata_work: u64,
    /// Child payload bytes placed in recovery construction scratch buffers.
    pub recovery_child_bytes: u64,
    /// Source bytes whose recovery coverage classifications were recomputed.
    pub recovery_coverage_bytes: u64,
    pub cache_evictions: u64,
    pub maintenance_passes: u64,
    pub reclaimed_syntax_bytes: u64,
}
#[derive(Debug, Default, Clone, Copy)]
pub struct StorageStats {
    pub syntax_payload_bytes: usize,
    pub source_buffer_bytes: usize,
    pub chunks: usize,
    pub physical_retained_bytes: usize,
    pub syntax_bytes: usize,
    pub source_bytes: usize,
    pub index_bytes: usize,
    pub sidecar_bytes: usize,
    pub unique_allocations: usize,
}
#[derive(Clone)]
pub struct ParseSnapshot {
    pub(crate) dispatch: Option<Arc<crate::parser::shared::dispatch::Dispatch>>,
    pub(crate) language: &'static Language<'static>,
    pub(crate) source: SourceText,
    pub(crate) arena: PublishedArena,
    pub(crate) root: SubtreeId,
    pub(crate) start: u32,
    pub(crate) mode: SnapshotMode,
    pub(crate) diagnostics: Arc<DiagnosticStore>,
    pub(crate) recovery: Option<Arc<crate::SnapshotRecovery>>,
    pub(crate) stats: SnapshotStats,
    pub(crate) maintenance_debt: usize,
    pub(crate) maintenance_base: usize,
    pub(crate) recognitions: crate::incremental::dependencies::Index,
}
pub(crate) struct Occurrences {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub root: u32,
    pub damaged_nodes: Vec<u32>,
}
#[derive(Clone, Copy)]
pub struct NodeView<'a> {
    snapshot: &'a ParseSnapshot,
    id: SubtreeId,
    start: u32,
}
impl ParseSnapshot {
    pub(crate) fn from_parts(
        language: &'static Language<'static>,
        source: SourceText,
        arena: PublishedArena,
        root: SubtreeId,
        start: u32,
        mode: SnapshotMode,
    ) -> Self {
        Self {
            dispatch: None,
            maintenance_debt: 0,
            maintenance_base: arena.payload_bytes(),
            language,
            source,
            arena,
            root,
            start,
            mode,
            diagnostics: Arc::new(DiagnosticStore::new(vec![], None)),
            recovery: None,
            stats: SnapshotStats::default(),
            recognitions: Default::default(),
        }
    }
    pub fn root(&self) -> NodeView<'_> {
        NodeView {
            snapshot: self,
            id: self.root,
            start: self.start,
        }
    }
    pub fn recovery(&self) -> Option<&crate::SnapshotRecovery> {
        self.recovery.as_deref()
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.diagnostics.as_slice()
    }
    /// Count without exporting the borrowed diagnostic slice.
    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics.len()
    }
    pub fn stats(&self) -> &SnapshotStats {
        &self.stats
    }
    pub(crate) fn visit_allocations(&self, visit: &mut super::memory::Visitor<'_>) {
        use super::memory::{self, Class};
        self.arena.visit_allocations(visit);
        self.source.visit_allocations(visit);
        self.recognitions.visit_allocations(visit);
        if let Some(dispatch) = &self.dispatch {
            let (id, size) = memory::arc(dispatch);
            if visit(Class::Index, id, size) {
                dispatch.visit_allocations(visit);
            }
        }
        let (id, size) = memory::arc(&self.diagnostics);
        if visit(Class::Sidecar, id, size) {
            self.diagnostics.visit(visit);
        }
        if let Some(recovery) = &self.recovery {
            let (id, size) = memory::arc(recovery);
            if visit(Class::Sidecar, id, size) {
                recovery.visit_allocations(visit);
            }
        }
    }
    pub fn storage_stats(&self) -> StorageStats {
        let mut counter = super::memory::Counter::default();
        self.visit_allocations(&mut |class, id, size| counter.add(class, id, size));
        StorageStats {
            syntax_payload_bytes: self.arena.payload_bytes(),
            source_buffer_bytes: self.source.retained_buffer_bytes(),
            chunks: self.arena.chunk_count(),
            physical_retained_bytes: counter.total(),
            syntax_bytes: counter.syntax,
            source_bytes: counter.source,
            index_bytes: counter.index,
            sidecar_bytes: counter.sidecar,
            unique_allocations: counter.allocations(),
        }
    }
    pub(crate) fn indexed(&self, limit: u64) -> Result<Occurrences, Diagnostic> {
        let mut pending = vec![(self.root(), false)];
        let mut completed: Vec<u32> = Vec::new();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let damaged: std::collections::HashSet<_> = self
            .recovery
            .as_ref()
            .map_or_else(Default::default, |r| r.damaged().collect());
        let mut damaged_nodes = Vec::new();
        let mut work = 0_u64;
        while let Some((view, visited)) = pending.pop() {
            work = work.saturating_add(1);
            if work > limit {
                return Err(Diagnostic::new(
                    "limit",
                    view.span(),
                    "occurrence indexing work limit exceeded",
                ));
            }
            if !visited {
                pending.push((view, true));
                let children: Vec<_> = view.children().collect();
                pending.extend(children.into_iter().rev().map(|(_, c)| (c, false)));
            } else {
                let children: Vec<_> = view.snapshot.arena.children(view.id).collect();
                let base = completed.len() - children.len();
                let first_child = edges.len() as u32;
                for (child, id) in children.iter().zip(completed.drain(base..)) {
                    edges.push(Edge {
                        label: child.label,
                        node: id,
                    });
                }
                let id = nodes.len() as u32;
                if nodes.len() >= u32::MAX as usize || edges.len() > u32::MAX as usize {
                    return Err(Diagnostic::new(
                        "limit",
                        view.span(),
                        "syntax storage capacity exceeded",
                    ));
                }
                if damaged.contains(&(view.rule(), view.span())) {
                    damaged_nodes.push(id);
                }
                nodes.push(Node {
                    rule: view.rule(),
                    span: view.span(),
                    first_child,
                    child_count: children.len() as u32,
                });
                completed.push(id);
            }
        }
        Ok(Occurrences {
            nodes,
            edges,
            root: completed[0],
            damaged_nodes,
        })
    }
    pub fn materialize(&self) -> Result<SyntaxTree<'static>, Diagnostic> {
        let indexed = self.indexed(u64::MAX)?;
        Ok(SyntaxTree {
            language: self.language,
            source: self.source.slice([0, self.source.len()])?.into_owned(),
            nodes: indexed.nodes,
            edges: indexed.edges,
            root: indexed.root,
            labels: self.arena.labels.to_vec(),
            recovery: self.recovery.as_ref().map(|r| {
                Box::new(crate::RecoveryData {
                    repairs: r.repairs().to_vec(),
                    coverage: r.coverage().collect(),
                    diagnostics: self.diagnostics().to_vec(),
                    damaged_nodes: indexed.damaged_nodes,
                })
            }),
        })
    }
}

/// Unique requested dynamic bytes for a collection of live snapshots.
pub fn shared_storage_bytes(snapshots: &[&ParseSnapshot]) -> usize {
    let mut counter = super::memory::Counter::default();
    for snapshot in snapshots {
        snapshot.visit_allocations(&mut |class, id, size| counter.add(class, id, size));
    }
    counter.total()
}
impl<'a> NodeView<'a> {
    pub fn subtree_id(&self) -> SubtreeId {
        self.id
    }
    pub fn rule(&self) -> u16 {
        self.snapshot
            .arena
            .node(self.id)
            .expect("reachable node")
            .rule
    }
    pub fn span(&self) -> [u32; 2] {
        [
            self.start,
            self.start
                + self
                    .snapshot
                    .arena
                    .node(self.id)
                    .expect("reachable node")
                    .width,
        ]
    }
    pub fn text(&self) -> Cow<'a, str> {
        self.snapshot
            .source
            .slice(self.span())
            .expect("validated syntax span")
    }
    pub fn children(&self) -> impl Iterator<Item = (&'static str, NodeView<'a>)> + 'a {
        let snapshot = self.snapshot;
        let mut position = self.start;
        snapshot.arena.children(self.id).map(move |edge| {
            position += edge.extent;
            let start = position - snapshot.arena.node(edge.node).expect("owned child").width;
            (
                snapshot.arena.labels[edge.label as usize],
                NodeView {
                    snapshot,
                    id: edge.node,
                    start,
                },
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::arena::{ArenaBuilder, Child};
    use crate::{Action, Expr, Profile, Rule};
    fn language() -> &'static Language<'static> {
        Box::leak(Box::new(Language {
            profile: Profile::SyntaxOnly,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            rule_checks: &[],
            source_hash: "fixture",
            rules: Box::leak(
                vec![Rule {
                    name: "Root",
                    expr: Expr::Literal("x"),
                    precedence: 0,
                    owner: None,
                    lexical: false,
                    except: &[],
                    right_associative: false,
                    transparent: false,
                    alias: false,
                    action: Action::None,
                }]
                .into_boxed_slice(),
            ),
        }))
    }
    #[test]
    fn same_subtree_two_placements_and_flat_export_is_postorder() {
        let language = language();
        let mut arena = ArenaBuilder::new(vec!["", "Item"].into());
        let leaf = arena.add(0, 1, vec![]).unwrap();
        let root = arena
            .add(
                0,
                3,
                vec![
                    Child {
                        node: leaf,
                        extent: 1,
                        label: 1,
                    },
                    Child {
                        node: leaf,
                        extent: 2,
                        label: 1,
                    },
                ],
            )
            .unwrap();
        let snapshot = ParseSnapshot::from_parts(
            language,
            SourceText::new("x x").unwrap(),
            arena.finish(),
            root,
            0,
            SnapshotMode::Strict(ParseOptions::default()),
        );
        let children: Vec<_> = snapshot.root().children().collect();
        assert_eq!(children[0].0, "Item");
        assert_eq!(children[0].1.subtree_id(), children[1].1.subtree_id());
        assert_eq!(children[0].1.span(), [0, 1]);
        assert_eq!(children[1].1.span(), [2, 3]);
        assert_eq!(children[1].1.text(), "x");
        let flat = snapshot.materialize().unwrap();
        assert_eq!(flat.nodes.len(), 3);
        assert_eq!(flat.root, 2);
        assert_eq!(flat.nodes[1].span, [2, 3]);
        assert_eq!(flat.field(2, "Item"), Some(0));
        for (id, node) in flat.nodes.iter().enumerate() {
            assert!(
                flat.children(id as u32)
                    .iter()
                    .all(|edge| edge.node < id as u32)
            );
            assert!(node.span[1] <= 3);
        }
        fn send_sync<T: Send + Sync>() {}
        send_sync::<ParseSnapshot>();
    }
}
