//! Precedence parsing with cached and planned alternatives.
use super::*;

impl<'a, 'g> Parser<'a, 'g> {
    pub(super) fn pratt(
        &mut self,
        point: u16,
        prefixes: &'g [u16],
        infixes: &'g [u16],
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        let key = MemoKey::new(
            point,
            self.position as u32,
            minimum,
            self.in_token.min(2) as u8,
        );
        if let Some(entry) = self.memo.iter().find_map(|m| m.get(&key)).copied() {
            self.stats.memo_hits += 1;
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
                Memo::Scanned { .. } => unreachable!("Pratt keys hold no lexical scans"),
            };
        }
        // Record this recognition's own observation, depth and failure reach,
        // as lexical memo entries do, rather than the running maxima of the
        // enclosing recognition: a replay elsewhere must not inherit them.
        let saved_observed = self.source.replace_observed(self.position as u64);
        let saved_depth = std::mem::replace(&mut self.max_depth, depth);
        let saved_furthest = std::mem::replace(&mut self.furthest, 0);
        let result = self.pratt_uncached(point, prefixes, infixes, minimum, depth);
        let observed = self.source.replace_observed(saved_observed);
        self.source.observe(observed);
        let local_depth = std::mem::replace(&mut self.max_depth, saved_depth);
        self.max_depth = self.max_depth.max(local_depth);
        let furthest = std::mem::replace(&mut self.furthest, saved_furthest);
        self.furthest = self.furthest.max(furthest);
        let result = result?;
        let weight = result.map_or(0, |root| self.arena.weight(root.node));
        if weight > 8192 * 64 {
            return Ok(result);
        }
        let cost = weight as usize + 256;
        if self.memo_cost[0] + cost > 2 * 1024 * 1024 || self.memo[0].len() >= self.memo_capacity {
            self.rotate_memo();
        }
        let relative_depth = local_depth.saturating_sub(depth);
        let entry = match result {
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
        Ok(result)
    }
    pub(super) fn pratt_uncached(
        &mut self,
        point: u16,
        prefixes: &'g [u16],
        infixes: &'g [u16],
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        if self.optimize_recognition && self.hole(point).is_none() {
            let dispatch = std::sync::Arc::clone(&self.dispatch);
            if let Some(plan) = dispatch.pratt(point) {
                return self.pratt_planned(&dispatch, plan, minimum, depth);
            }
        }
        self.tick(depth)?;
        let prefix_mark = self.mark();
        let mut winner = None;
        // A hole is one operand node; operators may still follow it.
        let hole = self.hole(point);
        let dispatch = std::sync::Arc::clone(&self.dispatch);
        let mut prefix_peek = None;
        let mut prefix_leads = None;
        for id in prefixes.iter().filter(|_| hole.is_none()) {
            self.restore(prefix_mark);
            let rule =
                self.language.rules.get(*id as usize).ok_or_else(|| {
                    Diagnostic::new("grammar", [0, 0], "invalid Pratt rule identity")
                })?;
            let power = rule
                .precedence
                .checked_add(1)
                .ok_or_else(|| Diagnostic::new("grammar", [0, 0], "precedence overflow"))?;
            let start = dispatch.start(*id);
            if self.optimize_recognition && !start.nullable() {
                let (at, character) = if rule.lexical {
                    self.dispatch_peek(false)?
                } else if let Some(value) = prefix_peek {
                    value
                } else {
                    let value = self.dispatch_peek(true)?;
                    prefix_peek = Some(value);
                    value
                };
                let failure = if !start.allows(character) {
                    Some(at)
                } else if let Some(lead) = dispatch.lead(*id).filter(|_| !rule.lexical) {
                    let peek = prefix_leads.get_or_insert_with(|| Peek::new(at, character));
                    self.lead_rejects(lead, peek, depth + 1)
                } else {
                    None
                };
                if let Some(failure) = failure {
                    self.tick(depth + 1)?;
                    self.furthest = self.furthest.max(failure);
                    continue;
                }
            }
            if let Some(node) = self.rule(*id, power, depth + 1)? {
                if self.position > prefix_mark.position {
                    self.consider(prefix_mark, node, rule.precedence, &mut winner);
                }
            }
        }
        let left = if let Some(length) = hole {
            let start = self.position;
            self.position += length;
            Some(self.add_node(point, start, &Children::new(start, None))?)
        } else {
            self.accept_winner(prefix_mark, winner)?
                .map(|node| self.stand_in(node))
        };
        let Some(mut left) = left else {
            return Ok(None);
        };
        loop {
            self.tick(depth)?;
            let infix_mark = self.mark();
            let mut winner = None;
            let mut infix_peek = None;
            let mut infix_leads = None;
            for id in infixes {
                self.restore(infix_mark);
                let rule = self.language.rules.get(*id as usize).ok_or_else(|| {
                    Diagnostic::new("grammar", [0, 0], "invalid Pratt rule identity")
                })?;
                if rule.precedence < minimum {
                    continue;
                }
                let Expr::Sequence(parts) = &rule.expr else {
                    return Err(Diagnostic::new(
                        "grammar",
                        [0, 0],
                        "infix must be a sequence",
                    ));
                };
                let label = match parts.first() {
                    Some(Expr::Label(label, _)) => *label,
                    Some(Expr::Ref(_)) => "",
                    _ => {
                        return Err(Diagnostic::new(
                            "grammar",
                            [0, 0],
                            "invalid infix left operand",
                        ));
                    }
                };
                let power = rule
                    .precedence
                    .checked_add(u32::from(!rule.right_associative))
                    .ok_or_else(|| Diagnostic::new("grammar", [0, 0], "precedence overflow"))?;
                if let Some(start) = dispatch
                    .tail(*id)
                    .filter(|start| self.optimize_recognition && !start.nullable())
                {
                    let (at, character) = if let Some(value) = infix_peek {
                        value
                    } else {
                        let value = self.dispatch_peek(true)?;
                        infix_peek = Some(value);
                        value
                    };
                    let failure = if !start.allows(character) {
                        Some(at)
                    } else if let Some(lead) = dispatch.tail_lead(*id) {
                        let peek = infix_leads.get_or_insert_with(|| Peek::new(at, character));
                        self.lead_rejects(lead, peek, depth + 1)
                    } else {
                        None
                    };
                    if let Some(failure) = failure {
                        self.tick(depth + 1)?;
                        self.furthest = self.furthest.max(failure);
                        continue;
                    }
                }
                let mark = self.mark();
                let start = left.start as usize;
                let mut children = self.child_buffer(*id, start)?;
                let label = self.intern(label)?;
                children.push(
                    Edge { label, node: left },
                    self.reusable.previous_position(left.start + left.width),
                );
                let compiled = self
                    .compiled
                    .filter(|_| self.optimize_recognition)
                    .and_then(|p| p.tails.get(*id as usize).copied().flatten());
                let matched = if let Some(tail) = compiled {
                    self.stats.native_syntax_tails += 1;
                    tail(
                        &mut SyntaxContext::new(self, &mut children),
                        power,
                        depth + 1,
                    )?
                } else {
                    let mut matched = true;
                    for (index, part) in parts.iter().enumerate().skip(1) {
                        let owner = if index + 1 == parts.len() {
                            rule.owner
                        } else {
                            None
                        };
                        if !self.expression(part, false, owner, power, depth + 1, &mut children)? {
                            matched = false;
                            break;
                        }
                    }
                    matched
                };
                if matched {
                    if self.position == mark.position {
                        return Err(Diagnostic::new(
                            "grammar",
                            [start as u32; 2],
                            "zero-width infix",
                        ));
                    }
                    let node = self.add_node(*id, start, &children)?;
                    self.consider(infix_mark, node, rule.precedence, &mut winner);
                }
                Self::recycle(&mut self.edge_buffers, children);
                self.restore(mark);
            }
            if let Some(node) = self.accept_winner(infix_mark, winner)? {
                self.arena.commit(prefix_mark.trail, node.node);
                left = node;
            } else {
                return Ok(Some(left));
            }
        }
    }
    /// `pratt_uncached` over a precomputed plan: one peek per operand or
    /// operator position, routed candidates in grammar order, and one bulk
    /// charge for the excluded remainder.
    pub(super) fn pratt_planned(
        &mut self,
        dispatch: &dispatch::Dispatch<'g>,
        plan: &dispatch::PrattPlan<'g>,
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        self.tick(depth)?;
        let prefix_mark = self.mark();
        let mut winner = None;
        let (at, character) = self.dispatch_peek(true)?;
        let mut peek = Peek::new(at, character);
        let mut attempted = 0;
        for index in plan.prefix_candidates(character) {
            let prefix = &plan.prefixes[index];
            if let Some(lead) = dispatch.lead(prefix.id) {
                if let Some(failure) = self.lead_rejects(lead, &mut peek, depth + 1) {
                    self.furthest = self.furthest.max(failure);
                    continue;
                }
            }
            if attempted != 0 {
                self.restore(prefix_mark);
            }
            attempted += 1;
            if let Some(node) = self.rule(prefix.id, prefix.power, depth + 1)? {
                if self.position > prefix_mark.position {
                    self.consider(prefix_mark, node, prefix.precedence, &mut winner);
                }
            }
        }
        self.skip_alternatives(plan.prefixes.len() - attempted, at, depth + 1)?;
        let left = self
            .accept_winner(prefix_mark, winner)?
            .map(|node| self.stand_in(node));
        let Some(mut left) = left else {
            return Ok(None);
        };
        loop {
            self.tick(depth)?;
            let infix_mark = self.mark();
            if !plan.has_infixes(minimum) {
                return Ok(Some(left));
            }
            let mut winner = None;
            let (at, character) = self.dispatch_peek(true)?;
            let mut peek = Peek::new(at, character);
            let (candidates, excluded) = plan.infix_candidates(character, minimum);
            self.skip_alternatives(excluded as usize, at, depth + 1)?;
            for index in candidates {
                let infix = &plan.infixes[index];
                if let Some(lead) = dispatch.tail_lead(infix.id) {
                    if let Some(failure) = self.lead_rejects(lead, &mut peek, depth + 1) {
                        self.skip_alternatives(1, failure, depth + 1)?;
                        continue;
                    }
                }
                let mark = self.mark();
                let start = left.start as usize;
                let mut children = self.child_buffer(infix.id, start)?;
                let label = self.intern(infix.label)?;
                children.push(
                    Edge { label, node: left },
                    self.reusable.previous_position(left.start + left.width),
                );
                let compiled = self
                    .compiled
                    .and_then(|p| p.tails.get(infix.id as usize).copied().flatten());
                let matched = if let Some(tail) = compiled {
                    self.stats.native_syntax_tails += 1;
                    tail(
                        &mut SyntaxContext::new(self, &mut children),
                        infix.power,
                        depth + 1,
                    )?
                } else {
                    let mut matched = true;
                    for (index, part) in infix.parts.iter().enumerate().skip(1) {
                        let owner = if index + 1 == infix.parts.len() {
                            infix.owner
                        } else {
                            None
                        };
                        if !self.expression(
                            part,
                            false,
                            owner,
                            infix.power,
                            depth + 1,
                            &mut children,
                        )? {
                            matched = false;
                            break;
                        }
                    }
                    matched
                };
                if matched {
                    if self.position == mark.position {
                        return Err(Diagnostic::new(
                            "grammar",
                            [start as u32; 2],
                            "zero-width infix",
                        ));
                    }
                    let node = self.add_node(infix.id, start, &children)?;
                    self.consider(infix_mark, node, infix.precedence, &mut winner);
                }
                Self::recycle(&mut self.edge_buffers, children);
                self.restore(mark);
            }
            if let Some(node) = self.accept_winner(infix_mark, winner)? {
                self.arena.commit(prefix_mark.trail, node.node);
                left = node;
            } else {
                return Ok(Some(left));
            }
        }
    }
}
