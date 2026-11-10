//! Rule recognition, memo reuse and syntax construction.
use super::*;

impl<'a, 'g> Parser<'a, 'g> {
    /// Convert a failed cold strict attempt into recovery seed facts. Fact
    /// roots become owners; all other syntax, including the partial root, is
    /// reclaimed before compaction.
    pub(super) fn into_seed(mut self) -> Result<Seed<'a, 'g>, Diagnostic> {
        self.clear_memo(0);
        self.clear_memo(1);
        let generated = self
            .lexers
            .zip(self.source.contiguous_text())
            .map(|(lexers, source)| Generated {
                compiled: self.compiled,
                lexers,
                source,
            });
        let facts = std::mem::take(&mut self.facts).into_entries();
        for (.., hit) in &facts {
            if hit.root != SubtreeId::NONE {
                self.arena.retain(hit.root);
            }
            self.arena.release_cache(hit.root);
        }
        self.arena.restore(0);
        let mut arena = self.arena.finish();
        arena.labels = self.labels.into();
        let mut roots: Vec<_> = facts
            .iter()
            .map(|(.., hit)| hit.root)
            .filter(|&root| root != SubtreeId::NONE)
            .collect();
        let (arena, mapping) = arena.compact(&mut roots)?;
        let recognitions =
            Index::from_entries(facts.into_iter().filter_map(|(at, key, mut hit)| {
                if hit.root != SubtreeId::NONE {
                    hit.root = mapping.get(hit.root)?;
                }
                Some((at, key, hit))
            }));
        Ok(Seed {
            tables: self.tables.clone(),
            arena,
            recognitions,
            dispatch: self.dispatch,
            generated,
        })
    }
    /// Reuse a cold seed for the same text: its facts need no edit mapping.
    pub(super) fn adopt_seed(&mut self, text: &'a SourceText, seed: &Seed<'a, 'g>) {
        if let Some(generated) = seed.generated {
            self.source = Reader::contiguous(text, generated.source);
        }
        self.labels = seed.arena.labels.to_vec();
        self.label_ids = label_ids(&self.labels);
        self.arena = ArenaBuilder::from_published(&seed.arena);
        self.reusable = seed.recognitions.clone();
        self.index = seed.recognitions.clone();
        self.dispatch = seed.dispatch.clone();
    }
    pub(super) fn key_minimum(&self, rule: u16, minimum: u32) -> u32 {
        if self
            .minimum_sensitive
            .get(rule as usize)
            .copied()
            .unwrap_or(true)
        {
            minimum
        } else {
            0
        }
    }

    pub(super) fn mark(&self) -> Mark {
        Mark {
            position: self.position,
            trail: self.arena.mark(),
        }
    }
    pub(super) fn restore(&mut self, mark: Mark) {
        self.position = mark.position;
        self.arena.restore(mark.trail);
        self.enforce_cache_budget();
    }
    pub(super) fn clear_memo(&mut self, generation: usize) {
        for entry in std::mem::take(&mut self.memo[generation]).into_values() {
            if let Memo::Hit { root, .. } = entry {
                self.arena.release_cache(root.node);
            }
        }
        self.memo_cost[generation] = 0;
    }
    /// Retire the older generation for a new one. Its buckets are kept: the
    /// new generation refills to the same bounded size.
    pub(super) fn rotate_memo(&mut self) {
        self.memo.swap(0, 1);
        self.memo_cost.swap(0, 1);
        for (_, entry) in self.memo[0].drain() {
            if let Memo::Hit { root, .. } = entry {
                self.arena.release_cache(root.node);
            }
        }
        self.memo_cost[0] = 0;
    }
    #[inline]
    pub(super) fn memo_storage_bound(&self) -> u64 {
        self.memo
            .iter()
            .map(|m| {
                if m.capacity() == 0 {
                    0
                } else {
                    (m.capacity() * std::mem::size_of::<(MemoKey, Memo)>() * 2 + 64) as u64
                }
            })
            .sum::<u64>()
            + self.prior_paths_bytes as u64
            + if self.prior_paths.capacity() == 0 {
                0
            } else {
                (self.prior_paths.capacity()
                    * 2
                    * (std::mem::size_of::<((SubtreeId, u32), PriorPath)>() + 1)
                    + 64) as u64
            }
    }
    /// Runs after every backtrack; the bound check is the common case.
    #[inline]
    pub(super) fn enforce_cache_budget(&mut self) {
        if self.arena.cache_only_bytes() + self.memo_storage_bound() > self.cache_budget {
            self.evict_caches();
        }
    }
    #[cold]
    #[inline(never)]
    pub(super) fn evict_caches(&mut self) {
        let budget = self.cache_budget;
        self.stats.cache_evictions += 1;
        self.clear_memo(1);
        self.clear_memo(0);
        self.prior_paths.clear();
        self.prior_paths.shrink_to_fit();
        self.prior_paths_bytes = 0;
        let mut remaining = self.pending_order.len();
        // Evict to a low-water mark: stopping just under the budget leaves no
        // room for the memo, so the next insert would clear it again.
        let target = budget - budget / 4;
        while self.arena.cache_only_bytes() > target && remaining != 0 {
            let Some(key) = self.pending_order.pop_back() else {
                break;
            };
            let Some(hit) = self.facts.get(&key).copied() else {
                remaining -= 1;
                continue;
            };
            if hit.root != SubtreeId::NONE && !self.arena.is_cache_only(hit.root) {
                // Its active/history owner already keeps the syntax alive.
                // Deleting this fact cannot relieve graph pressure, but loses
                // useful recognition for the first edit of a cold snapshot.
                self.pending_order.push_front(key);
                remaining -= 1;
                continue;
            }
            self.facts.remove(&key);
            self.pending_failures -= usize::from(hit.root == SubtreeId::NONE);
            self.arena.release_cache(hit.root);
            // Releasing a cached parent may expose previously owned children.
            remaining = self.pending_order.len();
        }
        self.arena.trim_dead_ids();
        debug_assert!(self.arena.cache_only_bytes() + self.memo_storage_bound() <= budget);
    }
    pub(super) fn recycle(pool: &mut Vec<Children>, mut buffer: Children) {
        if buffer.allocated() {
            buffer.clear();
            pool.push(buffer);
        }
    }
    pub(super) fn find_prior(
        &mut self,
        rule: u16,
        start: u32,
    ) -> Result<Option<PriorNode>, Diagnostic> {
        // Nodes record their context origin; prior nodes are construction hints only.
        let rule = self.language.node_rule(rule);
        let approximate = self.reusable.construction_position(start).unwrap_or(start);
        let mut scope = None;
        for i in (0..self.prior_nodes.len()).rev() {
            let prior = self.prior_nodes[i];
            let Some(node) = self.arena.node(prior.node) else {
                continue;
            };
            if node.rule == rule && (prior.start == start || prior.old_start == approximate) {
                return Ok(Some(PriorNode { start, ..prior }));
            }
            if scope.is_none()
                && (prior.start == start
                    || (prior.old_start <= approximate
                        && approximate <= prior.old_start + node.width))
            {
                scope = Some(prior);
            }
        }
        let Some(mut cursor) = scope else {
            return Ok(None);
        };
        let key = (cursor.node, start);
        if let Some(path) = self.prior_paths.get(&key) {
            return Ok(path.get(&rule).map(|(p, _)| PriorNode { start, ..*p }));
        }
        let at = approximate.max(cursor.old_start);
        let mut path = PriorPath::default();
        loop {
            self.tick(0)?;
            let Some(node) = self.arena.node(cursor.node) else {
                break;
            };
            let exact = cursor.start == start || cursor.old_start == at;
            if path.get(&node.rule).is_none_or(|(_, old_exact)| !old_exact) {
                path.insert(node.rule, (cursor, exact));
            }
            if at < cursor.old_start || at >= cursor.old_start + node.width {
                break;
            }
            let offset = at - cursor.old_start;
            let next = if let Some(sequence) = self.arena.sequence(cursor.node) {
                self.work = self
                    .work
                    .saturating_add(u64::from(sequence.len().max(1).ilog2()));
                self.tick(0)?;
                sequence.locate(u64::from(offset)).map(|(_, edge, within)| {
                    let end = at - within as u32 + edge.extent;
                    let old_start = end - self.arena.node(edge.node).unwrap().width;
                    PriorNode {
                        node: edge.node,
                        start: self
                            .reusable
                            .current_position(old_start)
                            .unwrap_or(old_start),
                        old_start,
                    }
                })
            } else {
                self.work = self.work.saturating_add(u64::from(node.child_count()));
                self.tick(0)?;
                let mut previous = cursor.old_start;
                let mut chosen = None;
                let mut score = 0;
                for edge in self.arena.children(cursor.node) {
                    let end = previous + edge.extent;
                    let child = self.arena.node(edge.node).unwrap();
                    let old_start = end - child.width;
                    if previous <= at && at <= end {
                        let rank = 1
                            + u8::from(old_start == at) * 4
                            + u8::from(old_start <= at && at < end) * 2;
                        if rank > score {
                            score = rank;
                            chosen = Some(PriorNode {
                                node: edge.node,
                                start: self
                                    .reusable
                                    .current_position(old_start)
                                    .unwrap_or(old_start),
                                old_start,
                            });
                        }
                    }
                    previous = end;
                }
                chosen
            };
            let Some(next) = next else { break };
            cursor = next;
        }
        let result = path.get(&rule).map(|(p, _)| PriorNode { start, ..*p });
        // Hints are optional construction accelerators. Keep only a bounded
        // recent generation instead of retaining paths for the whole file.
        let budget = (self.cache_budget as usize / 4).min(2 * 1024 * 1024);
        let bytes = path
            .capacity()
            .saturating_mul(2 * std::mem::size_of::<(u16, (PriorNode, bool))>())
            .saturating_add(64);
        let directory = (self.prior_paths.len() + 1)
            .next_power_of_two()
            .saturating_mul(2 * (std::mem::size_of::<((SubtreeId, u32), PriorPath)>() + 1))
            .saturating_add(64);
        if self.prior_paths.len() >= 4096
            || self
                .prior_paths_bytes
                .saturating_add(bytes)
                .saturating_add(directory)
                > budget
        {
            self.prior_paths.clear();
            self.prior_paths.shrink_to_fit();
            self.prior_paths_bytes = 0;
        }
        if bytes.saturating_add(256) <= budget {
            self.prior_paths_bytes += bytes;
            self.prior_paths.insert(key, path);
        }
        Ok(result)
    }
    pub(super) fn child_buffer(&mut self, rule: u16, start: usize) -> Result<Children, Diagnostic> {
        let prior = self.find_prior(rule, start as u32)?.and_then(|p| {
            self.arena
                .sequence(p.node)
                .map(|s| (p.node, p.old_start, s))
        });
        let mut buffer = self.edge_buffers.pop().unwrap_or_default();
        buffer.reset(start, prior);
        Ok(buffer)
    }
    pub(super) fn consider(
        &mut self,
        _mark: Mark,
        node: Handle,
        precedence: u32,
        winner: &mut Option<Winner>,
    ) {
        let rank = (self.position, precedence);
        if winner.as_ref().is_none_or(|w| rank > (w.end, w.precedence)) {
            self.arena.retain(node.node);
            if let Some(old) = winner.as_ref() {
                self.arena.release(old.node.node);
            }
            *winner = Some(Winner {
                end: self.position,
                precedence,
                node,
                tied: false,
            });
        } else if let Some(winner) = winner {
            if rank == (winner.end, winner.precedence) {
                winner.tied = true;
            }
        }
    }
    pub(super) fn accept_winner(
        &mut self,
        mark: Mark,
        winner: Option<Winner>,
    ) -> Result<Option<Handle>, Diagnostic> {
        self.restore(mark);
        let Some(winner) = winner else {
            return Ok(None);
        };
        if winner.tied {
            self.arena.release(winner.node.node);
            return Err(Diagnostic::new(
                "ambiguity",
                [mark.position as u32, winner.end as u32],
                "equally ranked extensible alternatives are outside current admission",
            ));
        }
        self.position = winner.end;
        self.arena.adopt(winner.node.node);
        Ok(Some(winner.node))
    }
    pub(super) fn add_node(
        &mut self,
        rule: u16,
        start: usize,
        children: &Children,
    ) -> Result<Handle, Diagnostic> {
        if self.in_token > 1 {
            return Ok(NO_NODE);
        }
        let node = self.arena.add_prepared(
            self.language.node_rule(rule),
            (self.position - start) as u32,
            children.iter(),
            children.prior_node(),
            children.prepare(),
        )?;
        Ok(Handle {
            node,
            start: start as u32,
            width: (self.position - start) as u32,
        })
    }
    /// `add_node` and committing it over `mark`'s temporaries.
    pub(super) fn add_committed_node(
        &mut self,
        mark: usize,
        rule: u16,
        start: usize,
        children: &Children,
    ) -> Result<Handle, Diagnostic> {
        if self.in_token > 1 {
            return Ok(NO_NODE);
        }
        let node = self.arena.add_committed(
            mark,
            self.language.node_rule(rule),
            (self.position - start) as u32,
            children.iter(),
            children.prior_node(),
            children.prepare(),
        )?;
        Ok(Handle {
            node,
            start: start as u32,
            width: (self.position - start) as u32,
        })
    }
    pub(super) fn hole(&self, _category: u16) -> Option<usize> {
        None
    }
    pub(super) fn rule(
        &mut self,
        id: u16,
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        let at = self.position as u32;
        let checkpoint = std::mem::replace(&mut self.checkpoint_next, false);
        let key = Key {
            rule: id,
            minimum: self.key_minimum(id, minimum),
            in_token: self.in_token.min(2) as u8,
        };
        let old = self.reusable.get(at, key);
        let current = checkpoint
            .then(|| self.facts.get(&(at, key)).copied())
            .flatten();
        // Facts may have been produced by a different recognition specialization
        // or a higher frame budget. An over-depth fact is not proof that normal
        // recognition needs that depth; decline reuse and run the recognizer.
        if let Some(hit) = old
            .or(current)
            .filter(|hit| depth.saturating_add(hit.depth) <= self.limits.parser_frames)
        {
            self.tick(depth.saturating_add(hit.depth))?;
            self.max_depth = self.max_depth.max(depth.saturating_add(hit.depth));
            self.source.observe(u64::from(at) + hit.observed_offset);
            self.position = at as usize + hit.end_offset as usize;
            self.furthest = self
                .furthest
                .max(at as usize + hit.furthest_offset as usize);
            if old.is_some() {
                self.stats.reused_subtrees += u64::from(hit.root != SubtreeId::NONE);
                let range = [at, self.position as u32];
                if range[0] == range[1] {
                } else if let Some(last) = self
                    .reused_ranges
                    .last_mut()
                    .filter(|last| last[0] <= range[0] && range[0] <= last[1])
                {
                    last[1] = last[1].max(range[1]);
                } else {
                    self.reused_ranges.push(range);
                }
            } else {
                self.stats.memo_hits += 1;
            }
            if hit.root == SubtreeId::NONE {
                return Ok(None);
            }
            self.arena.clone_temporary(hit.root);
            return Ok(Some(Handle {
                node: hit.root,
                start: at + hit.root_offset,
                width: self.arena.node(hit.root).expect("cached subtree").width,
            }));
        }
        if !checkpoint {
            return self.rule_inner(id, minimum, depth);
        }
        let prior = [self.reusable.construction_position(at), Some(at)]
            .into_iter()
            .flatten()
            .find_map(|old_at| {
                let hit = self.prior.get(old_at, key)?;
                self.arena.node(hit.root)?;
                let old_start = old_at + hit.root_offset;
                Some(PriorNode {
                    node: hit.root,
                    start: self
                        .reusable
                        .current_position(old_start)
                        .unwrap_or(old_start),
                    old_start,
                })
            });
        if let Some(prior) = prior {
            self.prior_nodes.push(prior);
        }
        let saved_observed = self.source.replace_observed(u64::from(at));
        let saved_depth = std::mem::replace(&mut self.max_depth, depth);
        let saved_furthest = std::mem::replace(&mut self.furthest, at as usize);
        let result = self.rule_inner(id, minimum, depth);
        if prior.is_some() {
            self.prior_nodes.pop();
        }
        let observed = self.source.replace_observed(saved_observed);
        self.source.observe(observed);
        let local_depth = std::mem::replace(&mut self.max_depth, saved_depth);
        self.max_depth = self.max_depth.max(local_depth);
        let local_furthest = std::mem::replace(&mut self.furthest, saved_furthest);
        self.furthest = self.furthest.max(local_furthest);
        let record = match result {
            Ok(Some(root)) if root != NO_NODE => Some(root),
            Ok(None) if self.failure_checkpoint == Some((at, key)) => Some(NO_NODE),
            _ => None,
        };
        if let Some(root) = record {
            if !self.language.rules[id as usize].lexical
                && (root == NO_NODE || self.position > at as usize)
            {
                let hit = Hit {
                    root: root.node,
                    root_offset: if root == NO_NODE { 0 } else { root.start - at },
                    end_offset: if root == NO_NODE {
                        0
                    } else {
                        self.position as u32 - at
                    },
                    observed_offset: observed.saturating_sub(u64::from(at)),
                    furthest_offset: (local_furthest as u32).saturating_sub(at),
                    depth: local_depth.saturating_sub(depth),
                };
                // Failed outer probes are a disposable lookup cache, not live output.
                // Bound their metadata independently of graph ownership.
                if root == NO_NODE
                    && self.pending_failures >= (self.cache_budget as usize / 512).max(1)
                    && !self.facts.contains_key(&(at, key))
                {
                    return result;
                }
                self.arena.retain_cache(root.node);
                if let Some(old) = self.facts.insert((at, key), hit) {
                    self.pending_failures -= usize::from(old.root == SubtreeId::NONE);
                    self.arena.release_cache(old.root);
                } else {
                    // Grow in step with the fact log rather than doubling.
                    let spare = self
                        .facts
                        .capacity()
                        .saturating_sub(self.pending_order.len());
                    if self.pending_order.len() == self.pending_order.capacity() && spare > 0 {
                        self.pending_order.reserve_exact(spare);
                    }
                    self.pending_order.push_back((at, key));
                }
                self.pending_failures += usize::from(root == NO_NODE);
            }
        }
        result
    }
    pub(super) fn rule_inner(
        &mut self,
        id: u16,
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        let lexical = self
            .language
            .rules
            .get(id as usize)
            .is_some_and(|r| r.lexical);
        if !lexical || self.options.token_children {
            return self.match_rule(id, minimum, depth);
        }
        if self.optimize_recognition
            && self.in_token == 0
            && !matches!(self.language.rules[id as usize].expr, Expr::Pratt { .. })
        {
            return self.lexical_cached(id, minimum, depth);
        }
        self.in_token += 1;
        let result = self.match_rule(id, minimum, depth);
        self.in_token -= 1;
        match result {
            Ok(Some(_)) if self.in_token > 0 => Ok(Some(NO_NODE)),
            other => other,
        }
    }
    pub(super) fn native_lexical(
        &mut self,
        id: u16,
        depth: u32,
    ) -> Option<Result<Option<Handle>, Diagnostic>> {
        let start = self.position;
        let (end, work, scan_depth, observed) = self.scan(id, start, depth)?;
        self.work = self.work.saturating_add(work);
        self.max_depth = self.max_depth.max(scan_depth);
        self.source.observe(observed);
        self.stats.native_lexical_calls += 1;
        self.stats.native_lexical_work = self.stats.native_lexical_work.saturating_add(work);
        let Some(end) = end else {
            self.furthest = self.furthest.max(start);
            return Some(Ok(None));
        };
        self.position = end;
        Some(self.lexical_leaf(id, start).map(Some))
    }
    /// The node a lexical match from `start` to the current position yields.
    pub(super) fn lexical_leaf(&mut self, id: u16, start: usize) -> Result<Handle, Diagnostic> {
        let rule = &self.language.rules[id as usize];
        if rule.alias && !self.options.alias_nodes && rule.owner.is_none() {
            return Ok(NO_NODE);
        }
        self.add_node(id, start, &Children::new(start, None))
    }
    /// A compiled scanner's verdict at `start`, publishing nothing: its end
    /// when matched, logical work, depth and observation. `None` without a
    /// scanner, contiguous text or a valid native verdict.
    pub(super) fn scan(
        &mut self,
        id: u16,
        start: usize,
        depth: u32,
    ) -> Option<(Option<usize>, u64, u32, u64)> {
        let lexers = self.lexers?;
        let source = self.source.contiguous_text()?;
        self.scan_with(lexers, source, id, start, depth)
    }
    /// `scan` over explicit scanners and contiguous text.
    pub(super) fn scan_with(
        &mut self,
        lexers: &crate::lexical::CompiledLexers,
        source: &str,
        id: u16,
        start: usize,
        depth: u32,
    ) -> Option<(Option<usize>, u64, u32, u64)> {
        let scanner = lexers.scanners.get(id as usize).copied().flatten()?;
        let mut context = crate::lexical::LexicalContext::new(
            source,
            start,
            self.work_budget.saturating_sub(self.work),
            self.limits.parser_frames,
            self.stack_base,
            self.stack_budget,
        );
        let matched = scanner(&mut context, depth)?;
        // Validate the publication boundary even for host-supplied callbacks.
        let end = context.position();
        if end < start || end > source.len() || !source.is_char_boundary(end) {
            return None;
        }
        Some((
            matched.then_some(end),
            context.work,
            context.max_depth,
            context.observed,
        ))
    }
    /// `lexical_cached` for a prediction, which needs only the end: the same
    /// memo hit, or the same scan, charges and memo entry, without the leaf.
    /// `None` defers to the full call (no scanner or a declined scan).
    pub(super) fn lexical_predicted(
        &mut self,
        id: u16,
        at: usize,
        depth: u32,
    ) -> Option<Result<Option<usize>, Diagnostic>> {
        let key = MemoKey::new(id, at as u32, 0, 0x80);
        if let Some(entry) = self.memo.iter().find_map(|m| m.get(&key)).copied() {
            if let Err(error) = self.tick(depth) {
                return Some(Err(error));
            }
            self.stats.memo_hits += 1;
            self.stats.lexical_cache_hits += 1;
            let (observed, relative_depth) = entry.replay();
            let maximum = depth.saturating_add(relative_depth);
            self.max_depth = self.max_depth.max(maximum);
            if maximum > self.limits.parser_frames {
                return Some(Err(Diagnostic::new(
                    "limit",
                    [at as u32; 2],
                    "parser depth/work limit exceeded",
                )));
            }
            self.source.observe(observed);
            return Some(Ok(match entry {
                Memo::Fail { .. } => None,
                Memo::Hit { end, .. } | Memo::Scanned { end, .. } => Some(end),
            }));
        }
        let (end, work, scan_depth, observed) = self.scan(id, at, depth)?;
        self.work = self.work.saturating_add(work);
        self.stats.native_lexical_calls += 1;
        self.stats.native_lexical_work = self.stats.native_lexical_work.saturating_add(work);
        let local_depth = depth.max(scan_depth);
        self.max_depth = self.max_depth.max(local_depth);
        self.source.observe(observed);
        // The leaf this match would build weighs a bare node, unless the rule
        // is an alias that builds none.
        let rule = &self.language.rules[id as usize];
        let weight = if rule.alias && !self.options.alias_nodes && rule.owner.is_none() {
            0
        } else {
            32
        };
        let cost = weight + 256;
        if self.memo_cost[0] + cost > 2 * 1024 * 1024 || self.memo[0].len() >= self.memo_capacity {
            self.rotate_memo();
        }
        let relative_depth = local_depth.saturating_sub(depth);
        let entry = match end {
            None => Memo::Fail {
                furthest: at,
                observed,
                depth: relative_depth,
            },
            Some(end) => Memo::Scanned {
                end,
                furthest: 0,
                observed,
                depth: relative_depth,
            },
        };
        self.memo[0].insert(key, entry);
        self.memo_cost[0] += cost;
        Some(Ok(end))
    }
    pub(super) fn lexical_cached(
        &mut self,
        id: u16,
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        // Pratt's context byte is 0..=2. The high bit reserves a separate
        // namespace while sharing the existing bounded generations/ownership.
        let key = MemoKey::new(id, self.position as u32, minimum, 0x80);
        if let Some(entry) = self.memo.iter().find_map(|m| m.get(&key)).copied() {
            self.tick(depth)?;
            self.stats.memo_hits += 1;
            self.stats.lexical_cache_hits += 1;
            let (observed, relative_depth) = entry.replay();
            let maximum = depth.saturating_add(relative_depth);
            self.max_depth = self.max_depth.max(maximum);
            if maximum > self.limits.parser_frames {
                return Err(Diagnostic::new(
                    "limit",
                    [self.position as u32; 2],
                    "parser depth/work limit exceeded",
                ));
            }
            self.source.observe(observed);
            return match entry {
                Memo::Fail { furthest, .. } => {
                    self.furthest = self.furthest.max(furthest);
                    Ok(None)
                }
                Memo::Hit {
                    end,
                    furthest,
                    root,
                    ..
                } => {
                    self.arena.clone_temporary(root.node);
                    self.position = end;
                    self.furthest = self.furthest.max(furthest);
                    Ok(Some(root))
                }
                Memo::Scanned {
                    end,
                    furthest,
                    observed,
                    depth: relative_depth,
                } => {
                    // Build the leaf a lexical miss would have built, and
                    // hold it in the entry as that miss would have.
                    let start = self.position;
                    self.position = end;
                    self.furthest = self.furthest.max(furthest);
                    let root = self.lexical_leaf(id, start)?;
                    self.arena.retain_cache(root.node);
                    let hit = Memo::Hit {
                        end,
                        furthest,
                        root,
                        observed,
                        depth: relative_depth,
                    };
                    if let Some(slot) = self.memo.iter_mut().find_map(|m| m.get_mut(&key)) {
                        *slot = hit;
                    }
                    Ok(Some(root))
                }
            };
        }
        let saved_observed = self.source.replace_observed(self.position as u64);
        let saved_depth = std::mem::replace(&mut self.max_depth, depth);
        let saved_furthest = std::mem::replace(&mut self.furthest, 0);
        self.in_token += 1;
        let result = self
            .native_lexical(id, depth)
            .unwrap_or_else(|| self.match_rule(id, minimum, depth));
        self.in_token -= 1;
        let observed = self.source.replace_observed(saved_observed);
        self.source.observe(observed);
        let local_depth = std::mem::replace(&mut self.max_depth, saved_depth);
        self.max_depth = self.max_depth.max(local_depth);
        let furthest = std::mem::replace(&mut self.furthest, saved_furthest);
        self.furthest = self.furthest.max(furthest);
        let root = result?;
        let weight = root.map_or(0, |root| self.arena.weight(root.node));
        if weight > 8192 * 64 {
            return Ok(root);
        }
        let cost = weight as usize + 256;
        if self.memo_cost[0] + cost > 2 * 1024 * 1024 || self.memo[0].len() >= self.memo_capacity {
            self.rotate_memo();
        }
        let relative_depth = local_depth.saturating_sub(depth);
        let entry = match root {
            None => Memo::Fail {
                furthest,
                observed,
                depth: relative_depth,
            },
            Some(root) => {
                self.arena.retain_cache(root.node);
                Memo::Hit {
                    end: self.position,
                    furthest,
                    root,
                    observed,
                    depth: relative_depth,
                }
            }
        };
        self.memo[0].insert(key, entry);
        self.memo_cost[0] += cost;
        Ok(root)
    }
    pub(super) fn stand_in(&mut self, node: Handle) -> Handle {
        if node == NO_NODE {
            return node;
        }
        let current = self.arena.node(node.node).expect("built node");
        if !self.language.rules[current.rule as usize].transparent || current.child_count() != 1 {
            return node;
        }
        let child = self
            .arena
            .children(node.node)
            .next()
            .expect("one transparent child");
        let width = self.arena.node(child.node).unwrap().width;
        self.arena.replace_last_temporary(node.node, child.node);
        Handle {
            node: child.node,
            start: node.start + child.extent - width,
            width,
        }
    }
    pub(super) fn trivia(&mut self) -> Result<(), Diagnostic> {
        // Trivia at a position is fixed by the source: skip it once, then
        // replay its end, observation and logical work. One physical check
        // covers a replay.
        if !(self.optimize_recognition && self.compiled.is_some()) {
            return self.trivia_scan();
        }
        let start = self.position;
        if let Some((end, observed, steps)) = self.tokens.trivia(start) {
            self.tick_steps(steps as usize, 0)?;
            self.position = end;
            self.source.observe(observed);
            return Ok(());
        }
        let saved = self.source.replace_observed(start as u64);
        let work = self.work;
        let result = self.trivia_scan();
        let observed = self.source.replace_observed(saved);
        self.source.observe(observed);
        result?;
        let steps = (self.work - work).min(u64::from(u32::MAX)) as u32;
        self.tokens
            .remember_trivia(start, self.position, observed, steps);
        Ok(())
    }
    pub(super) fn trivia_scan(&mut self) -> Result<(), Diagnostic> {
        loop {
            self.tick(0)?;
            let character = self.source.character(self.position);
            if let Some(c) = character.filter(|&c| self.language.trivia.whitespace(c)) {
                self.position += c.len_utf8();
            } else if !self.language.trivia.comments() {
                return Ok(());
            } else if self.optimize_recognition && self.compiled.is_some() && character != Some('/')
            {
                // Both original comment checks observe two bytes even when
                // their first byte cannot match. Preserve that dependency reach.
                self.source
                    .observe(self.position.saturating_add(2).min(self.source.len() + 1) as u64);
                self.stats.trivia_fast_skips += 1;
                return Ok(());
            } else if self.source.matches(self.position, "//") {
                self.position = self
                    .source
                    .find(self.position, "\n")
                    .unwrap_or(self.source.len());
            } else if self.source.matches(self.position, "/*") {
                if let Some(end) = self.source.find(self.position + 2, "*/") {
                    self.position = end + 2;
                } else if self.options.strict_comments {
                    return Err(Diagnostic::new(
                        "syntax",
                        [self.position as u32, self.source.len() as u32],
                        "unterminated comment",
                    ));
                } else {
                    self.position = self.source.len();
                }
            } else {
                return Ok(());
            }
        }
    }
    pub(super) fn intern(&mut self, label: &'g str) -> Result<u16, Diagnostic> {
        if label.is_empty() {
            return Ok(0);
        }
        let key = (label.as_ptr() as usize, label.len());
        if let Some(&id) = self.label_ids.get(&key) {
            return Ok(id);
        }
        let id = u16::try_from(self.labels.len()).map_err(|_| {
            Diagnostic::new(
                "limit",
                [self.position as u32; 2],
                "too many distinct labels",
            )
        })?;
        self.labels.push(label);
        self.label_ids.insert(key, id);
        Ok(id)
    }
    #[inline]
    pub(super) fn tick(&mut self, depth: u32) -> Result<(), Diagnostic> {
        self.work += 1;
        self.max_depth = self.max_depth.max(depth);
        let stack = crate::parser::stack_position_inline().abs_diff(self.stack_base);
        if stack > self.stack_budget
            || depth > self.limits.parser_frames
            || self.work > self.work_budget
        {
            return Err(self.tick_error(stack));
        }
        self.cancellation
            .check(self.work, [self.position as u32; 2])?;
        Ok(())
    }
    /// The stack limit takes precedence over the depth and work limits.
    #[cold]
    #[inline(never)]
    pub(super) fn tick_error(&self, stack: usize) -> Diagnostic {
        let message = if stack > self.stack_budget {
            crate::parser::STACK_EXHAUSTED
        } else {
            "parser depth/work limit exceeded"
        };
        Diagnostic::new("limit", [self.position as u32; 2], message)
    }
    pub(super) fn match_rule(
        &mut self,
        id: u16,
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        self.tick(depth)?;
        let rule = self
            .language
            .rules
            .get(id as usize)
            .ok_or_else(|| Diagnostic::new("grammar", [0, 0], "invalid rule identity"))?;
        let mark = self.mark();
        if !rule.lexical {
            self.trivia()?;
        }
        let start = self.position;
        let prior = self.find_prior(id, start as u32)?;
        if let Some(prior) = prior {
            self.prior_nodes.push(prior);
        }
        let result = self.match_contents(id, minimum, depth, mark, start);
        if prior.is_some() {
            self.prior_nodes.pop();
        }
        result
    }
    pub(super) fn match_contents(
        &mut self,
        id: u16,
        minimum: u32,
        depth: u32,
        mark: Mark,
        start: usize,
    ) -> Result<Option<Handle>, Diagnostic> {
        let rule = &self.language.rules[id as usize];
        if let Expr::Pratt {
            prefixes,
            infixes,
            memoize,
        } = rule.expr
        {
            return if memoize {
                self.pratt(id, prefixes, infixes, minimum, depth + 1)
            } else {
                self.pratt_uncached(id, prefixes, infixes, minimum, depth + 1)
            };
        }
        if !rule.lexical {
            if let Some(length) = self.hole(id) {
                self.position += length;
                return self
                    .add_node(id, start, &Children::new(start, None))
                    .map(Some);
            }
        }
        let mut children = self.child_buffer(id, start)?;
        let lexical_failure_base = self.furthest;
        // Only a direct final reference to the extensible point is an operand.
        // Enclosed expressions start a fresh precedence context, as in Nitrogen.
        let compiled = self
            .compiled
            .filter(|_| self.optimize_recognition && !rule.lexical)
            .and_then(|p| p.bodies.get(id as usize).copied().flatten());
        let matched = if let Some(body) = compiled {
            self.stats.native_syntax_calls += 1;
            body(
                &mut SyntaxContext::new(self, &mut children),
                minimum,
                depth + 1,
            )?
        } else if rule.owner.is_some() {
            let parts = match &rule.expr {
                Expr::Sequence(parts) => *parts,
                expr => std::slice::from_ref(expr),
            };
            let mut matched = true;
            for (index, part) in parts.iter().enumerate() {
                let owner = if index + 1 == parts.len() {
                    rule.owner
                } else {
                    None
                };
                if !self.expression(part, rule.lexical, owner, minimum, depth + 1, &mut children)? {
                    matched = false;
                    break;
                }
            }
            matched
        } else {
            self.expression(&rule.expr, rule.lexical, None, 0, depth + 1, &mut children)?
        };
        if rule.lexical {
            self.furthest = lexical_failure_base;
        }
        if !matched
            || rule
                .except
                .iter()
                .any(|text| text.len() == self.position - start && self.source.matches(start, text))
        {
            self.furthest = self
                .furthest
                .max(if rule.lexical { start } else { self.position });
            Self::recycle(&mut self.edge_buffers, children);
            self.restore(mark);
            return Ok(None);
        }
        // An alias builds no node: its one child stands in, labeled by the
        // reference to the alias. With no child, nothing remains, except for
        // an extensible alternative, which must yield an operand node.
        if rule.alias
            && !self.options.alias_nodes
            && (children.len() == 1 || (children.is_empty() && rule.owner.is_none()))
        {
            let node = children.first_node().unwrap_or(NO_NODE);
            self.arena.commit(mark.trail, node.node);
            Self::recycle(&mut self.edge_buffers, children);
            return Ok(Some(node));
        }
        let result = self.add_committed_node(mark.trail, id, start, &children);
        Self::recycle(&mut self.edge_buffers, children);
        result.map(Some)
    }
    /// Charge alternatives excluded before reaching their recognizers. One
    /// physical check covers them, as in `tick_steps`.
    pub(super) fn skip_alternatives(
        &mut self,
        count: usize,
        at: usize,
        depth: u32,
    ) -> Result<(), Diagnostic> {
        if count != 0 {
            self.tick_steps(count, depth)?;
            self.furthest = self.furthest.max(at);
        }
        Ok(())
    }
    pub(super) fn expression(
        &mut self,
        expr: &'g Expr<'g>,
        lexical: bool,
        owner: Option<u16>,
        power: u32,
        depth: u32,
        out: &mut Children,
    ) -> Result<bool, Diagnostic> {
        self.tick(depth)?;
        if !lexical
            && matches!(
                expr,
                Expr::Literal(_) | Expr::Class(_) | Expr::NotClass(_) | Expr::Any | Expr::Ref(_)
            )
        {
            self.trivia()?;
        }
        let result = match expr {
            Expr::Any => {
                if let Some(c) = self.source.character(self.position) {
                    self.position += c.len_utf8();
                    true
                } else {
                    false
                }
            }
            Expr::Predicate { positive, inner } => {
                let mark = self.mark();
                let furthest = self.furthest;
                let len = out.len();
                let matched = self.expression(inner, lexical, None, 0, depth + 1, out);
                self.restore(mark);
                self.furthest = furthest;
                out.truncate(len);
                matched? == *positive
            }
            Expr::Literal(text) => {
                let keyword = !lexical
                    && text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_')
                    && text.chars().all(|c| c.is_alphanumeric() || c == '_');
                let end = self.position + text.len();
                let boundary = !keyword
                    || self
                        .source
                        .character(end)
                        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                if self.source.matches(self.position, text) && boundary {
                    self.position += text.len();
                    true
                } else {
                    false
                }
            }
            Expr::Class(ranges) | Expr::NotClass(ranges) => {
                if let Some(c) = self.source.character(self.position).filter(|c| {
                    ranges.iter().any(|(a, b)| a <= c && c <= b)
                        != matches!(expr, Expr::NotClass(_))
                }) {
                    self.position += c.len_utf8();
                    true
                } else {
                    false
                }
            }
            Expr::Ref(id) => {
                if let Some(node) = self.rule(
                    *id,
                    if self.language.operand_of(*id, owner) {
                        power
                    } else {
                        0
                    },
                    depth + 1,
                )? {
                    if node != NO_NODE {
                        out.push(
                            Edge { label: 0, node },
                            self.reusable.previous_position(node.start + node.width),
                        );
                    }
                    true
                } else {
                    false
                }
            }
            Expr::Label(label, inner) => {
                let from = out.len();
                let matched = self.expression(inner, lexical, owner, power, depth + 1, out)?;
                if out.len() > from {
                    let label = self.intern(label)?;
                    self.work = self.work.saturating_add((out.len() - from) as u64);
                    self.tick(depth)?;
                    out.label(from, label);
                }
                matched
            }
            Expr::Sequence(parts) => {
                let mut matched = true;
                for part in *parts {
                    if !self.expression(part, lexical, None, 0, depth + 1, out)? {
                        matched = false;
                        break;
                    }
                }
                matched
            }
            Expr::Choice(parts) => {
                self.choice(expr, parts, lexical, depth, out, |parser, out, index| {
                    parser.expression(&parts[index], lexical, None, 0, depth + 1, out)
                })?
            }
            Expr::Repeat { inner, min, max } => {
                let mut count = 0;
                while count < *max {
                    let mark = self.mark();
                    let len = out.len();
                    let saved_checkpoint = self.checkpoint_next;
                    if *max > 1 && !lexical {
                        self.checkpoint_next = true;
                    }
                    let matched = self.expression(inner, lexical, None, 0, depth + 1, out);
                    self.checkpoint_next = saved_checkpoint;
                    if !matched? {
                        self.restore(mark);
                        out.truncate(len);
                        break;
                    }
                    if self.position == mark.position && *max > 1 {
                        return Err(Diagnostic::new(
                            "grammar",
                            [self.position as u32; 2],
                            "zero-width repetition",
                        ));
                    }
                    count += 1;
                }
                count >= *min
            }
            Expr::Pratt { .. } => {
                return Err(Diagnostic::new(
                    "grammar",
                    [0, 0],
                    "Pratt expressions must be rule roots",
                ));
            }
        };
        if !result {
            self.furthest = self.furthest.max(self.position);
        }
        Ok(result)
    }
}
