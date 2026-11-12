//! Predictive choice dispatch and speculative token lookahead.
use super::*;

impl<'a, 'g> Parser<'a, 'g> {
    pub(super) fn choice_plan(
        &mut self,
        expr: &'g Expr<'g>,
        parts: &'g [Expr],
        lexical: bool,
    ) -> Option<std::sync::Arc<dispatch::ChoicePlan>> {
        if !self.optimize_recognition || !(2..=256).contains(&parts.len()) {
            return None;
        }
        if lexical && parts.len() < 4 {
            return None;
        }
        let limit = (self.cache_budget / 8).min(256 * 1024) as usize;
        let base_cost = std::mem::size_of::<dispatch::ChoicePlan>() + 192;
        if self.choice_cache_bytes > limit {
            self.choice_cache = Default::default();
            self.choice_cache_bytes = 0;
        }
        if base_cost > limit {
            return None;
        }
        // Lexical and syntactic uses of one expression plan differently.
        let key = expr as *const Expr as usize | usize::from(lexical);
        if let Some(plan) = self.choice_cache.get(&key) {
            return plan.clone();
        }
        let shared = self.dispatch.shared_plan(key);
        let fresh = shared.is_none();
        let plan = match shared {
            Some(plan) => plan,
            None => self.build_choice_plan(parts, lexical),
        };
        let cost = base_cost + plan.as_ref().map_or(0, |plan| plan.bytes());
        if fresh {
            self.dispatch.remember_plan(key, plan.clone(), cost);
        }
        if cost > limit {
            return None;
        }
        if self.choice_cache_bytes + cost > limit {
            // Drop buckets too: retained capacity must not escape the charge.
            self.choice_cache = Default::default();
            self.choice_cache_bytes = 0;
        }
        self.choice_cache_bytes += cost;
        self.choice_cache.insert(key, plan.clone());
        plan
    }
    pub(super) fn build_choice_plan(
        &mut self,
        parts: &'g [Expr],
        lexical: bool,
    ) -> Option<std::sync::Arc<dispatch::ChoicePlan>> {
        self.stats.choice_plan_builds += 1;
        let starts: Vec<_> = parts
            .iter()
            .map(|part| self.dispatch.choice_start(part, self.language))
            .collect();
        let leads = if lexical {
            Box::default()
        } else {
            parts
                .iter()
                .map(|part| self.dispatch.alternative_lead(part, self.language))
                .collect()
        };
        dispatch::ChoicePlan::new(&starts, leads).map(std::sync::Arc::new)
    }
    #[inline(always)]
    pub(super) fn tick_steps(&mut self, count: usize, depth: u32) -> Result<(), Diagnostic> {
        if count == 0 {
            return Ok(());
        }
        // Skipped alternatives don't recurse. One physical stack/depth check is
        // sufficient; the virtual work budget still stops at its first excess.
        self.tick(depth)?;
        let extra = (count - 1) as u64;
        if extra > self.work_budget.saturating_sub(self.work) {
            self.work = self.work_budget.saturating_add(1);
            return Err(Diagnostic::new(
                "limit",
                [self.position as u32; 2],
                "parser depth/work limit exceeded",
            ));
        }
        self.work += extra;
        self.cancellation
            .check(self.work, [self.position as u32; 2])?;
        Ok(())
    }
    /// An ordered choice. `attempt` runs alternative `index` at the choice's
    /// start; the interpreter and generated bodies differ only there.
    pub(super) fn choice<F>(
        &mut self,
        expr: &'g Expr<'g>,
        parts: &'g [Expr],
        lexical: bool,
        depth: u32,
        out: &mut Children,
        mut attempt: F,
    ) -> Result<bool, Diagnostic>
    where
        F: FnMut(&mut Self, &mut Children, usize) -> Result<bool, Diagnostic>,
    {
        // Choices inside lexical rules are token-level: compiled scanners make
        // them in generated parsers, so the census counts syntax choices only.
        if lexical {
            return self.choice_inner(expr, parts, lexical, depth, out, attempt);
        }
        let mut decision = census::decision(
            census::DecisionKind::Choice,
            std::ptr::from_ref(expr) as usize,
            parts.len() as u32,
        );
        if !decision.active() {
            return self.choice_inner(expr, parts, lexical, depth, out, attempt);
        }
        let result = self.choice_inner(expr, parts, lexical, depth, out, |parser, out, index| {
            parser.counted(
                &mut decision,
                |p| attempt(p, out, index),
                |matched| !*matched,
            )
        });
        decision.finish(matches!(result, Ok(true)));
        result
    }
    fn choice_inner<F>(
        &mut self,
        expr: &'g Expr<'g>,
        parts: &'g [Expr],
        lexical: bool,
        depth: u32,
        out: &mut Children,
        mut attempt: F,
    ) -> Result<bool, Diagnostic>
    where
        F: FnMut(&mut Self, &mut Children, usize) -> Result<bool, Diagnostic>,
    {
        if let Some(plan) = self.choice_plan(expr, parts, lexical) {
            return self.choice_with_plan(&plan, parts.len(), lexical, depth, out, &mut attempt);
        }
        for index in 0..parts.len() {
            let mark = self.mark();
            let len = out.len();
            if attempt(self, out, index)? {
                return Ok(true);
            }
            self.restore(mark);
            out.truncate(len);
        }
        Ok(false)
    }
    pub(super) fn choice_with_plan<F>(
        &mut self,
        plan: &dispatch::ChoicePlan,
        count: usize,
        lexical: bool,
        depth: u32,
        out: &mut Children,
        attempt: &mut F,
    ) -> Result<bool, Diagnostic>
    where
        F: FnMut(&mut Self, &mut Children, usize) -> Result<bool, Diagnostic>,
    {
        for index in 0..plan.first() {
            let mark = self.mark();
            let len = out.len();
            if attempt(self, out, index)? {
                return Ok(true);
            }
            self.restore(mark);
            out.truncate(len);
        }
        let saved = (self.position, self.work, self.max_depth, self.furthest);
        let peek = self.dispatch_peek(!lexical);
        self.cancellation.rewind(self.work, saved.1);
        (self.position, self.work, self.max_depth, self.furthest) = saved;
        // Prediction never commits work or errors before an alternative's own
        // checks. Keep read observations conservatively; replay real matching
        // when trivia/limits prevented a reliable peek.
        let Ok((at, character)) = peek else {
            for index in plan.first()..count {
                let mark = self.mark();
                let len = out.len();
                if attempt(self, out, index)? {
                    return Ok(true);
                }
                self.restore(mark);
                out.truncate(len);
            }
            return Ok(false);
        };
        let mut peek = Peek::new(at, character);
        let mut cursor = plan.first();
        for index in plan.candidates(character, cursor) {
            if let Some(lead) = plan.lead(index) {
                if let Some(failure) = self.lead_rejects(lead, &mut peek, depth + 1) {
                    self.furthest = self.furthest.max(failure);
                    continue;
                }
            }
            let skipped = index - cursor;
            self.tick_steps(skipped, depth + 1)?;
            if skipped != 0 {
                self.furthest = self.furthest.max(at);
                self.stats.choice_skips += skipped as u64;
            }
            let mark = self.mark();
            let len = out.len();
            if attempt(self, out, index)? {
                return Ok(true);
            }
            self.restore(mark);
            out.truncate(len);
            cursor = index + 1;
        }
        let skipped = count - cursor;
        self.tick_steps(skipped, depth + 1)?;
        if skipped != 0 {
            self.furthest = self.furthest.max(at);
            self.stats.choice_skips += skipped as u64;
        }
        Ok(false)
    }
    /// `None` when the input can begin one of `lead`; otherwise where every
    /// path would fail: the first token, or the token after a matched one.
    /// Tokens are read exactly as the alternative's own first steps would
    /// read them. Any error defers to the alternative, which meets it itself.
    pub(super) fn lead_rejects(
        &mut self,
        lead: &[dispatch::Step],
        peek: &mut Peek,
        depth: u32,
    ) -> Option<usize> {
        self.lead_failure(lead, peek, depth).unwrap_or(None)
    }
    pub(super) fn lead_failure(
        &mut self,
        lead: &[dispatch::Step],
        peek: &mut Peek,
        depth: u32,
    ) -> Result<Option<usize>, Diagnostic> {
        let mut furthest = peek.at;
        let byte = peek.character.map_or(dispatch::Dispatch::LEXICAL, |c| {
            if c.is_ascii() {
                c as u8
            } else {
                dispatch::Dispatch::WIDE
            }
        });
        // Steps sort by first token: read each distinct one once.
        let mut first = (u16::MAX, None);
        for step in lead {
            if step.first != first.0 {
                let lead_byte = self.dispatch.first_byte(step.first);
                let end = if lead_byte < dispatch::Dispatch::LEXICAL && lead_byte != byte {
                    None
                } else {
                    self.token_end(step.first, peek.at, peek.character, depth)?
                };
                first = (step.first, end);
            }
            let Some(end) = first.1 else {
                continue;
            };
            let Some(second) = step.second() else {
                return Ok(None);
            };
            let (next, character) = self.after_token(end, peek)?;
            if self.token_end(second, next, character, depth)?.is_some() {
                return Ok(None);
            }
            furthest = furthest.max(next);
        }
        Ok(Some(furthest))
    }
    /// Where terminal `id` ends when read at `at`, exactly as the alternative
    /// would read it. Reads are recorded as observations, cached or not.
    pub(super) fn token_end(
        &mut self,
        id: u16,
        at: usize,
        character: Option<char>,
        depth: u32,
    ) -> Result<Option<usize>, Diagnostic> {
        match self.dispatch.terminal(id) {
            dispatch::Terminal::Literal(text) => {
                if !text.starts_with(|c| Some(c) == character) {
                    return Ok(None);
                }
                let keyword = self.dispatch.keyword(id);
                Ok(self
                    .literal_at(at, text, keyword)
                    .then_some(at + text.len()))
            }
            dispatch::Terminal::Lexical(rule) => {
                if !self.dispatch.start(rule).allows(character) {
                    return Ok(None);
                }
                if let Some((end, observed)) = self.tokens.lexical(at, rule) {
                    self.source.observe(observed);
                    return Ok(end);
                }
                let saved = self.source.replace_observed(at as u64);
                let end = if self.lexical_memoized(rule) {
                    self.lexical_predicted(rule, at, depth)
                } else {
                    None
                }
                .unwrap_or_else(|| self.lexical_end(rule, at, depth));
                let observed = self.source.replace_observed(saved);
                self.source.observe(observed);
                let end = end?;
                self.tokens.remember_lexical(at, rule, end, observed);
                Ok(end)
            }
        }
    }
    /// The position and character after trivia following a token.
    pub(super) fn after_token(
        &mut self,
        end: usize,
        peek: &mut Peek,
    ) -> Result<(usize, Option<char>), Diagnostic> {
        if let Some(known) = peek.after(end) {
            return Ok(known);
        }
        let saved = (self.position, self.work, self.max_depth, self.furthest);
        self.position = end;
        let result = self.dispatch_peek(true);
        self.cancellation.rewind(self.work, saved.1);
        (self.position, self.work, self.max_depth, self.furthest) = saved;
        let value = result?;
        peek.remember_after(end, value);
        Ok(value)
    }
    /// `literal`'s comparison. A keyword compares against the identifier word
    /// at `at`, which also decides its boundary; reading the whole word only
    /// widens the recorded observation.
    pub(super) fn literal_at(&mut self, at: usize, text: &str, keyword: bool) -> bool {
        let Some(source) = self.source.contiguous_text() else {
            return self.source.matches(at, text)
                && (!keyword
                    || self
                        .source
                        .character(at + text.len())
                        .is_none_or(|c| !(c.is_alphanumeric() || c == '_')));
        };
        let end = at + text.len();
        if !keyword {
            self.source.observe(end.min(source.len() + 1) as u64);
            return source.as_bytes().get(at..end) == Some(text.as_bytes());
        }
        let (word, observed) = self.tokens.word(source, at);
        self.source
            .observe(observed.max(end.min(source.len() + 1) as u64));
        word == end && source.as_bytes().get(at..end) == Some(text.as_bytes())
    }
    /// Whether a call of lexical rule `id` goes through `lexical_cached`.
    pub(super) fn lexical_memoized(&self, id: u16) -> bool {
        self.optimize_recognition
            && self.in_token == 0
            && !self.options.token_children
            && !matches!(self.language.rules[id as usize].expr, Expr::Pratt { .. })
    }
    pub(super) fn lexical_end(
        &mut self,
        id: u16,
        at: usize,
        depth: u32,
    ) -> Result<Option<usize>, Diagnostic> {
        let mark = self.mark();
        let furthest = self.furthest;
        let checkpoint = std::mem::replace(&mut self.checkpoint_next, false);
        self.position = at;
        let reach = self.census_reach_begin();
        // Lexical rules have no reusable or pending facts: enter below `rule`.
        let result = self.rule_inner(id, 0, depth);
        self.census_reach_discard(reach);
        let end = self.position;
        self.checkpoint_next = checkpoint;
        self.restore(mark);
        self.furthest = furthest;
        Ok(result?.map(|_| end))
    }
    pub(super) fn dispatch_peek(
        &mut self,
        trivia: bool,
    ) -> Result<(usize, Option<char>), Diagnostic> {
        let saved = self.position;
        if trivia {
            self.trivia()?;
        }
        let at = self.position;
        let character = self.source.character(at);
        self.position = saved;
        Ok((at, character))
    }
}
