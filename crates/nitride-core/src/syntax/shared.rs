//! Immutable structure is separate from its occurrence in a source snapshot.
pub type ParseSnapshot = BorrowedParseSnapshot<'static>;
pub type NodeView<'a> = BorrowedNodeView<'a, 'static>;
#[cfg(test)]
#[path = "projection_tests.rs"]
mod projection_tests;
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
pub struct BorrowedParseSnapshot<'g> {
    pub(crate) tables: Option<Arc<crate::parser::shared::LanguageTables<'g>>>,
    pub(crate) dispatch: Option<Arc<crate::parser::shared::dispatch::Dispatch<'g>>>,
    pub(crate) language: &'g Language<'g>,
    pub(crate) source: SourceText,
    pub(crate) arena: PublishedArena<'g>,
    pub(crate) root: SubtreeId,
    pub(crate) start: u32,
    pub(crate) mode: SnapshotMode,
    pub(crate) diagnostics: Arc<DiagnosticStore>,
    pub(crate) recovery: Option<Arc<crate::BorrowedSnapshotRecovery<'g>>>,
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
pub struct BorrowedNodeView<'a, 'g> {
    snapshot: &'a BorrowedParseSnapshot<'g>,
    id: SubtreeId,
    start: u32,
}
impl<'g> BorrowedParseSnapshot<'g> {
    pub(crate) fn from_parts(
        language: &'g Language<'g>,
        source: SourceText,
        arena: PublishedArena<'g>,
        root: SubtreeId,
        start: u32,
        mode: SnapshotMode,
    ) -> Self {
        Self {
            tables: None,
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
    pub fn root(&self) -> BorrowedNodeView<'_, 'g> {
        BorrowedNodeView {
            snapshot: self,
            id: self.root,
            start: self.start,
        }
    }
    pub fn recovery(&self) -> Option<&crate::BorrowedSnapshotRecovery<'g>> {
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
        // `dispatch` is the language's analysis, shared by every parse of it
        // for the life of the process: not storage this snapshot owns.
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
        let cancellation = crate::cancellation::Poll::new();
        let mut reader = self.arena.reader();
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
        let mut polling_work = 0_u64;
        while let Some((view, visited)) = pending.pop() {
            work = work.saturating_add(1);
            let node = reader.node(view.id).expect("reachable node");
            let span = [view.start, view.start + node.width];
            polling_work = polling_work.saturating_add(1);
            cancellation.check(polling_work, span)?;
            if work > limit {
                return Err(Diagnostic::new(
                    "limit",
                    span,
                    "occurrence indexing work limit exceeded",
                ));
            }
            if !visited {
                pending.push((view, true));
                let base = pending.len();
                let mut position = view.start;
                for child in reader.children(view.id) {
                    polling_work = polling_work.saturating_add(1);
                    cancellation.check(polling_work, span)?;
                    position += child.extent;
                    let start = position - reader.node(child.node).expect("owned child").width;
                    pending.push((
                        BorrowedNodeView {
                            snapshot: self,
                            id: child.node,
                            start,
                        },
                        false,
                    ));
                }
                // Pop children in source order without a temporary child vector.
                pending[base..].reverse();
            } else {
                let child_count = node.child_count();
                let base = completed.len() - child_count as usize;
                let first_child = edges.len() as u32;
                for (child, id) in reader.children(view.id).zip(completed.drain(base..)) {
                    polling_work = polling_work.saturating_add(1);
                    cancellation.check(polling_work, span)?;
                    edges.push(Edge {
                        label: child.label,
                        node: id,
                    });
                }
                let id = nodes.len() as u32;
                if nodes.len() >= u32::MAX as usize || edges.len() > u32::MAX as usize {
                    return Err(Diagnostic::new(
                        "limit",
                        span,
                        "syntax storage capacity exceeded",
                    ));
                }
                if damaged.contains(&(node.rule, span)) {
                    damaged_nodes.push(id);
                }
                nodes.push(Node {
                    rule: node.rule,
                    span,
                    first_child,
                    child_count,
                });
                completed.push(id);
            }
        }
        crate::cancellation::checkpoint([0, self.source.len()])?;
        Ok(Occurrences {
            nodes,
            edges,
            root: completed[0],
            damaged_nodes,
        })
    }
    /// Visit occurrences in flat-tree postorder, without exporting source,
    /// labels or recovery coverage. Child IDs and node offsets match
    /// `materialize`; the child slice is scoped to one callback. Each node
    /// visit and each scheduled child consumes a work unit; a node's children
    /// must all fit the remaining budget before expansion. An error may
    /// follow partial visitation, so consumers must discard partial output.
    pub fn visit_postorder(
        &self,
        limit: u64,
        visit: impl FnMut(u32, Node, &[u32]),
    ) -> Result<u32, Diagnostic> {
        let cancellation = crate::cancellation::Poll::new();
        if cancellation.enabled() {
            self.visit_postorder_inner::<true>(limit, visit, &cancellation)
        } else {
            self.visit_postorder_inner::<false>(limit, visit, &cancellation)
        }
    }
    fn visit_postorder_inner<const CANCEL: bool>(
        &self,
        limit: u64,
        mut visit: impl FnMut(u32, Node, &[u32]),
        cancellation: &crate::cancellation::Poll,
    ) -> Result<u32, Diagnostic> {
        let mut reader = self.arena.reader();
        let root = reader.node(self.root).expect("reachable root");
        if CANCEL {
            cancellation.check_unit(0, [self.start, self.start + root.width])?;
        }
        let mut pending = vec![(self.root, root, self.start, false)];
        let mut completed = Vec::new();
        let mut work = 0_u64;
        let mut nodes = 0_u64;
        let mut edges = 0_u64;
        while let Some((id, node, start, visited)) = pending.pop() {
            work = work.saturating_add(1);
            let span = [start, start + node.width];
            if CANCEL {
                cancellation.check_unit(work, span)?;
            }
            if work > limit {
                return Err(Diagnostic::new(
                    "limit",
                    span,
                    "occurrence projection work limit exceeded",
                ));
            }
            if !visited {
                let scheduled_work = work.saturating_add(u64::from(node.child_count()));
                if scheduled_work > limit {
                    return Err(Diagnostic::new(
                        "limit",
                        span,
                        "occurrence projection child scheduling work limit exceeded",
                    ));
                }
                let scheduling_base = work;
                work = scheduled_work;
                if node.child_count() != 0 {
                    pending.push((id, node, start, true));
                    let base = pending.len();
                    let mut position = start;
                    for (offset, child) in reader.children(id).enumerate() {
                        if CANCEL {
                            cancellation.check_unit(scheduling_base + offset as u64 + 1, span)?;
                        }
                        let record = reader.node(child.node).expect("owned child");
                        position += child.extent;
                        pending.push((child.node, record, position - record.width, false));
                    }
                    pending[base..].reverse();
                    continue;
                }
                // Elide an empty child iterator and leaf exit frame, but keep
                // the exit's logical work charge and failure position.
                work = work.saturating_add(1);
                if CANCEL {
                    cancellation.check_unit(work, span)?;
                }
                if work > limit {
                    return Err(Diagnostic::new(
                        "limit",
                        span,
                        "occurrence projection work limit exceeded",
                    ));
                }
            }
            let count = node.child_count();
            if nodes >= u64::from(u32::MAX) || edges + u64::from(count) > u64::from(u32::MAX) {
                return Err(Diagnostic::new(
                    "limit",
                    span,
                    "syntax storage capacity exceeded",
                ));
            }
            let base = completed.len() - count as usize;
            visit(
                nodes as u32,
                Node {
                    rule: node.rule,
                    span,
                    first_child: edges as u32,
                    child_count: count,
                },
                &completed[base..],
            );
            completed.truncate(base);
            completed.push(nodes as u32);
            nodes += 1;
            edges += u64::from(count);
        }
        if CANCEL {
            crate::cancellation::checkpoint([self.start, self.start + root.width])?;
        }
        Ok(completed[0])
    }

    pub fn materialize(&self) -> Result<SyntaxTree<'g>, Diagnostic> {
        let indexed = self.indexed(u64::MAX)?;
        let tree = SyntaxTree {
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
        };
        crate::cancellation::checkpoint([0, self.source.len()])?;
        Ok(tree)
    }
}

/// Unique requested dynamic bytes for a collection of live snapshots.
pub fn shared_storage_bytes<'g>(snapshots: &[&BorrowedParseSnapshot<'g>]) -> usize {
    let mut counter = super::memory::Counter::default();
    for snapshot in snapshots {
        snapshot.visit_allocations(&mut |class, id, size| counter.add(class, id, size));
    }
    counter.total()
}
impl<'a, 'g> BorrowedNodeView<'a, 'g> {
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
    pub fn children(&self) -> impl Iterator<Item = (&'g str, BorrowedNodeView<'a, 'g>)> + 'a {
        let snapshot = self.snapshot;
        let mut position = self.start;
        snapshot.arena.children(self.id).map(move |edge| {
            position += edge.extent;
            let start = position - snapshot.arena.node(edge.node).expect("owned child").width;
            (
                snapshot.arena.labels[edge.label as usize],
                BorrowedNodeView {
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
    pub(super) fn language() -> &'static Language<'static> {
        Box::leak(Box::new(Language {
            trivia: crate::TriviaPolicy::Nitrogen,
            profile: Profile::SyntaxOnly,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            rule_checks: &[],
            patterns: &[],
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
                    origin: None,
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
        let mut projected = Vec::new();
        let mut child_ids = Vec::new();
        let root = snapshot
            .visit_postorder(u64::MAX, |id, node, children| {
                assert_eq!(id as usize, projected.len());
                projected.push((node.rule, node.span, node.first_child, node.child_count));
                child_ids.extend_from_slice(children);
            })
            .unwrap();
        assert_eq!(root, flat.root);
        assert_eq!(
            projected,
            flat.nodes
                .iter()
                .map(|n| (n.rule, n.span, n.first_child, n.child_count))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            child_ids,
            flat.edges.iter().map(|e| e.node).collect::<Vec<_>>()
        );
        let mut emitted = 0;
        assert!(snapshot.visit_postorder(0, |_, _, _| emitted += 1).is_err());
        assert_eq!(emitted, 0);
        let mut partial = 0;
        assert!(snapshot.visit_postorder(7, |_, _, _| partial += 1).is_err());
        assert_eq!(partial, 2);
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

    #[test]
    fn wide_projection_rejects_child_scheduling_before_expansion() {
        const CHILDREN: u32 = 4096;
        let mut arena = ArenaBuilder::new(vec![""].into());
        let leaf = arena.add(0, 1, vec![]).unwrap();
        let root = arena
            .add(
                0,
                CHILDREN,
                (0..CHILDREN)
                    .map(|_| Child {
                        node: leaf,
                        extent: 1,
                        label: 0,
                    })
                    .collect(),
            )
            .unwrap();
        let snapshot = ParseSnapshot::from_parts(
            language(),
            SourceText::new(&"x".repeat(CHILDREN as usize)).unwrap(),
            arena.finish(),
            root,
            0,
            SnapshotMode::Strict(ParseOptions::default()),
        );
        let error = snapshot
            .visit_postorder(1, |_, _, _| panic!("no completed node fits"))
            .unwrap_err();
        assert_eq!(error.span, [0, CHILDREN]);
        assert!(error.message.contains("scheduling"));
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let token = crate::CancellationToken::from_predicate(move || {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 1
        });
        let error = crate::with_cancellation(&token, || snapshot.indexed(u64::MAX))
            .err()
            .unwrap();
        assert_eq!(error.category, "cancelled");
        assert_eq!(
            error.span,
            [0, CHILDREN],
            "cancel during wide-root scheduling, before visiting a leaf"
        );
        let mut emitted = 0;
        crate::syntax::arena::reset_record_reads();
        let root_id = snapshot
            .visit_postorder(u64::from(CHILDREN + 1) * 3, |id, node, children| {
                assert_eq!(id, emitted);
                if id < CHILDREN {
                    assert_eq!(node.span, [id, id + 1]);
                    assert!(children.is_empty());
                } else {
                    assert_eq!(children, (0..CHILDREN).collect::<Vec<_>>());
                }
                emitted += 1;
            })
            .unwrap();
        assert_eq!(root_id, CHILDREN);
        assert_eq!(emitted, CHILDREN + 1);
        assert!(
            crate::syntax::arena::record_reads() <= u64::from(CHILDREN + 1) * 2,
            "projection should resolve at most two records per occurrence"
        );
        let cancelled = crate::CancellationToken::new();
        cancelled.cancel();
        let error = crate::with_cancellation(&cancelled, || {
            snapshot.visit_postorder(u64::MAX, |_, _, _| panic!("cancelled at entry"))
        })
        .unwrap_err();
        assert_eq!(
            error.span,
            [0, CHILDREN],
            "preserve the full root cancellation span"
        );
        let token = crate::CancellationToken::new();
        let result = crate::with_cancellation(&token, || {
            snapshot.visit_postorder(u64::MAX, |id, _, _| {
                if id == CHILDREN {
                    token.cancel();
                }
            })
        });
        assert_eq!(
            result.expect_err("cancelled at final callback").category,
            "cancelled"
        );
    }
}
