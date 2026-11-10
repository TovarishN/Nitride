// The generator's runtime crate name resolves to our single facade dependency.
extern crate nitride as nitride_core;

// This binary uses only part of the generated interface API.
#[allow(dead_code)]
mod language {
    include!(concat!(env!("OUT_DIR"), "/calculator.rs"));
}

use nitride::{Engine, Error, host};

fn main() -> Result<(), Error> {
    let engine = Engine::new(&language::LANGUAGE);
    let bindings = host::bindings(&language::LANGUAGE);
    let report = engine.run("let x : Scalar = 6; x * 7;", &bindings)?;
    for value in report.values {
        println!("{value}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nitride::{HostBinding, Limits, Phase};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn counted_bindings(calls: &Arc<AtomicUsize>) -> Vec<HostBinding<'static>> {
        language::LANGUAGE
            .operations
            .iter()
            .map(|signature| {
                let calls = calls.clone();
                HostBinding::new(*signature, move |args| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(args.iter().sum())
                })
            })
            .collect()
    }

    #[test]
    fn owned_language_executes_with_explicit_host_bindings() {
        let engine = Engine::new(&language::LANGUAGE);
        let report = engine
            .run(
                "let x : Scalar = 6; x * 7;",
                &host::bindings(&language::LANGUAGE),
            )
            .unwrap();
        assert_eq!(report.values, [42.0]);
        assert_eq!(report.trace, ["Calculator.Mul"]);
    }

    #[test]
    fn invalid_later_statement_prevents_all_host_effects() {
        let engine = Engine::new(&language::LANGUAGE);
        let calls = Arc::new(AtomicUsize::new(0));
        let bindings = counted_bindings(&calls);
        for (source, phase) in [
            ("1 + 2; /*", Phase::Parse),
            ("1 + 2; missing;", Phase::Check),
            ("1 + 2; duration(3) + 4;", Phase::Check),
        ] {
            let error = engine.run(source, &bindings).unwrap_err();
            assert_eq!(error.phase(), phase);
            assert!(!error.diagnostics().is_empty());
            assert!(error.trace().is_empty());
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn preflight_preserves_all_missing_bindings_without_effects() {
        let error = Engine::new(&language::LANGUAGE)
            .run("1 + 2; 3 * 4;", &[])
            .unwrap_err();
        assert_eq!(error.phase(), Phase::Preflight);
        assert!(error.diagnostics().len() >= 2);
        assert!(error.trace().is_empty());
        let calls = Arc::new(AtomicUsize::new(0));
        let mut partial = counted_bindings(&calls);
        partial.retain(|binding| binding.signature.id == "Calculator.Add");
        let error = Engine::new(&language::LANGUAGE)
            .run("1 + 2; 3 * 4;", &partial)
            .unwrap_err();
        assert_eq!(error.phase(), Phase::Preflight);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn host_failure_keeps_completed_call_trace() {
        let mut bindings = host::bindings(&language::LANGUAGE);
        let mul = language::LANGUAGE
            .operations
            .iter()
            .find(|s| s.id == "Calculator.Mul")
            .unwrap();
        bindings.retain(|binding| binding.signature.id != mul.id);
        bindings.push(HostBinding::new(*mul, |_| {
            Err("host refused multiplication".into())
        }));
        let error = Engine::new(&language::LANGUAGE)
            .run("1 + 2; 3 * 4;", &bindings)
            .unwrap_err();
        assert_eq!(error.phase(), Phase::Execute);
        assert_eq!(error.trace(), ["Calculator.Add", "Calculator.Mul"]);
        assert!(
            error.diagnostics()[0]
                .message
                .contains("host refused multiplication")
        );
        assert!(error.to_string().contains("host refused multiplication"));
    }

    #[test]
    fn caller_limits_apply_before_host_effects() {
        let calls = Arc::new(AtomicUsize::new(0));
        let engine = Engine::new(&language::LANGUAGE).with_limits(Limits {
            work: 0,
            parser_work_per_byte: 0,
            ..Default::default()
        });
        let error = engine.run("1 + 2;", &counted_bindings(&calls)).unwrap_err();
        assert_eq!(error.phase(), Phase::Parse);
        assert_eq!(error.diagnostics()[0].category, "limit");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn syntax_only_profile_can_parse_but_cannot_compile() {
        let syntax_only = nitride::Language {
            profile: nitride::Profile::SyntaxOnly,
            ..language::LANGUAGE
        };
        let engine = Engine::new(&syntax_only);
        assert!(engine.parse("1 + 2;").is_ok());
        let error = engine.compile("1 + 2;").unwrap_err();
        assert_eq!(error.phase(), Phase::Check);
        assert_eq!(error.diagnostics()[0].category, "capability");
    }
    #[test]
    fn generated_interface_binding_runs_without_cli_or_direct_core_dependency() {
        struct Arithmetic;
        impl language::arithmetic::Arithmetic for Arithmetic {
            fn double(&self, value: f64) -> Result<f64, language::arithmetic::ArithmeticFailure> {
                Ok(value * 2.0)
            }
        }
        let bindings = language::arithmetic::bindings(&language::LANGUAGE, Arc::new(Arithmetic));
        let report = Engine::new(&language::LANGUAGE)
            .run("double(21);", &bindings)
            .unwrap();
        assert_eq!(report.values, [42.0]);
    }

    #[test]
    fn explicit_host_values_are_read_only_once_per_execution() {
        let calls = Arc::new(AtomicUsize::new(0));
        let reads = calls.clone();
        let values = [nitride::HostValue::new(
            "value",
            "pi",
            "Core.Scalar",
            move || {
                reads.fetch_add(1, Ordering::SeqCst);
                Ok(10.0)
            },
        )];
        let engine = Engine::new(&language::LANGUAGE);
        let bindings = host::bindings(&language::LANGUAGE);
        assert_eq!(
            engine.run("pi + pi;", &bindings).unwrap_err().phase(),
            Phase::Preflight
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            engine
                .run_with_values("pi + pi;", &bindings, &values)
                .unwrap()
                .values,
            [20.0]
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
