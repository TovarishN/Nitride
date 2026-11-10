use nitride_core::{
    host::{self, Example, ExampleExpr, Expected},
    *,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

static LANGUAGE: Language<'static> = Language {
    trivia: TriviaPolicy::Nitrogen,
    profile: Profile::SyntaxOnly,
    rules: &[],
    start: 0,
    types: &["Core.Scalar", "Core.Text"],
    operations: &[
        Signature {
            id: "Example.Identity",
            inputs: &["Core.Scalar"],
            result: "Core.Scalar",
        },
        Signature {
            id: "Example.Sum",
            inputs: &["Core.Scalar", "Core.Scalar"],
            result: "Core.Scalar",
        },
        Signature {
            id: "Example.Text",
            inputs: &["Core.Text"],
            result: "Core.Scalar",
        },
    ],
    handlers: &["identity", "add", "iso-date"],
    checks: &[],
    helpers: &[],
    rule_checks: &[],
    patterns: &[],
    source_hash: "host-example-test",
};

fn nested(calls: usize) -> host::ExampleExpr<'static> {
    let mut expr = ExampleExpr::Number(1.0);
    for _ in 0..calls {
        expr = ExampleExpr::Call {
            operation: "Example.Identity",
            args: Box::leak(Box::new([expr])),
        };
    }
    expr
}

fn bindings(calls: &Arc<AtomicUsize>) -> Vec<HostBinding<'static>> {
    let calls = calls.clone();
    vec![HostBinding::new(LANGUAGE.operations[0], move |a| {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(a[0])
    })]
}

#[test]
fn deep_examples_fail_before_any_host_effects() {
    // The large test stack lets the old implementation reach the assertion
    // safely; the missing bound should not abort the test runner itself.
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let calls = Arc::new(AtomicUsize::new(0));
            let examples = [Example {
                source: "deep",
                value: nested(256),
                expected: Expected::Equals(ExampleExpr::Number(1.0)),
            }];
            let result = host::conformance(&LANGUAGE, &bindings(&calls), &examples);
            assert_eq!(result.failed.len(), 1, "deep examples must be bounded");
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn explicit_limits_bound_both_sides_before_effects() {
    for (depth, work, accepted) in [(2, 4, true), (1, 4, false), (2, 3, false)] {
        let calls = Arc::new(AtomicUsize::new(0));
        let examples = [Example {
            source: "boundary",
            value: nested(2),
            expected: Expected::Equals(ExampleExpr::Number(1.0)),
        }];
        let result = host::conformance_with_limits(
            &LANGUAGE,
            &bindings(&calls),
            &examples,
            Limits {
                depth,
                work,
                ..Default::default()
            },
        );
        assert_eq!(result.failed.is_empty(), accepted);
        assert_eq!(calls.load(Ordering::SeqCst), if accepted { 2 } else { 0 });
    }
}

#[test]
fn oversized_expected_side_cannot_run_the_value_side() {
    let calls = Arc::new(AtomicUsize::new(0));
    let examples = [Example {
        source: "expected",
        value: nested(1),
        expected: Expected::Equals(nested(256)),
    }];
    let result = host::conformance(&LANGUAGE, &bindings(&calls), &examples);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn small_stack_handles_deep_inputs_and_the_accepted_boundary() {
    let accepted = nested(128);
    let rejected = nested(20_000);
    std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(move || {
            let calls = Arc::new(AtomicUsize::new(0));
            for (value, success) in [(accepted, true), (rejected, false)] {
                let examples = [Example {
                    source: "small-stack",
                    value,
                    expected: Expected::Equals(ExampleExpr::Number(1.0)),
                }];
                let result = host::conformance_with_limits(
                    &LANGUAGE,
                    &bindings(&calls),
                    &examples,
                    Limits {
                        depth: u32::MAX,
                        ..Default::default()
                    },
                );
                assert_eq!(result.failed.is_empty(), success);
            }
            assert_eq!(calls.load(Ordering::SeqCst), 128);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn bounded_denied_examples_are_skipped_without_calls() {
    let binding = HostBinding {
        signature: LANGUAGE.operations[0],
        handler: HostHandler::Denied(vec!["Clock".into()]),
    };
    let examples = [Example {
        source: "denied",
        value: nested(2),
        expected: Expected::Equals(ExampleExpr::Number(1.0)),
    }];
    let result = host::conformance(&LANGUAGE, &[binding], &examples);
    assert!(result.failed.is_empty());
    assert_eq!(result.skipped, ["denied"]);
}

#[test]
fn iterative_evaluation_preserves_argument_order_and_text_inputs() {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = order.clone();
    let mut bindings = host::bindings(&LANGUAGE);
    bindings.retain(|binding| {
        binding.signature.id != "Example.Identity" && binding.signature.id != "Example.Text"
    });
    bindings.push(HostBinding::new(LANGUAGE.operations[0], move |args| {
        seen.lock().unwrap().push(args[0]);
        Ok(args[0])
    }));
    bindings.push(HostBinding::text(LANGUAGE.operations[2], |text| {
        assert_eq!(text, "é😀");
        Ok(42.0)
    }));
    let examples = [
        Example {
            source: "order",
            value: ExampleExpr::Call {
                operation: "Example.Sum",
                args: &[
                    ExampleExpr::Call {
                        operation: "Example.Identity",
                        args: &[ExampleExpr::Number(2.0)],
                    },
                    ExampleExpr::Call {
                        operation: "Example.Identity",
                        args: &[ExampleExpr::Number(3.0)],
                    },
                ],
            },
            expected: Expected::Equals(ExampleExpr::Number(5.0)),
        },
        Example {
            source: "text",
            value: ExampleExpr::Call {
                operation: "Example.Text",
                args: &[ExampleExpr::Text("é😀")],
            },
            expected: Expected::Equals(ExampleExpr::Number(42.0)),
        },
    ];
    assert!(
        host::conformance(&LANGUAGE, &bindings, &examples)
            .failed
            .is_empty()
    );
    assert_eq!(*order.lock().unwrap(), [2.0, 3.0]);
}

#[test]
fn work_budget_resets_for_each_example() {
    let calls = Arc::new(AtomicUsize::new(0));
    let examples = (0..32)
        .map(|_| Example {
            source: "per-example",
            value: nested(1),
            expected: Expected::Equals(ExampleExpr::Number(1.0)),
        })
        .collect::<Vec<_>>();
    let result = host::conformance_with_limits(
        &LANGUAGE,
        &bindings(&calls),
        &examples,
        Limits {
            work: 3,
            ..Default::default()
        },
    );
    assert!(result.failed.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 32);
}
