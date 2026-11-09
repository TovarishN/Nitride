//! Opt-in cooperative cancellation scoped to one operation and its spill threads.
use crate::Diagnostic;
use std::cell::{Cell, RefCell};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct State {
    cancelled: AtomicBool,
    predicate: Option<Box<dyn Fn() -> bool + Send + Sync>>,
}

/// A monotonic cooperative signal. Predicates must be cheap, nonblocking and
/// safe to call on a parser spill thread. A true result is latched permanently.
#[derive(Clone)]
pub struct CancellationToken(Arc<State>);
impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}
impl CancellationToken {
    pub fn new() -> Self {
        Self(Arc::new(State {
            cancelled: AtomicBool::new(false),
            predicate: None,
        }))
    }
    pub fn from_predicate(predicate: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(State {
            cancelled: AtomicBool::new(false),
            predicate: Some(Box::new(predicate)),
        }))
    }
    pub fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self::from_predicate(move || flag.load(Ordering::Relaxed))
    }
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        if self.0.cancelled.load(Ordering::Relaxed) {
            return true;
        }
        if self.0.predicate.as_ref().is_some_and(|check| check()) {
            self.cancel();
            return true;
        }
        false
    }
}

#[derive(Clone, Default)]
pub(crate) struct Context(Option<Arc<[CancellationToken]>>);
thread_local! { static CURRENT: RefCell<Context> = const { RefCell::new(Context(None)) }; }
struct Restore(Option<Context>);
impl Drop for Restore {
    fn drop(&mut self) {
        CURRENT.with(|current| {
            current.replace(self.0.take().unwrap_or_default());
        });
    }
}
impl Context {
    pub(crate) fn capture() -> Self {
        CURRENT
            .try_with(|current| current.borrow().clone())
            .unwrap_or_default()
    }
    pub(crate) fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        let previous = CURRENT.with(|current| current.replace(self.clone()));
        let guard = Restore(Some(previous));
        let result = f();
        drop(guard);
        result
    }
    fn is_cancelled(&self) -> bool {
        self.0
            .as_ref()
            .is_some_and(|tokens| tokens.iter().any(CancellationToken::is_cancelled))
    }
    fn check(&self, span: [u32; 2]) -> Result<(), Diagnostic> {
        if self.is_cancelled() {
            Err(Diagnostic::new("cancelled", span, "analysis cancelled"))
        } else {
            Ok(())
        }
    }
}

/// Run with an additional cancellation token. Enclosing tokens remain active;
/// context is restored on return or unwind. Unrelated threads do not inherit it.
pub fn with_cancellation<R>(token: &CancellationToken, f: impl FnOnce() -> R) -> R {
    let current = Context::capture();
    let mut tokens = current.0.as_deref().unwrap_or_default().to_vec();
    tokens.push(token.clone());
    Context(Some(tokens.into())).run(f)
}
pub fn cancellation_requested() -> bool {
    Context::capture().is_cancelled()
}
/// Check an operation boundary, returning a `cancelled` control-flow diagnostic
/// when any token in the current scope has been signalled.
pub fn checkpoint(span: [u32; 2]) -> Result<(), Diagnostic> {
    Context::capture().check(span)
}

/// Capture once per operation; only consult tokens after a batch of work.
pub(crate) struct Poll {
    next: Cell<u64>,
    context: Context,
}
impl Poll {
    pub(crate) fn new() -> Self {
        Self {
            next: Cell::new(0),
            context: Context::capture(),
        }
    }
    #[inline]
    pub(crate) fn check(&self, work: u64, span: [u32; 2]) -> Result<(), Diagnostic> {
        if self.context.0.is_none() {
            return Ok(());
        }
        if work >= self.next.get() {
            self.next.set(work.saturating_add(1024));
            self.context.check(span)?;
        }
        Ok(())
    }
    pub(crate) fn enabled(&self) -> bool {
        self.context.0.is_some()
    }
    /// For walks that visit every consecutive logical work value, including
    /// virtual exits. Unlike `check`, this must not be used across work jumps
    /// or speculative rewinds. Work zero checks the entry boundary.
    #[inline]
    pub(crate) fn check_unit(&self, work: u64, span: [u32; 2]) -> Result<(), Diagnostic> {
        if work & 1023 == 0 {
            self.context.check(span)?;
        }
        Ok(())
    }
    /// Speculative parser work is rolled back, but its polling credit is not.
    #[inline]
    pub(crate) fn rewind(&self, from: u64, to: u64) {
        if self.context.0.is_some() && from > to {
            self.next
                .set(to.saturating_add(self.next.get().saturating_sub(from)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    #[test]
    fn default_checkpoint_remains_usable_during_thread_local_teardown() {
        struct Probe(Arc<AtomicBool>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.store(
                    std::panic::catch_unwind(|| checkpoint([0, 0]).is_ok()).unwrap_or(false),
                    Ordering::Relaxed,
                );
            }
        }
        thread_local! { static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) }; }
        let succeeded = Arc::new(AtomicBool::new(false));
        let result = succeeded.clone();
        std::thread::spawn(move || {
            PROBE.with(|probe| *probe.borrow_mut() = Some(Probe(result)));
            assert!(checkpoint([0, 0]).is_ok());
        })
        .join()
        .unwrap();
        assert!(succeeded.load(Ordering::Relaxed));
    }

    #[test]
    fn nested_scopes_restore_after_return_and_unwind_without_masking_outer_cancel() {
        let flag = Arc::new(AtomicBool::new(false));
        let outer = CancellationToken::from_flag(flag.clone());
        let inner = CancellationToken::from_flag(Arc::new(AtomicBool::new(false)));
        assert!(!cancellation_requested());
        with_cancellation(&outer, || {
            assert!(!cancellation_requested());
            flag.store(true, Ordering::Relaxed);
            assert!(with_cancellation(&inner, cancellation_requested));
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_cancellation(&inner, || panic!("restore"))
            }));
            assert!(cancellation_requested());
        });
        assert!(!cancellation_requested());
    }

    #[test]
    fn poller_batches_work_and_observes_large_work_jumps() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let token = CancellationToken::from_predicate(move || {
            observed.fetch_add(1, Ordering::Relaxed) >= 2
        });
        with_cancellation(&token, || {
            let poll = Poll::new();
            for work in 0..1024 {
                poll.check(work, [7, 7]).unwrap();
            }
            assert_eq!(calls.load(Ordering::Relaxed), 1);
            poll.check(10_000, [8, 8]).unwrap();
            let error = poll.check(20_000, [9, 9]).unwrap_err();
            assert_eq!(error.category, "cancelled");
            assert_eq!(error.span, [9, 9]);
        });
        assert!(!cancellation_requested());
    }

    #[test]
    fn speculative_work_rewinds_preserve_remaining_poll_credit() {
        let flag = Arc::new(AtomicBool::new(false));
        let token = CancellationToken::from_flag(flag.clone());
        with_cancellation(&token, || {
            let poll = Poll::new();
            poll.check(0, [0, 0]).unwrap();
            flag.store(true, Ordering::Relaxed);
            for _ in 0..1023 {
                poll.check(1, [1, 1]).unwrap();
                poll.rewind(1, 0);
            }
            assert_eq!(poll.check(1, [2, 2]).unwrap_err().category, "cancelled");
        });
    }

    #[test]
    fn captured_context_crosses_threads_but_unrelated_threads_stay_independent() {
        let token = CancellationToken::from_flag(Arc::new(AtomicBool::new(true)));
        with_cancellation(&token, || {
            let context = Context::capture();
            assert!(!std::thread::spawn(cancellation_requested).join().unwrap());
            assert!(
                std::thread::spawn(move || context.run(cancellation_requested))
                    .join()
                    .unwrap()
            );
        });
    }

    fn rules() -> [Rule<'static>; 2] {
        let rule = |name, lexical, expr| Rule {
            name,
            lexical,
            expr,
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            origin: None,
            action: Action::None,
        };
        [
            rule(
                "Root",
                false,
                Expr::Repeat {
                    inner: &Expr::Ref(1),
                    min: 0,
                    max: u32::MAX,
                },
            ),
            rule("Word", true, Expr::Literal("x")),
        ]
    }
    fn language<'a>(rules: &'a [Rule<'static>]) -> Language<'a> {
        Language {
            trivia: TriviaPolicy::Nitrogen,
            profile: Profile::SyntaxOnly,
            rules,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            rule_checks: &[],
            patterns: &[],
            source_hash: "cancellation-fixture",
        }
    }
    fn after_polls(count: usize) -> CancellationToken {
        let calls = AtomicUsize::new(0);
        CancellationToken::from_predicate(move || calls.fetch_add(1, Ordering::Relaxed) >= count)
    }

    #[test]
    fn real_strict_parsers_stop_after_work_has_started() {
        let rules = rules();
        let language = language(&rules);
        let source = "x ".repeat(30_000);
        let token = after_polls(6);
        let error = match with_cancellation(&token, || parse(&language, &source, Limits::default()))
        {
            Err(error) => error,
            Ok(_) => panic!("cancelled strict parse"),
        };
        assert_eq!(error.category, "cancelled");
        assert!(error.span[0] > 0);
        let token = after_polls(6);
        let error = with_cancellation(&token, || {
            parse_borrowed_snapshot(
                &language,
                &source,
                Limits::default(),
                SnapshotMode::Strict(ParseOptions::default()),
            )
        })
        .err()
        .unwrap();
        assert_eq!(error.category, "cancelled");
        assert!(error.span[0] > 0);
        assert!(parse(&language, "x", Limits::default()).is_ok());
    }

    #[test]
    fn recovery_cancellation_returns_no_partial_snapshot_or_flat_retry() {
        let rules = rules();
        let language = language(&rules);
        let source =
            SourceText::new(&format!("{}?{}", "x ".repeat(5000), "x ".repeat(5000))).unwrap();
        let token = after_polls(6);
        let result = with_cancellation(&token, || {
            crate::recovery::incremental::recover_snapshot(
                &language,
                &source,
                Limits::default(),
                RecoveryOptions::default(),
                None,
                Some(Diagnostic::new("syntax", [10_000; 2], "invalid source")),
                None,
            )
        });
        assert_eq!(
            result.err().expect("no partial success").category,
            "cancelled"
        );
    }

    #[test]
    fn cancelled_edit_leaves_original_reader_and_future_edits_usable() {
        let rules = rules();
        let language = language(&rules);
        let source = "x ".repeat(15_000);
        let original = parse_borrowed_snapshot(
            &language,
            &source,
            Limits::default(),
            SnapshotMode::Recovering(RecoveryOptions::default()),
        )
        .unwrap();
        let token = after_polls(4);
        let result = with_cancellation(&token, || {
            original.edit(
                &TextEdit {
                    range: [0, source.len() as u32],
                    replacement: "x ".repeat(20_000),
                },
                Limits::default(),
            )
        });
        assert_eq!(result.err().unwrap().category, "cancelled");
        assert_eq!(original.materialize().unwrap().source, source);
        assert!(
            original
                .edit(
                    &TextEdit {
                        range: [0, 0],
                        replacement: "x ".into()
                    },
                    Limits::default()
                )
                .is_ok()
        );
    }

    #[test]
    fn parser_spill_threads_inherit_the_cancellation_scope() {
        let rules = rules();
        let language = language(&rules);
        let token = CancellationToken::from_predicate(|| {
            std::thread::current()
                .name()
                .is_some_and(|name| name.starts_with("nitride-"))
        });
        let limits = Limits {
            parser_stack_bytes: 1,
            parser_spill_stack_bytes: 8 << 20,
            ..Limits::default()
        };
        let error = with_cancellation(&token, || parse(&language, "x", limits)).unwrap_err();
        assert_eq!(error.category, "cancelled");
        let token = CancellationToken::from_predicate(|| {
            std::thread::current()
                .name()
                .is_some_and(|name| name.starts_with("nitride-"))
        });
        let error = with_cancellation(&token, || {
            parse_borrowed_snapshot(
                &language,
                "x",
                limits,
                SnapshotMode::Strict(ParseOptions::default()),
            )
        })
        .err()
        .unwrap();
        assert_eq!(error.category, "cancelled");
    }

    #[test]
    fn recovery_spill_threads_inherit_the_scope_and_never_finish_partial_success() {
        let rules = rules();
        let language = language(&rules);
        let limits = Limits {
            parser_stack_bytes: 1,
            parser_spill_stack_bytes: 8 << 20,
            ..Limits::default()
        };
        let source = "x ? x";
        let token = CancellationToken::from_predicate(|| {
            std::thread::current().name() == Some("nitride-recovery")
        });
        let parsed = with_cancellation(&token, || {
            crate::recovery::recover_after_strict(
                &language,
                source,
                limits,
                RecoveryOptions::default(),
                Diagnostic::new("syntax", [2; 2], "invalid source"),
            )
        });
        assert!(parsed.diagnostics.iter().any(|d| d.category == "cancelled"));
        assert!(
            parsed.tree.recovery().unwrap().repairs.is_empty(),
            "cancellation must not suggest deleting the unparsed document"
        );
        let text = SourceText::new(source).unwrap();
        let token = CancellationToken::from_predicate(|| {
            std::thread::current().name() == Some("nitride-shared-recovery")
        });
        let result = with_cancellation(&token, || {
            crate::recovery::incremental::recover_snapshot(
                &language,
                &text,
                limits,
                RecoveryOptions::default(),
                None,
                Some(Diagnostic::new("syntax", [2; 2], "invalid source")),
                None,
            )
        });
        assert_eq!(result.err().unwrap().category, "cancelled");
    }
}
