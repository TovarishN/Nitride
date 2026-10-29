//! Grammar-derived recovery. Strict parsing remains a separate public contract.
use crate::*;
mod grammar;
mod search;
mod semantic;
mod tree;
pub use semantic::*;
pub use tree::*;

pub fn parse_recovering<'g>(
    language: &'g Language<'g>,
    source: &str,
    limits: Limits,
    options: RecoveryOptions,
) -> RecoveredParse<'g> {
    match parse_with_options(
        language,
        source,
        limits,
        ParseOptions {
            strict_comments: true,
            ..Default::default()
        },
    ) {
        Ok(tree) => RecoveredParse {
            tree,
            diagnostics: vec![],
            stats: RecoveryStats::default(),
        },
        Err(error) => recover_after_strict(language, source, limits, options, error),
    }
}

pub(crate) mod incremental;
pub use incremental::SnapshotRecovery;

/// Continue recovery after an equivalent strict recognizer has already failed.
pub(crate) fn recover_after_strict<'g>(
    language: &'g Language<'g>,
    source: &str,
    limits: Limits,
    options: RecoveryOptions,
    error: Diagnostic,
) -> RecoveredParse<'g> {
    let error = if options.max_states == 0 && error.category == "syntax" {
        Diagnostic::new("limit", error.span, "recovery search state limit exceeded")
    } else {
        error
    };
    if error.category == "syntax" && options.max_states > 0 {
        let result = search::recover(language, source, limits, options);
        if result
            .diagnostics
            .iter()
            .any(|e| e.message == "parser stack limit exceeded")
            && limits.parser_spill_stack_bytes > limits.parser_stack_bytes
        {
            drop(result);
            let spill = Limits {
                parser_stack_bytes: limits.parser_spill_stack_bytes
                    - limits.parser_spill_stack_bytes / 8,
                ..limits
            };
            std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .name("nitride-recovery".into())
                    .stack_size(limits.parser_spill_stack_bytes)
                    .spawn_scoped(scope, || search::recover(language, source, spill, options))
                    .map(|thread| {
                        thread.join().unwrap_or_else(|_| {
                            tree::fallback(
                                language,
                                source,
                                Diagnostic::new("limit", [0, 0], "deep recovery failed"),
                            )
                        })
                    })
                    .unwrap_or_else(|_| {
                        tree::fallback(
                            language,
                            source,
                            Diagnostic::new("limit", [0, 0], "cannot start deep recovery thread"),
                        )
                    })
            })
        } else {
            result
        }
    } else {
        tree::fallback(language, source, error)
    }
}
