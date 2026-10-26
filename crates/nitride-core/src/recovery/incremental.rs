//! Shared recovery with a conservative compatibility fallback. Coverage, repairs,
//! diagnostics and damage use persistent relative records; public borrowed slices
//! are explicit lazy exports. Full-parse fallback remains visible in counters.
use crate::syntax::sidecars::Records;
use crate::syntax::{
    arena::{ArenaBuilder, Child},
    sequence::{Measured, Sequence},
};
use crate::*;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};
#[derive(Clone, PartialEq, Eq)]
enum RepairValue {
    Missing(Expected),
    Skipped,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct CoveragePiece {
    width: u32,
    kind: CoverageKind,
}
impl Measured for CoveragePiece {
    fn width(&self) -> u64 {
        u64::from(self.width)
    }
}
pub struct SnapshotRecovery {
    repairs: Records<RepairValue>,
    flat_repairs: OnceLock<Box<[Repair]>>,
    coverage: Sequence<CoveragePiece>,
    damaged: Records<u16>,
    pub(crate) structures: Arc<[(u16, u32, SubtreeId)]>,
}
impl SnapshotRecovery {
    pub(crate) fn visit_allocations(&self, visit: &mut crate::syntax::memory::Visitor<'_>) {
        use crate::syntax::memory::{self, Class};
        self.repairs.visit(visit);
        self.damaged.visit(visit);
        if let Some(flat) = self.flat_repairs.get().filter(|flat| !flat.is_empty()) {
            visit(
                Class::Sidecar,
                flat.as_ptr() as usize,
                std::mem::size_of_val(flat.as_ref()),
            );
        }
        let (id, size) = memory::arc(&self.structures);
        visit(Class::Sidecar, id, size);
        self.coverage.visit_allocations(Class::Sidecar, visit);
    }
    pub(crate) fn repair_count(&self) -> usize {
        self.repairs.len()
    }
    pub(crate) fn damage_count(&self) -> usize {
        self.damaged.len()
    }
    pub fn repairs(&self) -> &[Repair] {
        self.flat_repairs
            .get_or_init(|| self.iter_repairs().collect())
    }
    pub(crate) fn iter_repairs(&self) -> impl Iterator<Item = Repair> {
        self.repairs.iter().map(|(span, value)| match value {
            RepairValue::Missing(expected) => Repair::Missing {
                expected: *expected,
                at: span[0],
            },
            RepairValue::Skipped => Repair::Skipped { span },
        })
    }
    pub(crate) fn damaged(&self) -> impl Iterator<Item = (u16, [u32; 2])> {
        self.damaged.iter().map(|(span, rule)| (*rule, span))
    }
    pub(crate) fn set_records(
        &mut self,
        repairs: Vec<Repair>,
        damaged: Vec<(u16, [u32; 2])>,
        old: Option<&Self>,
    ) {
        self.repairs = Records::new(
            repairs.into_iter().map(|r| match r {
                Repair::Missing { expected, at } => ([at, at], RepairValue::Missing(expected)),
                Repair::Skipped { span } => (span, RepairValue::Skipped),
            }),
            old.map(|o| &o.repairs),
        );
        self.damaged = Records::new(
            damaged.into_iter().map(|(rule, span)| (span, rule)),
            old.map(|o| &o.damaged),
        );
        self.flat_repairs = OnceLock::new();
    }
    pub fn coverage(&self) -> impl Iterator<Item = Coverage> + '_ {
        let mut position = 0;
        self.coverage.iter().map(move |p| {
            let start = position;
            position += p.width;
            Coverage {
                span: [start, position],
                kind: p.kind,
            }
        })
    }
    /// Canonical coverage joins adjacent equal kinds, so one weighted lookup
    /// proves whether all bytes of a lexical occurrence were already retained.
    pub(crate) fn retained_span(&self, span: [u32; 2]) -> (bool, u64) {
        let units = u64::from(self.coverage.len().max(1).ilog2()) + 2;
        let retained = span[0] < span[1]
            && self
                .coverage
                .locate(u64::from(span[0]))
                .is_some_and(|(_, piece, offset)| {
                    piece.kind == CoverageKind::Retained
                        && u64::from(piece.width) - offset >= u64::from(span[1] - span[0])
                });
        (retained, units)
    }
    /// Replace one classified byte interval, preserving all distant blocks.
    /// Boundary neighbors join the splice so adjacent equal kinds stay canonical.
    pub(crate) fn splice_coverage(&self, range: [u32; 2], replacement: Vec<Coverage>) -> Self {
        let start = self.coverage.locate(u64::from(range[0]));
        let end = self.coverage.locate(u64::from(range[1]));
        let mut first = start.map_or(self.coverage.len(), |(i, _, _)| i);
        let mut last = end.map_or(self.coverage.len(), |(i, _, _)| i + 1);
        let mut pieces = Vec::new();
        if first > 0 {
            first -= 1;
            pieces.push(*self.coverage.get(first).unwrap());
        }
        if let Some((_, piece, offset)) = start {
            if offset > 0 {
                pieces.push(CoveragePiece {
                    width: offset as u32,
                    kind: piece.kind,
                });
            }
        }
        pieces.extend(replacement.into_iter().map(|c| CoveragePiece {
            width: c.span[1] - c.span[0],
            kind: c.kind,
        }));
        if let Some((_, piece, offset)) = end {
            if offset < u64::from(piece.width) {
                pieces.push(CoveragePiece {
                    width: piece.width - offset as u32,
                    kind: piece.kind,
                });
            }
        }
        if last < self.coverage.len() {
            pieces.push(*self.coverage.get(last).unwrap());
            last += 1;
        }
        let mut merged: Vec<CoveragePiece> = Vec::new();
        for piece in pieces {
            if piece.width == 0 {
                continue;
            }
            if let Some(previous) = merged.last_mut().filter(|p| p.kind == piece.kind) {
                previous.width += piece.width;
            } else {
                merged.push(piece)
            }
        }
        Self {
            repairs: Records::new([], None),
            flat_repairs: OnceLock::new(),
            damaged: Records::new([], None),
            structures: Arc::from([]),
            coverage: self.coverage.replace(first..last, merged),
        }
    }
    fn from_flat(tree: &SyntaxTree, previous: Option<&Self>) -> Self {
        let data = tree.recovery().expect("recovered metadata");
        Self::from_parts(
            data.repairs.clone(),
            data.coverage.clone(),
            data.damaged_nodes
                .iter()
                .filter_map(|id| tree.node(*id).map(|n| (n.rule, n.span)))
                .collect(),
            previous,
        )
    }
    pub(crate) fn from_parts(
        repairs: Vec<Repair>,
        coverage: Vec<Coverage>,
        damaged: Vec<(u16, [u32; 2])>,
        previous: Option<&Self>,
    ) -> Self {
        let entries: Vec<_> = coverage
            .iter()
            .map(|c| CoveragePiece {
                width: c.span[1] - c.span[0],
                kind: c.kind,
            })
            .collect();
        let coverage = if let Some(old) = previous {
            let prefix = entries
                .iter()
                .zip(old.coverage.iter())
                .take_while(|(a, b)| a == b)
                .count();
            let suffix = entries
                .iter()
                .rev()
                .zip(
                    (0..old.coverage.len())
                        .rev()
                        .map(|i| old.coverage.get(i).unwrap()),
                )
                .take(entries.len().min(old.coverage.len()) - prefix)
                .take_while(|(a, b)| a == b)
                .count();
            old.coverage.replace(
                prefix..old.coverage.len() - suffix,
                entries[prefix..entries.len() - suffix].to_vec(),
            )
        } else {
            Sequence::from_vec(entries)
        };
        let mut result = Self {
            structures: Arc::from([]),
            repairs: Records::new([], None),
            flat_repairs: OnceLock::new(),
            coverage,
            damaged: Records::new([], None),
        };
        result.set_records(repairs, damaged, previous);
        result
    }
}
fn mapped_span(span: [u32; 2], edit: &TextEdit) -> Option<[u32; 2]> {
    if span[1] < edit.range[0] {
        return Some(span);
    }
    if span[0] > edit.range[1] {
        let delta = edit.replacement.len() as i64 - i64::from(edit.range[1] - edit.range[0]);
        return Some([
            u32::try_from(i64::from(span[0]) + delta).ok()?,
            u32::try_from(i64::from(span[1]) + delta).ok()?,
        ]);
    }
    None
}
fn same_shape(old: NodeView<'_>, tree: &SyntaxTree, id: u32) -> bool {
    let mut pending = vec![(old, id)];
    while let Some((old, id)) = pending.pop() {
        let new = tree.nodes[id as usize];
        let span = old.span();
        if old.rule() != new.rule || span[1] - span[0] != new.span[1] - new.span[0] {
            return false;
        }
        let mut before = old.children();
        let after = tree.children(id);
        for edge in after {
            let Some((label, child)) = before.next() else {
                return false;
            };
            if label != tree.label(edge)
                || i64::from(child.span()[0]) - i64::from(span[0])
                    != i64::from(tree.nodes[edge.node as usize].span[0]) - i64::from(new.span[0])
            {
                return false;
            }
            pending.push((child, edge.node));
        }
        if before.next().is_some() {
            return false;
        }
    }
    true
}
pub(crate) fn recover_snapshot(
    language: &'static Language,
    text: &SourceText,
    limits: Limits,
    options: RecoveryOptions,
    previous: Option<(&ParseSnapshot, &TextEdit)>,
    strict_error: Option<Diagnostic>,
) -> Result<ParseSnapshot, Diagnostic> {
    if options.max_states > 0
        && strict_error
            .as_ref()
            .is_some_and(|e| e.category == "syntax")
    {
        let base = previous.map(|(old, _)| old);
        let index = previous.map_or_else(Default::default, |(old, edit)| {
            let delta = edit.replacement.len() as i64 - i64::from(edit.range[1] - edit.range[0]);
            old.recognitions.edit(edit.range, delta).0
        });
        let result = super::search::shared::recover(
            language,
            text,
            limits,
            options,
            base,
            index,
            previous.map(|(_, edit)| edit),
        );
        if let Ok(mut snapshot) = result {
            if !snapshot.diagnostics.has_category("limit") {
                snapshot.stats.source_bytes_copied = previous
                    .map_or(u64::from(text.len()), |(_, edit)| {
                        edit.replacement.len() as u64
                    });
                return Ok(snapshot);
            }
        }
    }
    let source = text.slice([0, text.len()])?;
    let parsed = match strict_error {
        Some(error) => super::recover_after_strict(language, &source, limits, options, error),
        None => parse_recovering(language, &source, limits, options),
    };
    let tree = &parsed.tree;
    let mut candidates: HashMap<(u16, [u32; 2]), Vec<NodeView<'_>>> = HashMap::new();
    if let Some((old, edit)) = previous {
        let mut pending = vec![old.root()];
        while let Some(node) = pending.pop() {
            if let Some(span) = mapped_span(node.span(), edit) {
                candidates
                    .entry((node.rule(), span))
                    .or_default()
                    .push(node);
                continue;
            }
            pending.extend(node.children().map(|(_, n)| n));
        }
    }
    let mut labels = previous.map_or_else(|| vec![""], |(old, _)| old.arena.labels.to_vec());
    let mut label_ids = Vec::new();
    for label in &tree.labels {
        let id = labels.iter().position(|s| s == label).unwrap_or_else(|| {
            labels.push(label);
            labels.len() - 1
        });
        label_ids.push(
            u16::try_from(id)
                .map_err(|_| Diagnostic::new("limit", [0, 0], "too many distinct labels"))?,
        );
    }
    let mut arena = previous.map_or_else(
        || ArenaBuilder::new(labels.clone().into()),
        |(old, _)| ArenaBuilder::from_published(&old.arena),
    );
    let mut mapped = vec![SubtreeId::NONE; tree.nodes.len()];
    let mut pending = vec![(tree.root, false)];
    let mut reused = 0;
    while let Some((id, visited)) = pending.pop() {
        if mapped[id as usize] != SubtreeId::NONE {
            continue;
        }
        let node = tree.nodes[id as usize];
        if !visited {
            if let Some(old) = candidates
                .get(&(node.rule, node.span))
                .and_then(|list| list.iter().find(|old| same_shape(**old, tree, id)))
            {
                mapped[id as usize] = old.subtree_id();
                reused += 1;
                continue;
            }
            pending.push((id, true));
            pending.extend(tree.children(id).iter().rev().map(|e| (e.node, false)));
        } else {
            let source_children = tree.children(id);
            let children = source_children.iter().enumerate().map(|(index, edge)| {
                let previous = if index == 0 {
                    node.span[0]
                } else {
                    tree.nodes[source_children[index - 1].node as usize].span[1]
                };
                Child {
                    node: mapped[edge.node as usize],
                    extent: tree.nodes[edge.node as usize].span[1] - previous,
                    label: label_ids[edge.label as usize],
                }
            });
            mapped[id as usize] =
                arena.add_reusing(node.rule, node.span[1] - node.span[0], children, None)?;
        }
    }
    let maintenance_debt = previous
        .map_or(0, |(old, _)| old.maintenance_debt)
        .saturating_add(arena.publication_allocation_bound());
    let mut arena = arena.finish();
    arena.labels = labels.into();
    let root = tree.nodes[tree.root as usize];
    let compact = previous.is_none()
        || maintenance_debt > 8 * 1024 * 1024
        || previous
            .is_some_and(|(old, _)| u64::from(text.len()) * 3 < u64::from(old.source.len()) * 2);
    let mut roots = [mapped[tree.root as usize]];
    let mut reclaimed = 0;
    if compact {
        let (next, relocation) = arena.compact(&mut roots)?;
        arena = next;
        reclaimed = relocation.removed_bytes;
    }
    let mut snapshot = ParseSnapshot::from_parts(
        language,
        text.clone(),
        arena,
        roots[0],
        root.span[0],
        SnapshotMode::Recovering(options),
    );
    if !compact {
        snapshot.maintenance_debt = maintenance_debt;
    }
    snapshot.stats.maintenance_passes = u64::from(compact && previous.is_some());
    snapshot.stats.reclaimed_syntax_bytes = reclaimed as u64;
    snapshot.diagnostics = Arc::new(crate::syntax::sidecars::DiagnosticStore::new(
        parsed.diagnostics,
        previous.map(|(old, _)| old.diagnostics.as_ref()),
    ));
    snapshot.recovery = tree.recovery().map(|_| {
        Arc::new(SnapshotRecovery::from_flat(
            tree,
            previous.and_then(|(old, _)| old.recovery()),
        ))
    });
    snapshot.stats.reused_subtrees = reused;
    snapshot.stats.full_parse_fallbacks = 1;
    snapshot.stats.work = parsed.stats.work;
    snapshot.stats.reparsed_bytes = u64::from(text.len());
    snapshot.stats.source_bytes_copied = u64::from(text.len());
    Ok(snapshot)
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    #[test]
    fn local_coverage_splice_merges_boundaries_and_shares_unchanged_blocks() {
        let entries = (0..10000)
            .map(|i| Coverage {
                span: [i, i + 1],
                kind: if i % 2 == 0 {
                    CoverageKind::Retained
                } else {
                    CoverageKind::Trivia
                },
            })
            .collect();
        let old = SnapshotRecovery::from_parts(vec![], entries, vec![], None);
        let next = old.splice_coverage(
            [5000, 5001],
            vec![Coverage {
                span: [5000, 5002],
                kind: CoverageKind::Trivia,
            }],
        );
        let mut expected: Vec<_> = old.coverage().take(5000).collect();
        expected.pop();
        expected.push(Coverage {
            span: [4999, 5003],
            kind: CoverageKind::Trivia,
        });
        expected.extend(old.coverage().skip(5002).map(|mut c| {
            c.span[0] += 1;
            c.span[1] += 1;
            c
        }));
        assert_eq!(next.coverage().collect::<Vec<_>>(), expected);
        assert!(old.coverage.shared_leaves(&next.coverage) > 250);
    }
}
