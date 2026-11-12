//! Opt-in decision census (`census` feature): how many alternatives each
//! ordered choice and Pratt phase actually attempts, after the existing
//! routes and leads, during the parses between `census_begin` and
//! `census_take` on this thread and the stack-spill threads it starts.
//! Without the feature every hook is empty and nothing is recorded.

/// The kind of decision a census entry counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecisionKind {
    Choice,
    PrattPrefix,
    PrattInfix,
}

#[cfg(feature = "census")]
mod enabled {
    use super::DecisionKind;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// One decision point: its rule, kind, ordinal within the rule (for
    /// choices; 0 for Pratt phases) and what happened there.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct DecisionStats {
        pub kind: DecisionKind,
        pub rule: u16,
        pub ordinal: u32,
        pub alternatives: u32,
        /// Decisions reached.
        pub entries: u64,
        /// Entries that ran exactly one alternative or candidate.
        pub single: u64,
        /// Entries that ran none: lookahead excluded every alternative.
        pub empty: u64,
        /// Alternatives or candidates actually run.
        pub attempts: u64,
        /// Attempts that failed.
        pub failed_attempts: u64,
        /// Failed attempts in which a match passed the decision's start.
        pub consumed_failures: u64,
        /// Logical work charged inside failed attempts, including failures
        /// nested within them.
        pub failed_work: u64,
        /// Entries where nothing matched.
        pub unmatched: u64,
    }

    /// Everything counted between `census_begin` and `census_take`.
    #[derive(Clone, Debug, Default)]
    pub struct Census {
        /// Sorted by rule, kind and ordinal.
        pub decisions: Vec<DecisionStats>,
        pub predicate_evaluations: u64,
        /// Entries into context-instance rules (`Rule::origin` is set).
        pub context_entries: u64,
        pub repeat_exits: u64,
        /// Repetition exits whose failed iteration matched past its start.
        pub repeat_consumed_exits: u64,
        /// Logical work charged inside failed choice and Pratt attempts, each
        /// counted once: a failure nested in a failed attempt is part of it.
        pub wasted_work: u64,
    }

    #[derive(Clone, Default)]
    struct State {
        rules: Vec<u16>,
        ordinals: HashMap<(u16, usize), u32>,
        next_ordinal: HashMap<u16, u32>,
        stats: HashMap<(u16, DecisionKind, u32), DecisionStats>,
        predicates: u64,
        contexts: u64,
        repeat_exits: u64,
        repeat_consumed: u64,
        wasted: u64,
    }
    type Session = Arc<Mutex<State>>;
    thread_local! {
        static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    }
    fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
        SESSION.with(|s| {
            s.borrow()
                .as_ref()
                .map(|session| f(&mut session.lock().unwrap_or_else(|e| e.into_inner())))
        })
    }

    /// Starts counting on this thread, discarding any earlier counts.
    pub fn census_begin() {
        SESSION.with(|s| *s.borrow_mut() = Some(Arc::default()));
    }
    /// Stops counting on this thread and returns the counts.
    pub fn census_take() -> Census {
        let Some(session) = SESSION.with(|s| s.borrow_mut().take()) else {
            return Census::default();
        };
        let state = std::mem::take(&mut *session.lock().unwrap_or_else(|e| e.into_inner()));
        let mut decisions: Vec<_> = state.stats.into_values().collect();
        decisions.sort_by_key(|d| (d.rule, d.kind as u8, d.ordinal));
        Census {
            decisions,
            predicate_evaluations: state.predicates,
            context_entries: state.contexts,
            repeat_exits: state.repeat_exits,
            repeat_consumed_exits: state.repeat_consumed,
            wasted_work: state.wasted,
        }
    }

    /// The census as it stood before an attempt that may be rerun from
    /// scratch, such as a parse that exhausts its stack and starts again on a
    /// spill thread: restoring it drops the aborted attempt's counts.
    pub(crate) struct Checkpoint(Option<(Session, State)>);
    impl Checkpoint {
        pub(crate) fn take() -> Self {
            Self(SESSION.with(|s| {
                s.borrow().as_ref().map(|session| {
                    let state = session.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    (session.clone(), state)
                })
            }))
        }
        pub(crate) fn restore(self) {
            if let Some((session, state)) = self.0 {
                *session.lock().unwrap_or_else(|e| e.into_inner()) = state;
            }
        }
    }

    /// The current session, carried onto a stack-spill thread.
    pub(crate) struct Scope(Option<Session>);
    impl Scope {
        pub(crate) fn capture() -> Self {
            Self(SESSION.with(|s| s.borrow().clone()))
        }
        pub(crate) fn run<R>(self, f: impl FnOnce() -> R) -> R {
            let previous = SESSION.with(|s| s.replace(self.0));
            let result = f();
            SESSION.with(|s| *s.borrow_mut() = previous);
            result
        }
    }

    /// The innermost rule being matched, for keying decisions.
    pub(crate) struct RuleGuard(bool);
    impl RuleGuard {
        pub(crate) fn enter(id: u16) -> Self {
            Self(with(|state| state.rules.push(id)).is_some())
        }
    }
    impl Drop for RuleGuard {
        fn drop(&mut self) {
            if self.0 {
                with(|state| state.rules.pop());
            }
        }
    }

    /// One decision being made; counts accumulate locally until `finish`.
    pub(crate) struct Decision {
        key: Option<(u16, DecisionKind, u32)>,
        attempts: u64,
        failed: u64,
        consumed: u64,
        failed_work: u64,
    }
    pub(crate) fn decision(kind: DecisionKind, expr: usize, alternatives: u32) -> Decision {
        let key = with(|state| {
            let rule = state.rules.last().copied().unwrap_or(u16::MAX);
            let ordinal = if kind == DecisionKind::Choice {
                let next = &mut state.next_ordinal;
                *state.ordinals.entry((rule, expr)).or_insert_with(|| {
                    let n = next.entry(rule).or_insert(0);
                    *n += 1;
                    *n - 1
                })
            } else {
                0
            };
            state
                .stats
                .entry((rule, kind, ordinal))
                .or_insert(DecisionStats {
                    kind,
                    rule,
                    ordinal,
                    alternatives,
                    entries: 0,
                    single: 0,
                    empty: 0,
                    attempts: 0,
                    failed_attempts: 0,
                    consumed_failures: 0,
                    failed_work: 0,
                    unmatched: 0,
                });
            (rule, kind, ordinal)
        });
        Decision {
            key,
            attempts: 0,
            failed: 0,
            consumed: 0,
            failed_work: 0,
        }
    }
    impl Decision {
        pub(crate) fn active(&self) -> bool {
            self.key.is_some()
        }
        pub(crate) fn attempted(&mut self) {
            self.attempts += 1;
        }
        pub(crate) fn failed(&mut self, consumed: bool, work: u64) {
            self.failed += 1;
            self.consumed += u64::from(consumed);
            self.failed_work += work;
        }
        pub(crate) fn finish(self, matched: bool) {
            let Some(key) = self.key else { return };
            with(|state| {
                if let Some(stats) = state.stats.get_mut(&key) {
                    stats.entries += 1;
                    stats.single += u64::from(self.attempts == 1);
                    stats.empty += u64::from(self.attempts == 0);
                    stats.attempts += self.attempts;
                    stats.failed_attempts += self.failed;
                    stats.consumed_failures += self.consumed;
                    stats.failed_work += self.failed_work;
                    stats.unmatched += u64::from(!matched);
                }
            });
        }
    }
    pub(crate) fn predicate() {
        with(|state| state.predicates += 1);
    }
    /// The wasted-work total so far.
    pub(crate) fn wasted_total() -> u64 {
        with(|state| state.wasted).unwrap_or(0)
    }
    /// A failed attempt that started when the total was `before` and spent
    /// `work`: its work replaces whatever nested failures added meanwhile.
    pub(crate) fn wasted(before: u64, work: u64) {
        with(|state| state.wasted = before + work);
    }
    pub(crate) fn context_rule() {
        with(|state| state.contexts += 1);
    }
    pub(crate) fn repeat_exit(consumed: bool) {
        with(|state| {
            state.repeat_exits += 1;
            state.repeat_consumed += u64::from(consumed);
        });
    }
    pub(crate) fn active() -> bool {
        SESSION.with(|s| s.borrow().is_some())
    }
}
#[cfg(feature = "census")]
pub use enabled::{Census, DecisionStats, census_begin, census_take};
#[cfg(feature = "census")]
pub(crate) use enabled::{
    Checkpoint, Decision, RuleGuard, Scope, active, context_rule, decision, predicate, repeat_exit,
    wasted, wasted_total,
};

#[cfg(not(feature = "census"))]
mod disabled {
    use super::DecisionKind;
    pub(crate) struct Checkpoint;
    impl Checkpoint {
        #[inline(always)]
        pub(crate) fn take() -> Self {
            Self
        }
        #[inline(always)]
        pub(crate) fn restore(self) {}
    }
    pub(crate) struct Scope;
    impl Scope {
        #[inline(always)]
        pub(crate) fn capture() -> Self {
            Self
        }
        #[inline(always)]
        pub(crate) fn run<R>(self, f: impl FnOnce() -> R) -> R {
            f()
        }
    }
    pub(crate) struct RuleGuard;
    impl RuleGuard {
        #[inline(always)]
        pub(crate) fn enter(_id: u16) -> Self {
            Self
        }
    }
    pub(crate) struct Decision;
    #[inline(always)]
    pub(crate) fn decision(_kind: DecisionKind, _expr: usize, _alternatives: u32) -> Decision {
        Decision
    }
    impl Decision {
        #[inline(always)]
        pub(crate) fn active(&self) -> bool {
            false
        }
        #[inline(always)]
        pub(crate) fn attempted(&mut self) {}
        #[inline(always)]
        pub(crate) fn failed(&mut self, _consumed: bool, _work: u64) {}
        #[inline(always)]
        pub(crate) fn finish(self, _matched: bool) {}
    }
    #[inline(always)]
    pub(crate) fn predicate() {}
    #[inline(always)]
    pub(crate) fn wasted_total() -> u64 {
        0
    }
    #[inline(always)]
    pub(crate) fn wasted(_before: u64, _work: u64) {}
    #[inline(always)]
    pub(crate) fn context_rule() {}
    #[inline(always)]
    pub(crate) fn repeat_exit(_consumed: bool) {}
    #[inline(always)]
    pub(crate) fn active() -> bool {
        false
    }
}
#[cfg(not(feature = "census"))]
pub(crate) use disabled::{
    Checkpoint, Decision, RuleGuard, Scope, active, context_rule, decision, predicate, repeat_exit,
    wasted, wasted_total,
};
