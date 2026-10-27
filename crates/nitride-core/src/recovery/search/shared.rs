//! The recovery VM chooses the same events as the flat backend. Strict events
//! attach structural handles; only recovery-created ancestors are constructed.
use super::*;
use crate::incremental::dependencies::Index;
use crate::parser::shared::{Children, Handle, Seed, SharedProbe};
use std::sync::Arc;
impl Probe<'static> for SharedProbe<'_> {
    fn selected(&mut self, root: u32) {
        self.selected(root)
    }
    fn rule(&mut self, id: u16, at: usize, power: u32) -> Result<Option<ProbeHit>, Diagnostic> {
        self.rule(id, at, power)
    }
    fn predicate(&mut self, expr: &'static Expr<'static>, at: usize) -> Result<bool, Diagnostic> {
        self.predicate(expr, at)
    }
    fn commit(&mut self, roots: &mut [u32], keep_from: usize) {
        self.commit(roots, keep_from)
    }
    fn work(&self) -> u64 {
        self.work()
    }
    fn reusable_rule(&mut self, id: u16, at: usize) -> Result<Option<ProbeHit>, Diagnostic> {
        self.reusable_rule(id, at)
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn recover<'a>(
    language: &'static Language<'static>,
    text: &'a SourceText,
    limits: Limits,
    options: RecoveryOptions,
    base: Option<&ParseSnapshot>,
    index: Index,
    edit: Option<&TextEdit>,
    seed: Option<&Seed<'a>>,
) -> Result<ParseSnapshot, Diagnostic> {
    let result = recover_once(
        language,
        text,
        limits,
        options,
        base,
        index.clone(),
        edit,
        seed,
    );
    let exhausted = match &result {
        Ok(snapshot) => snapshot
            .diagnostics
            .has_message("parser stack limit exceeded"),
        Err(error) => error.message == "parser stack limit exceeded",
    };
    if exhausted && limits.parser_spill_stack_bytes > limits.parser_stack_bytes {
        drop(result);
        let spill = Limits {
            parser_stack_bytes: limits.parser_spill_stack_bytes
                - limits.parser_spill_stack_bytes / 8,
            ..limits
        };
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("nitride-shared-recovery".into())
                .stack_size(limits.parser_spill_stack_bytes)
                .spawn_scoped(scope, move || {
                    recover_once(language, text, spill, options, base, index, edit, seed)
                })
                .map_err(|_| Diagnostic::new("limit", [0, 0], "cannot start a deep-parse thread"))?
                .join()
                .unwrap_or_else(|_| Err(Diagnostic::new("limit", [0, 0], "deep recovery failed")))
        })
    } else {
        result
    }
}
#[allow(clippy::too_many_arguments)]
fn recover_once<'a>(
    language: &'static Language<'static>,
    text: &'a SourceText,
    limits: Limits,
    options: RecoveryOptions,
    base: Option<&ParseSnapshot>,
    index: Index,
    edit: Option<&TextEdit>,
    seed: Option<&Seed<'a>>,
) -> Result<ParseSnapshot, Diagnostic> {
    let mut engine = engine(
        language,
        Source::Shared(text),
        limits,
        options,
        SharedProbe::new(language, text, limits, base, index, seed),
    );
    let (state, failure) = match engine.run() {
        Ok(state) => (state, None),
        Err(error) => {
            let Some(state) = engine.best.clone() else {
                return Err(error);
            };
            (state, Some(error))
        }
    };
    engine.finish_shared(text, state, failure, base, edit)
}
struct SharedFrame {
    id: u16,
    start: usize,
    label: u16,
    children: Children,
}
fn construct(
    probe: &mut SharedProbe<'_>,
    id: u16,
    start: usize,
    end: usize,
    children: &Children,
) -> Result<Handle, Diagnostic> {
    let node = probe.recovery_node(id, start as u32, end as u32, children)?;
    Ok(Handle {
        node,
        start: start as u32,
        width: (end - start) as u32,
    })
}
fn close(
    probe: &mut SharedProbe<'_>,
    frames: &mut Vec<SharedFrame>,
    end: usize,
    language: &Language,
) -> Result<(), Diagnostic> {
    if frames.len() < 2 {
        return Ok(());
    }
    let frame = frames.pop().unwrap();
    let rule = &language.rules[frame.id as usize];
    if rule.alias && frame.children.is_empty() && rule.owner.is_none() {
        return Ok(());
    }
    let node = if (rule.transparent || rule.alias) && frame.children.len() == 1 {
        frame.children.first_node().unwrap()
    } else {
        construct(probe, frame.id, frame.start, end, &frame.children)?
    };
    probe.recovery_push(&mut frames.last_mut().unwrap().children, frame.label, node);
    Ok(())
}
impl Engine<'static, '_, SharedProbe<'_>> {
    fn finish_shared(
        mut self,
        text: &SourceText,
        state: State,
        failure: Option<Diagnostic>,
        base: Option<&ParseSnapshot>,
        edit: Option<&TextEdit>,
    ) -> Result<ParseSnapshot, Diagnostic> {
        let mut events = Vec::new();
        let mut history = state.history;
        while history > 0 {
            let (previous, event) = self.history[history as usize];
            events.push(event);
            history = previous;
        }
        events.reverse();
        let mut frames = vec![SharedFrame {
            id: self.language.start,
            start: 0,
            label: 0,
            children: self.probe.recovery_children(self.language.start, 0)?,
        }];
        let mut repairs = Vec::new();
        let mut exhausted = Vec::new();
        for event in events {
            match event {
                Event::Exhausted { at } => exhausted.push(Diagnostic::new(
                    "limit",
                    [at as u32; 2],
                    "local recovery search exhausted; resumed at a grammar continuation",
                )),
                Event::Open { id, start, label } => {
                    let label = self.probe.label(label)?;
                    frames.push(SharedFrame {
                        id,
                        start,
                        label,
                        children: self.probe.recovery_children(id, start as u32)?,
                    });
                }
                Event::Close { end } => close(&mut self.probe, &mut frames, end, self.language)?,
                Event::Strict { root, label } => {
                    if let Some(node) = self.probe.handle(root) {
                        let label = self.probe.label(label)?;
                        self.probe.recovery_push(
                            &mut frames.last_mut().unwrap().children,
                            label,
                            node,
                        );
                    }
                }
                Event::Missing { expected, at } => repairs.push(Repair::Missing {
                    expected,
                    at: at as u32,
                }),
                Event::Skip { start, end } => repairs.push(Repair::Skipped {
                    span: [start as u32, end as u32],
                }),
                Event::Wrap {
                    id,
                    start,
                    left_label,
                    label,
                } => {
                    let label = self.probe.label(label)?;
                    let left_label = self.probe.label(left_label)?;
                    let child = frames
                        .last_mut()
                        .and_then(|f| self.probe.recovery_pop(&mut f.children));
                    let mut children = self.probe.recovery_children(id, start as u32)?;
                    if let Some(node) = child {
                        self.probe.recovery_push(&mut children, left_label, node);
                    }
                    frames.push(SharedFrame {
                        id,
                        start,
                        label,
                        children,
                    });
                }
            }
        }
        if failure.is_some() && state.position < self.source.len() {
            repairs.push(Repair::Skipped {
                span: [state.position as u32, self.source.len() as u32],
            });
        }
        while frames.len() > 1 {
            close(&mut self.probe, &mut frames, state.position, self.language)?;
        }
        let roots = frames.pop().unwrap().children;
        let mut root = if roots.len() == 1 {
            roots.first_node().unwrap()
        } else {
            construct(
                &mut self.probe,
                self.language.start,
                0,
                self.source.len(),
                &roots,
            )?
        };
        if root.start != 0 || root.width != self.source.len() as u32 {
            let stored = self.probe.arena().node(root.node).unwrap();
            let children = self.probe.recovery_root_children(root);
            root = construct(
                &mut self.probe,
                stored.rule,
                0,
                self.source.len(),
                &children,
            )?;
        }
        let recognition_work = self.probe.work().saturating_add(self.stats.work);
        let mut metadata = MetadataWork::new(
            self.budget().saturating_sub(recognition_work),
            state.position as u32,
        );
        let mut diagnostics = repair_diagnostics_source(&repairs, self.source, &mut metadata)?;
        diagnostics.extend(exhausted);
        if let Some(error) = failure {
            diagnostics.push(error)
        }
        if diagnostics.is_empty() {
            diagnostics.push(Diagnostic::new(
                "syntax",
                [state.position as u32; 2],
                "incomplete syntax",
            ));
        }
        let (mut snapshot, recovery_keys) =
            self.probe
                .finish(text, root, base, SnapshotMode::Recovering(self.options))?;
        metadata.charge((repairs.len() as u64).saturating_mul(8))?;
        let old_recovery = base.and_then(|b| b.recovery());
        if let Some(old) = old_recovery {
            metadata.charge(
                (old.repair_count() as u64)
                    .saturating_mul(6)
                    .saturating_add(old.damage_count() as u64),
            )?;
        }
        let old_damage = SpanIndex::new(
            old_recovery.map_or_else(Vec::new, |r| r.damaged().map(|(_, span)| span).collect()),
            &mut metadata,
        )?;
        let old_skipped = SpanIndex::new(
            old_recovery.map_or_else(Vec::new, |r| {
                r.iter_repairs()
                    .filter_map(|r| match r {
                        Repair::Skipped { span } => Some(span),
                        _ => None,
                    })
                    .collect()
            }),
            &mut metadata,
        )?;
        let new_skipped = SpanIndex::new(
            repairs
                .iter()
                .filter_map(|r| match r {
                    Repair::Skipped { span } => Some(*span),
                    _ => None,
                })
                .collect(),
            &mut metadata,
        )?;
        let missing: HashSet<_> = repairs
            .iter()
            .filter_map(|r| match r {
                Repair::Missing {
                    expected: Expected::Rule(rule),
                    at,
                } => Some((*rule, *at)),
                _ => None,
            })
            .collect();
        let mut lexical = Vec::new();
        let mut structures = Vec::new();
        let mut damaged = Vec::new();
        let mut reused = Vec::new();
        let mut stack = vec![snapshot.root()];
        while let Some(node) = stack.pop() {
            metadata.charge(1)?;
            let span = node.span();
            if recovery_keys.contains(&(node.rule(), span)) {
                structures.push((node.rule(), span[0], node.subtree_id()));
            }
            if let Some(old) = base.filter(|b| b.recovery().is_some()) {
                if span[0] < span[1] && old.arena.node(node.subtree_id()).is_some() {
                    if let (Some(start), Some(end)) = (
                        snapshot.recognitions.previous_position(span[0]),
                        snapshot.recognitions.previous_position(span[1]),
                    ) {
                        let damage = old_damage.overlaps([start, end], &mut metadata)?;
                        let skipped = old_skipped.overlaps([start, end], &mut metadata)?
                            || new_skipped.overlaps(span, &mut metadata)?;
                        if end - start == span[1] - span[0] && !damage && !skipped {
                            reused.push(span);
                            continue;
                        }
                    }
                }
            }
            if self.language.rules[node.rule() as usize].lexical && span[0] < span[1] {
                lexical.push(span);
            }
            if span[0] == span[1] && missing.contains(&(node.rule(), span[0])) {
                damaged.push((node.rule(), span));
            }
            stack.extend(node.children().map(|(_, n)| n));
        }
        let mut recovery = if let Some((old, edit)) = base
            .and_then(|old| old.recovery().map(|r| (r, edit)))
            .and_then(|(old, edit)| edit.map(|e| (old, e)))
        {
            let mut dirty = [edit.range[0], edit.range[0] + edit.replacement.len() as u32];
            let mut include = |span: [u32; 2]| {
                dirty[0] = dirty[0].min(span[0]);
                dirty[1] = dirty[1].max(span[1]);
            };
            for span in &lexical {
                // A rebuilt recovery ancestor need not make its unchanged
                // lexical coverage dirty. Keep distant retained bytes shared.
                let same = match (
                    snapshot.recognitions.previous_position(span[0]),
                    snapshot.recognitions.previous_position(span[1]),
                ) {
                    (Some(start), Some(end)) if end - start == span[1] - span[0] => {
                        let (retained, units) = old.retained_span([start, end]);
                        metadata.charge(units)?;
                        retained
                    }
                    _ => false,
                };
                if !same {
                    include(*span);
                }
            }
            let mapped: Option<Vec<Repair>> = old
                .iter_repairs()
                .map(|r| {
                    Some(match r {
                        Repair::Missing { expected, at } => Repair::Missing {
                            expected,
                            at: snapshot.recognitions.current_position(at)?,
                        },
                        Repair::Skipped { span } => Repair::Skipped {
                            span: [
                                snapshot.recognitions.current_position(span[0])?,
                                snapshot.recognitions.current_position(span[1])?,
                            ],
                        },
                    })
                })
                .collect();
            if let Some(mapped) = mapped {
                let old_set: HashSet<_> = mapped.iter().collect();
                let new_set: HashSet<_> = repairs.iter().collect();
                for repair in mapped
                    .iter()
                    .filter(|r| !new_set.contains(r))
                    .chain(repairs.iter().filter(|r| !old_set.contains(r)))
                {
                    include(match repair {
                        Repair::Missing { at, .. } => [*at, *at],
                        Repair::Skipped { span } => *span,
                    });
                }
            } else {
                // A repair touched the edit itself; include all changed repair
                // locations conservatively while preserving distant healthy spans.
                for repair in &repairs {
                    include(match repair {
                        Repair::Missing { at, .. } => [*at, *at],
                        Repair::Skipped { span } => *span,
                    });
                }
            }
            let start = reused
                .iter()
                .filter(|span| span[1] <= dirty[0])
                .map(|span| span[1])
                .max()
                .unwrap_or(0);
            let end = reused
                .iter()
                .filter(|span| span[0] >= dirty[1])
                .map(|span| span[0])
                .min()
                .unwrap_or(text.len());
            let mut local_tokens = Vec::new();
            let mut stack = vec![snapshot.root()];
            while let Some(node) = stack.pop() {
                metadata.charge(1)?;
                let span = node.span();
                if span[1] <= start || span[0] >= end {
                    continue;
                }
                if self.language.rules[node.rule() as usize].lexical && span[0] < span[1] {
                    local_tokens.push(span);
                }
                stack.extend(node.children().map(|(_, n)| n));
            }
            snapshot.stats.recovery_coverage_bytes += u64::from(end - start);
            let coverage = coverage_source(
                self.source,
                &new_skipped.spans,
                local_tokens,
                [start, end],
                &mut metadata,
            )?;
            // Coverage's preserved prefix has left affinity at an edit seam.
            // Structural occurrences use right affinity for a shifted suffix;
            // a deletion at byte zero must still remove the old prefix bytes.
            let old_start = if start <= edit.range[0] {
                start
            } else {
                snapshot
                    .recognitions
                    .previous_position(start)
                    .unwrap_or(edit.range[0])
            };
            let old_end = snapshot
                .recognitions
                .previous_position(end)
                .unwrap_or(edit.range[1]);
            let mut next = old.splice_coverage([old_start, old_end], coverage);
            next.set_records(repairs, damaged, Some(old));
            next
        } else {
            let coverage = coverage_source(
                self.source,
                &new_skipped.spans,
                lexical,
                [0, text.len()],
                &mut metadata,
            )?;
            crate::SnapshotRecovery::from_parts(repairs, coverage, damaged, None)
        };
        metadata.sorting(structures.len())?;
        structures.sort_by_key(|(rule, at, _)| (*rule, *at));
        recovery.structures = structures.into();
        snapshot.recovery = Some(Arc::new(recovery));
        snapshot.diagnostics = Arc::new(crate::syntax::sidecars::DiagnosticStore::new(
            diagnostics,
            base.map(|b| b.diagnostics.as_ref()),
        ));
        snapshot.stats.recovery_batches = self.reused_batches;
        snapshot.stats.recovery_metadata_work = snapshot
            .stats
            .recovery_metadata_work
            .saturating_add(metadata.used);
        snapshot.stats.work = snapshot
            .stats
            .work
            .saturating_add(self.stats.work)
            .saturating_add(metadata.used);
        Ok(snapshot)
    }
}
fn repair_diagnostics_source(
    repairs: &[Repair],
    source: Source<'_>,
    work: &mut MetadataWork,
) -> Result<Vec<Diagnostic>, Diagnostic> {
    let mut result: Vec<Diagnostic> = Vec::new();
    for repair in repairs {
        work.charge(1)?;
        let (span, message) = match repair {
            Repair::Missing { expected, at } => ([*at, *at], format!("missing {expected:?}")),
            Repair::Skipped { span } => (*span, "unexpected source".into()),
        };
        if let Some(previous) = result.last_mut() {
            if previous.span[1] <= span[0]
                && source.all_whitespace(previous.span[1] as usize, span[0] as usize, || {
                    work.charge(1)
                })?
            {
                previous.span[1] = span[1];
                continue;
            }
        }
        result.push(Diagnostic::new("syntax", span, message));
    }
    Ok(result)
}
fn coverage_source(
    source: Source<'_>,
    skipped: &[[u32; 2]],
    mut tokens: Vec<[u32; 2]>,
    range: [u32; 2],
    work: &mut MetadataWork,
) -> Result<Vec<Coverage>, Diagnostic> {
    work.sorting(tokens.len())?;
    tokens.sort_unstable();
    let (mut sk, mut tk, mut at) = (0, 0, range[0] as usize);
    let mut result: Vec<Coverage> = Vec::new();
    while at < range[1] as usize {
        while sk < skipped.len() && skipped[sk][1] as usize <= at {
            sk += 1;
        }
        while tk < tokens.len() && tokens[tk][1] as usize <= at {
            tk += 1;
        }
        let (end, kind) = if sk < skipped.len() && skipped[sk][0] as usize <= at {
            (skipped[sk][1] as usize, CoverageKind::Skipped)
        } else if tk < tokens.len() && tokens[tk][0] as usize <= at {
            (tokens[tk][1] as usize, CoverageKind::Retained)
        } else if source.matches(at, "//") {
            (
                source.find(at, "\n").unwrap_or(source.len()),
                CoverageKind::Trivia,
            )
        } else if source.matches(at, "/*") && source.find(at + 2, "*/").is_some() {
            (source.find(at + 2, "*/").unwrap() + 2, CoverageKind::Trivia)
        } else {
            let c = source.character(at).unwrap();
            (
                at + c.len_utf8(),
                if c.is_whitespace() {
                    CoverageKind::Trivia
                } else {
                    CoverageKind::Retained
                },
            )
        };
        let end = if sk < skipped.len() && skipped[sk][0] as usize > at {
            end.min(skipped[sk][0] as usize)
        } else {
            end
        };
        let end = end.min(range[1] as usize);
        work.charge((end - at) as u64)?;
        if let Some(last) = result.last_mut().filter(|last| last.kind == kind) {
            last.span[1] = end as u32;
        } else {
            result.push(Coverage {
                span: [at as u32, end as u32],
                kind,
            });
        }
        at = end;
    }
    Ok(result)
}

struct MetadataWork {
    remaining: u64,
    used: u64,
    at: u32,
}
impl MetadataWork {
    fn new(remaining: u64, at: u32) -> Self {
        Self {
            remaining,
            used: 0,
            at,
        }
    }
    fn charge(&mut self, units: u64) -> Result<(), Diagnostic> {
        if units > self.remaining {
            return Err(Diagnostic::new(
                "limit",
                [self.at; 2],
                "recovery metadata work limit exceeded",
            ));
        }
        self.remaining -= units;
        self.used = self.used.saturating_add(units);
        Ok(())
    }
    fn sorting(&mut self, len: usize) -> Result<(), Diagnostic> {
        self.charge((len as u64).saturating_mul(2 * u64::from(len.max(1).ilog2()) + 2))
    }
}
struct SpanIndex {
    spans: Vec<[u32; 2]>,
}
impl SpanIndex {
    fn new(mut spans: Vec<[u32; 2]>, work: &mut MetadataWork) -> Result<Self, Diagnostic> {
        work.sorting(spans.len())?;
        spans.sort_unstable();
        let mut kept = 0;
        for i in 0..spans.len() {
            let span = spans[i];
            if kept > 0 && span[0] <= spans[kept - 1][1] {
                spans[kept - 1][1] = spans[kept - 1][1].max(span[1]);
            } else {
                spans[kept] = span;
                kept += 1;
            }
        }
        spans.truncate(kept);
        Ok(Self { spans })
    }
    fn overlaps(&self, span: [u32; 2], work: &mut MetadataWork) -> Result<bool, Diagnostic> {
        work.charge(u64::from(self.spans.len().max(1).ilog2()) + 2)?;
        let index = self.spans.partition_point(|s| s[1] < span[0]);
        Ok(self.spans.get(index).is_some_and(|s| s[0] <= span[1]))
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    #[test]
    fn span_queries_are_indexed_and_budgeted() {
        let mut work = MetadataWork::new(u64::MAX, 0);
        let spans = SpanIndex::new(
            (0..100_000).map(|n| [n * 4, n * 4 + 1]).collect(),
            &mut work,
        )
        .unwrap();
        work.used = 0;
        for n in 0..1000 {
            assert!(spans.overlaps([n * 4, n * 4], &mut work).unwrap());
            assert!(!spans.overlaps([n * 4 + 2, n * 4 + 3], &mut work).unwrap());
        }
        assert!(
            work.used < 40_000,
            "quadratic metadata queries: {}",
            work.used
        );
        let mut exhausted = MetadataWork::new(1, 0);
        assert!(spans.overlaps([0, 1], &mut exhausted).is_err());
    }
}
