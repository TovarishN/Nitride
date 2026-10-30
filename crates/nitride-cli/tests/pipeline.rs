use nitride_cli::LANGUAGE;
use nitride_core::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn program(source: &str) -> Result<Program<'static>, Vec<Diagnostic>> {
    let tree = parse(&LANGUAGE, source, Limits::default()).map_err(|e| vec![e])?;
    compile(&LANGUAGE, &tree, Limits::default())
}

#[test]
fn sequential_values_do_not_replay_initializers() {
    let p = program("let x : Scalar = 1 + 2; x + x;").unwrap();
    let report = execute(&p, &fixture_bindings(), Limits::default()).unwrap();
    assert_eq!(report.values, [6.0]);
    assert_eq!(report.trace, ["Portable.Add", "Portable.Add"]);
}

#[test]
fn sequential_visibility_and_shadowing() {
    for source in ["x; let x : Scalar = 1;", "let x : Scalar = x;"] {
        assert_eq!(program(source).unwrap_err()[0].category, "binding");
    }
    let p = program("let x : Scalar = 1; let x : Scalar = x + 1; x;").unwrap();
    assert_eq!(
        execute(&p, &fixture_bindings(), Limits::default())
            .unwrap()
            .values,
        [2.0]
    );
}

#[test]
fn nominal_types_and_initializers_are_checked() {
    for source in [
        "duration(1) + 2;",
        "let x : Time = 1;",
        "let x : Unknown = 1;",
    ] {
        assert_eq!(program(source).unwrap_err()[0].category, "type");
    }
    let p = program("let t : Time = duration(2); t;").unwrap();
    assert_eq!(p.nodes.last().unwrap().ty, "Portable.Time");
}

#[test]
fn missing_late_host_binding_prevents_all_effects() {
    let p = program("duration(1); 1 + 2;").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let signature = LANGUAGE.operations[0];
    let bindings = vec![HostBinding::new(signature, move |args| {
        count.fetch_add(1, Ordering::SeqCst);
        Ok(args[0])
    })];
    assert_eq!(
        execute(&p, &bindings, Limits::default())
            .unwrap_err()
            .diagnostic
            .category,
        "host"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn incompatible_signature_is_rejected_before_effects() {
    let p = program("duration(1);").unwrap();
    let mut bindings = fixture_bindings();
    bindings[0].signature.result = "Core.Scalar";
    assert_eq!(
        execute(&p, &bindings, Limits::default())
            .unwrap_err()
            .diagnostic
            .category,
        "host"
    );
}

#[test]
fn nonfinite_result_is_an_execution_error_with_trace() {
    let failure = execute(
        &program("1 / 0;").unwrap(),
        &fixture_bindings(),
        Limits::default(),
    )
    .unwrap_err();
    assert_eq!(failure.diagnostic.category, "execution");
    assert_eq!(failure.trace, ["Portable.Div"]);
}

#[test]
fn mismatched_language_snapshot_is_rejected() {
    let tree = parse(&LANGUAGE, "1;", Limits::default()).unwrap();
    let other = Box::leak(Box::new(Language {
        profile: Profile::Typed,
        rules: LANGUAGE.rules,
        start: LANGUAGE.start,
        types: LANGUAGE.types,
        operations: LANGUAGE.operations,
        handlers: LANGUAGE.handlers,
        checks: LANGUAGE.checks,
        helpers: LANGUAGE.helpers,
        rule_checks: LANGUAGE.rule_checks,
        source_hash: LANGUAGE.source_hash,
    }));
    assert!(compile(other, &tree, Limits::default()).is_err());
}

#[test]
fn invalid_tree_edges_are_rejected_without_panic() {
    let mut tree = parse(&LANGUAGE, "1;", Limits::default()).unwrap();
    tree.edges.last_mut().unwrap().node = u32::MAX;
    assert!(compile(&LANGUAGE, &tree, Limits::default()).is_err());
}

#[test]
fn work_limits_prevent_execution_effects() {
    let p = program("1 + 2;").unwrap();
    let failure = execute(
        &p,
        &fixture_bindings(),
        Limits {
            depth: 128,
            work: 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(failure.diagnostic.category, "limit");
    assert!(failure.trace.is_empty());
}

fn fixture_bindings() -> Vec<HostBinding<'static>> {
    LANGUAGE
        .operations
        .iter()
        .zip(LANGUAGE.handlers)
        .map(|(signature, handler)| {
            HostBinding::new(*signature, move |a| {
                Ok(match *handler {
                    "identity" => a[0],
                    "neg" => -a[0],
                    "add" => a[0] + a[1],
                    "sub" => a[0] - a[1],
                    "mul" => a[0] * a[1],
                    "div" => a[0] / a[1],
                    _ => return Err("unknown handler".into()),
                })
            })
        })
        .collect()
}
