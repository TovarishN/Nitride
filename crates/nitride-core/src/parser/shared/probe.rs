//! Recovery probes over shared immutable syntax records.
use super::*;

impl<'g> SharedProbe<'_, 'g> {
    pub(crate) fn may_consume(&self, id: u16, c: char) -> bool {
        self.parser.dispatch.may_consume(id, c)
    }
    /// Whether a call of rule `id` at `at` may match, judged by the character
    /// there. Reading it records no observation: no fact is built from it.
    pub(super) fn may_begin(&mut self, id: u16, at: usize) -> bool {
        let c = match self.generated {
            Some(generated) => generated.source.get(at..).and_then(|s| s.chars().next()),
            None => {
                let observed = self.parser.source.observed();
                let c = self.parser.source.character(at);
                self.parser.source.replace_observed(observed);
                c
            }
        };
        let lexical = self.parser.language.rules[id as usize].lexical;
        self.parser.dispatch.may_begin(id, lexical, c)
    }
}
impl<'a, 'g> SharedProbe<'a, 'g> {
    pub(crate) fn new(
        language: &'g Language<'g>,
        text: &'a SourceText,
        limits: Limits,
        base: Option<&BorrowedParseSnapshot<'g>>,
        index: Index,
        seed: Option<&Seed<'a, 'g>>,
    ) -> Self {
        let mut probe = Self {
            parser: Parser::new(
                language,
                Input {
                    tables: seed.map(|s| s.tables.clone()),
                    compiled: None,
                    lexers: None,
                    text,
                    contiguous: None,
                },
                limits,
                ParseOptions {
                    strict_comments: true,
                    ..Default::default()
                },
                limits.parser_stack_bytes,
                base,
                index,
            ),
            handles: Vec::new(),
            active: Vec::new(),
            borrowed: None,
            unselected: Default::default(),
            free_handles: Vec::new(),
            committed: 0,
            cache: Default::default(),
            ends: Default::default(),
            prior_recovery: base
                .and_then(|b| b.recovery())
                .map_or_else(|| std::sync::Arc::from([]), |r| r.structures.clone()),
            recovery_keys: Default::default(),
            generated: None,
        };
        // Probe work is part of the existing recovery search budget. Keep its
        // recognizer unchanged until that accounting contract is versioned.
        probe.parser.optimize_recognition = false;
        if let Some(seed) = seed.filter(|_| base.is_none()) {
            probe.parser.adopt_seed(text, seed);
            probe.generated = seed.generated;
        }
        probe
    }
    pub(crate) fn rule(
        &mut self,
        id: u16,
        at: usize,
        power: u32,
    ) -> Result<Option<crate::parser::ProbeHit>, Diagnostic> {
        self.retire_borrowed();
        self.enforce_budget();
        if let Some(hit) = self.cache.get(&(id, at, power)).copied() {
            if let Some(hit) = hit {
                self.borrow_result(hit.root);
            }
            return Ok(hit);
        }
        if !self.may_begin(id, at) {
            // No match can begin here: charge it as a recorded failure's reuse.
            let hit = self.parser.tick(0).map(|()| None);
            if hit.is_ok() {
                self.cache.insert((id, at, power), None);
            }
            self.enforce_budget();
            return hit;
        }
        let key = Key {
            rule: id,
            minimum: self.parser.key_minimum(id, power),
            in_token: 0,
        };
        if self.prerecognize(id, at, power, key) {
            // The generated parser decided this probe fails: charge it as the
            // reuse of a recorded failure, without interpreting it again.
            let hit = self.parser.tick(0).map(|()| None);
            if hit.is_ok() {
                self.cache.insert((id, at, power), None);
            }
            self.enforce_budget();
            return hit;
        }
        self.parser.position = at;
        self.parser.furthest = at;
        self.parser.checkpoint_next = true;
        let mark = self.parser.mark();
        self.parser.failure_checkpoint = Some((at as u32, key));
        let result = self.parser.rule(id, power, 0);
        self.parser.failure_checkpoint = None;
        let hit = result.map(|root| {
            root.map(|root| {
                if root == NO_NODE {
                    return crate::parser::ProbeHit {
                        root: u32::MAX,
                        end: self.parser.position,
                        furthest: self.parser.furthest,
                    };
                }
                self.parser.arena.retain(root.node);
                let id = if let Some(id) = self.free_handles.pop() {
                    self.handles[id as usize] = root;
                    self.active[id as usize] = false;
                    id
                } else {
                    let id = self.handles.len() as u32;
                    self.handles.push(root);
                    self.active.push(false);
                    id
                };
                self.borrowed = Some(id);
                self.unselected.push_back(id);
                crate::parser::ProbeHit {
                    root: id,
                    end: self.parser.position,
                    furthest: self.parser.furthest,
                }
            })
        });
        self.parser.position = mark.position;
        self.parser.arena.restore(mark.trail);
        if let Ok(result) = hit {
            self.cache.insert((id, at, power), result);
        }
        self.enforce_budget();
        hit
    }
    /// Recognize a missing fact with the generated parser before the
    /// interpreted probe runs. The probe then reuses it as it reuses a seeded
    /// fact, so only that reuse is charged to the recovery budget. A failure
    /// is recorded at the probe's own key, as the probe itself records one,
    /// and reported, since the failure cap may leave it unrecorded.
    pub(super) fn prerecognize(&mut self, id: u16, at: usize, power: u32, key: Key) -> bool {
        let Some(generated) = self.generated else {
            return false;
        };
        let parser = &mut self.parser;
        if parser.language.rules[id as usize].lexical
            || parser.reusable.get(at as u32, key).is_some()
            || parser.facts.contains_key(&(at as u32, key))
        {
            return false;
        }
        let saved = (parser.work, parser.max_depth, parser.furthest);
        parser.compiled = generated.compiled;
        parser.lexers = Some(generated.lexers);
        parser.optimize_recognition = true;
        parser.position = at;
        parser.furthest = at;
        parser.checkpoint_next = true;
        let failure = parser.failure_checkpoint.replace((at as u32, key));
        let mark = parser.mark();
        let failed = matches!(parser.rule(id, power, 0), Ok(None));
        parser.failure_checkpoint = failure;
        parser.checkpoint_next = false;
        parser.position = mark.position;
        parser.arena.restore(mark.trail);
        parser.compiled = None;
        parser.lexers = None;
        parser.optimize_recognition = false;
        parser.cancellation.rewind(parser.work, saved.0);
        (parser.work, parser.max_depth, parser.furthest) = saved;
        failed
    }
    /// Whether `expr` matches at `at`, with every rule it references run by
    /// the generated parser; only the fragment's own structure is
    /// interpreted. `None` when there is no generated parser or the run
    /// reached a limit, which the interpreted predicate then decides.
    pub(super) fn generated_predicate(
        &mut self,
        expr: &'g Expr<'g>,
        at: usize,
        reuse: bool,
    ) -> Option<bool> {
        let generated = self.generated?;
        let parser = &mut self.parser;
        let saved = (parser.work, parser.max_depth, parser.furthest);
        parser.compiled = generated.compiled;
        parser.lexers = Some(generated.lexers);
        parser.optimize_recognition = true;
        parser.position = at;
        let mark = parser.mark();
        let mut out = Children::sink(at);
        parser.checkpoint_next = reuse;
        let result = parser.expression(expr, false, None, 0, 0, &mut out);
        parser.checkpoint_next = false;
        parser.restore(mark);
        parser.compiled = None;
        parser.lexers = None;
        parser.optimize_recognition = false;
        parser.cancellation.rewind(parser.work, saved.0);
        (parser.work, parser.max_depth, parser.furthest) = saved;
        result.ok()
    }
    /// The end of lexical rule `id`'s match at `at`, or `None` when it fails,
    /// without building a node. A compiled scanner decides it when one
    /// accepts the call, charging its logical work, which equals the
    /// interpreter's; otherwise the ordinary probe does.
    pub(crate) fn token_end(&mut self, id: u16, at: usize) -> Result<Option<usize>, Diagnostic> {
        // Only the end is needed, so a cached hit lends no node.
        if let Some(hit) = self.cache.get(&(id, at, 0)).copied() {
            return Ok(hit.map(|hit| hit.end));
        }
        if let Some(end) = self.ends.get(&(id, at)).copied() {
            return Ok(end);
        }
        if let Some(generated) = self.generated {
            let source = generated.source;
            if let Some((end, work, _, _)) =
                self.parser.scan_with(generated.lexers, source, id, at, 0)
            {
                // The scanner ran within the remaining work budget.
                self.parser.work = self.parser.work.saturating_add(work);
                self.ends.insert((id, at), end);
                return Ok(end);
            }
        }
        Ok(self.rule(id, at, 0)?.map(|hit| hit.end))
    }
    /// A predicate over a single syntax reference skips trivia, then calls the
    /// rule there. Pre-recognize that call: `Some(true)` when the generated
    /// parser decided it fails, `Some(false)` when a fact now exists.
    pub(super) fn prerecognize_reference(&mut self, expr: &'g Expr<'g>, at: usize) -> Option<bool> {
        let Expr::Ref(id) = *expr else {
            return None;
        };
        if self.generated.is_none() || self.parser.language.rules[id as usize].lexical {
            return None;
        }
        if !self.may_begin(id, at) {
            return Some(true);
        }
        let saved = (
            self.parser.work,
            self.parser.max_depth,
            self.parser.furthest,
        );
        self.parser.position = at;
        let skipped = self.parser.trivia();
        let start = self.parser.position;
        self.parser.cancellation.rewind(self.parser.work, saved.0);
        (
            self.parser.work,
            self.parser.max_depth,
            self.parser.furthest,
        ) = saved;
        if skipped.is_err() {
            return None;
        }
        let key = Key {
            rule: id,
            minimum: self.parser.key_minimum(id, 0),
            in_token: 0,
        };
        if self.prerecognize(id, start, 0, key) {
            return Some(true);
        }
        self.parser
            .facts
            .contains_key(&(start as u32, key))
            .then_some(false)
    }
    pub(super) fn borrow_result(&mut self, id: u32) {
        if id == u32::MAX || self.active[id as usize] {
            return;
        }
        let node = self.handles[id as usize].node;
        self.parser.arena.retain(node);
        self.parser.arena.release_cache(node);
        self.borrowed = Some(id);
    }
    pub(super) fn retire_borrowed(&mut self) {
        if let Some(id) = self.borrowed.take() {
            if !self.active[id as usize] {
                let node = self.handles[id as usize].node;
                self.parser.arena.retain_cache(node);
                self.parser.arena.release(node);
            }
        }
    }
    pub(super) fn enforce_budget(&mut self) {
        let budget = self.parser.cache_budget;
        let limit = (budget as usize / 512).clamp(1, 16384);
        let clear = self.cache.len() + self.ends.len() > limit;
        let lookup = if self.cache.capacity() == 0 {
            0
        } else {
            (self.cache.capacity()
                * std::mem::size_of::<((u16, usize, u32), Option<crate::parser::ProbeHit>)>()
                * 2
                + 64) as u64
        } + if self.ends.capacity() == 0 {
            0
        } else {
            (self.ends.capacity() * std::mem::size_of::<((u16, usize), Option<usize>)>() * 2 + 64)
                as u64
        };
        let reserved = lookup.saturating_add(
            (self.unselected.capacity() * 4 + self.free_handles.capacity() * 4) as u64,
        );
        let available = budget.saturating_sub(reserved);
        if !clear
            && self.parser.arena.cache_only_bytes() + self.parser.memo_storage_bound() <= available
        {
            return;
        }
        self.parser.stats.cache_evictions += 1;
        self.parser.clear_memo(0);
        self.parser.clear_memo(1);
        while clear || self.parser.arena.cache_only_bytes() > available {
            let Some(&id) = self.unselected.front() else {
                break;
            };
            if self.borrowed == Some(id) {
                break;
            }
            self.unselected.pop_front();
            if self.active[id as usize] || self.handles[id as usize] == NO_NODE {
                continue;
            }
            self.parser
                .arena
                .release_cache(self.handles[id as usize].node);
            self.handles[id as usize] = NO_NODE;
            self.free_handles.push(id);
        }
        let handles = &self.handles;
        if clear {
            self.cache.clear();
            self.cache.shrink_to(limit);
            self.ends.clear();
            self.ends.shrink_to(limit);
        } else {
            self.cache.retain(|_, hit| {
                hit.is_none_or(|h| h.root == u32::MAX || handles[h.root as usize] != NO_NODE)
            });
        }
        // Pending facts may own the last cached references after probe eviction.
        self.parser.cache_budget = available;
        self.parser.enforce_cache_budget();
        self.parser.cache_budget = budget;
    }
    pub(crate) fn reusable_rule(
        &mut self,
        id: u16,
        at: usize,
    ) -> Result<Option<crate::parser::ProbeHit>, Diagnostic> {
        let key = Key {
            rule: id,
            minimum: 0,
            in_token: 0,
        };
        let Some(hit) = self.parser.reusable.get(at as u32, key) else {
            return Ok(None);
        };
        if hit.root == SubtreeId::NONE || hit.end_offset == 0 {
            return Ok(None);
        }
        self.rule(id, at, 0)
    }
    pub(crate) fn predicate(&mut self, expr: &'g Expr<'g>, at: usize) -> Result<bool, Diagnostic> {
        self.retire_borrowed();
        self.enforce_budget();
        let prerecognized = self.prerecognize_reference(expr, at);
        if prerecognized == Some(true) {
            // The referenced rule fails: charge it as a recorded failure's reuse.
            let result = self.parser.tick(0).map(|()| false);
            self.enforce_budget();
            return result;
        }
        let reuse = prerecognized.is_some();
        if let Some(matched) = self.generated_predicate(expr, at, reuse) {
            // Decided with generated rules: charge it as a recorded fact's reuse.
            let result = self.parser.tick(0).map(|()| matched);
            self.enforce_budget();
            return result;
        }
        self.parser.position = at;
        let mark = self.parser.mark();
        let mut out = Children::sink(at);
        // A pre-recognized reference is reused only by that one rule call.
        self.parser.checkpoint_next = reuse;
        let result = self.parser.expression(expr, false, None, 0, 0, &mut out);
        self.parser.checkpoint_next = false;
        self.parser.restore(mark);
        self.enforce_budget();
        result
    }
    /// A history event is active speculative/output syntax, like a live winner.
    /// Its owner must survive cache eviction until the VM discards that history.
    pub(crate) fn selected(&mut self, root: u32) {
        if root == u32::MAX || self.active[root as usize] {
            return;
        }
        let node = self.handles[root as usize].node;
        if self.borrowed != Some(root) {
            self.parser.arena.retain(node);
            self.parser.arena.release_cache(node);
        }
        self.active[root as usize] = true;
    }
    pub(crate) fn commit(&mut self, roots: &mut [u32], keep_from: usize) {
        let mut next = Vec::new();
        let mut mapped = HashMap::new();
        for root in roots {
            if *root == u32::MAX || (*root as usize) < self.committed {
                continue;
            }
            *root = *mapped.entry(*root).or_insert_with(|| {
                let handle = self.handles[*root as usize];
                self.parser.arena.retain(handle.node);
                let id = (self.committed + next.len()) as u32;
                next.push(handle);
                id
            });
        }
        for (offset, (handle, active)) in self
            .handles
            .drain(self.committed..)
            .zip(self.active.drain(self.committed..))
            .enumerate()
        {
            if active || self.borrowed == Some((self.committed + offset) as u32) {
                self.parser.arena.release(handle.node)
            } else {
                self.parser.arena.release_cache(handle.node)
            }
        }
        let committed = self.committed;
        self.cache.retain(|(_, at, _), hit| {
            if *at < keep_from {
                return false;
            }
            match hit {
                None => true,
                Some(hit) if hit.root == u32::MAX || (hit.root as usize) < committed => true,
                Some(hit) => {
                    if let Some(root) = mapped.get(&hit.root) {
                        hit.root = *root;
                        true
                    } else {
                        false
                    }
                }
            }
        });
        self.borrowed = None;
        self.unselected.clear();
        self.free_handles.clear();
        self.active.extend(std::iter::repeat_n(true, next.len()));
        self.handles.extend(next);
        self.committed = self.handles.len();
        self.enforce_budget();
    }
    pub(crate) fn work(&self) -> u64 {
        self.parser.work
    }
    pub(crate) fn handle(&self, id: u32) -> Option<Handle> {
        (id != u32::MAX).then(|| self.handles[id as usize])
    }
    pub(crate) fn label(&mut self, name: &'g str) -> Result<u16, Diagnostic> {
        self.parser.intern(name)
    }
    pub(crate) fn arena(&mut self) -> &mut ArenaBuilder<'g> {
        &mut self.parser.arena
    }
    pub(crate) fn recovery_children(
        &mut self,
        rule: u16,
        start: u32,
    ) -> Result<Children, Diagnostic> {
        let units = u64::from(self.prior_recovery.len().max(1).ilog2()) + 2;
        if units > self.parser.work_budget.saturating_sub(self.parser.work) {
            return Err(Diagnostic::new(
                "limit",
                [start, start],
                "recovery metadata work limit exceeded",
            ));
        }
        self.parser.work += units;
        self.parser.stats.recovery_metadata_work += units;
        let prior = self
            .parser
            .reusable
            .previous_position(start)
            .and_then(|at| {
                let i = self
                    .prior_recovery
                    .partition_point(|(id, old_at, _)| (*id, *old_at) < (rule, at));
                self.prior_recovery
                    .get(i)
                    .filter(|(id, old_at, _)| (*id, *old_at) == (rule, at))
                    .map(|(_, _, node)| *node)
            });
        if let Some((node, old_start, items)) = prior.and_then(|node| {
            let at = self.parser.reusable.previous_position(start)?;
            self.parser
                .arena
                .sequence(node)
                .map(|items| (node, at, items))
        }) {
            Ok(Children::new(
                start as usize,
                Some((node, old_start, items)),
            ))
        } else {
            self.parser.child_buffer(rule, start as usize).map(|b| *b)
        }
    }
    pub(crate) fn recovery_push(&mut self, children: &mut Children, label: u16, node: Handle) {
        let copied = children.push(
            Edge { label, node },
            self.parser
                .reusable
                .previous_position(node.start + node.width),
        );
        self.parser.stats.recovery_child_bytes += copied as u64;
    }
    pub(crate) fn recovery_pop(&mut self, children: &mut Children) -> Option<Handle> {
        let (child, end) = children.pop_last()?;
        let width = self.parser.arena.node(child.node)?.width;
        Some(Handle {
            node: child.node,
            start: end - width,
            width,
        })
    }
    pub(crate) fn recovery_root_children(&mut self, root: Handle) -> Children {
        let previous = self
            .parser
            .arena
            .sequence(root.node)
            .map(|s| (root.node, root.start, s));
        let mut children = Children::new(0, previous);
        let mut end = root.start;
        for child in self.parser.arena.children(root.node) {
            end += child.extent;
            let width = self.parser.arena.node(child.node).unwrap().width;
            let node = Handle {
                node: child.node,
                start: end - width,
                width,
            };
            self.parser.stats.recovery_child_bytes += children.push(
                Edge {
                    label: child.label,
                    node,
                },
                Some(end),
            ) as u64;
        }
        children
    }
    pub(crate) fn recovery_node(
        &mut self,
        rule: u16,
        start: u32,
        end: u32,
        children: &Children,
    ) -> Result<SubtreeId, Diagnostic> {
        let rule = self.parser.language.node_rule(rule);
        let node = self.parser.arena.add_prepared(
            rule,
            end - start,
            children.iter(),
            children.prior_node(),
            children.prepare(),
        )?;
        self.recovery_keys.insert((rule, [start, end]));
        Ok(node)
    }
    pub(crate) fn finish(
        mut self,
        text: &SourceText,
        root: Handle,
        base: Option<&BorrowedParseSnapshot<'g>>,
        mode: SnapshotMode,
    ) -> Result<(BorrowedParseSnapshot<'g>, RecoveryKeys), Diagnostic> {
        self.parser.arena.retain(root.node);
        for (i, (handle, active)) in self.handles.into_iter().zip(self.active).enumerate() {
            if active || self.borrowed == Some(i as u32) {
                self.parser.arena.release(handle.node)
            } else {
                self.parser.arena.release_cache(handle.node)
            }
        }
        let selected: std::collections::HashSet<_> = self
            .recovery_keys
            .iter()
            .map(|(rule, span)| (*rule, span[0]))
            .collect();
        self.parser.facts.retain(|(at, key), hit| {
            hit.root != SubtreeId::NONE || selected.contains(&(key.rule, *at))
        });
        publish(self.parser, text, root, base, mode).map(|snapshot| (snapshot, self.recovery_keys))
    }
}
