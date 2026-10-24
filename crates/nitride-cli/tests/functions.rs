use nitride_core::*;
use serde_json::Value;

fn normalize(value: &Value) -> Value {
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(k, _)| k.as_str() != "message")
                .map(|(k, v)| (k.clone(), normalize(v)))
                .collect(),
        ),
        v => v.clone(),
    }
}

#[test]
fn template_bodies_binding_origins_and_effects_match_real_nitrogen() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/functions-reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        assert_eq!(
            normalize(&nitride_cli::observe_functions(
                case["source"].as_str().unwrap()
            )),
            normalize(&case["observation"]),
            "{}",
            case["name"]
        );
    }
}
fn compile_source(source: &str) -> Program {
    let language = &nitride_cli::functions::LANGUAGE;
    let tree = parse(language, source, Limits::default()).unwrap();
    compile(language, &tree, Limits::default()).unwrap()
}

#[test]
fn repeated_and_unused_arguments_follow_template_effect_semantics() {
    let used = compile_source("def twice(x: Scalar)=add(x,x); call twice(neg(3));");
    let report = execute(
        &used,
        &nitride_cli::fixture_bindings_for(used.language),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        report.trace,
        ["Functions.Neg", "Functions.Neg", "Functions.Add"]
    );
    let dropped = compile_source("def drop(x: Scalar)=2; call drop(neg(3));");
    assert!(
        execute(&dropped, &[], Limits::default())
            .unwrap()
            .trace
            .is_empty()
    );
}
#[test]
fn all_roots_are_preflighted_before_any_template_effects() {
    let p = compile_source("neg(1); def f(x: Scalar)=add(x,x); call f(2);");
    let effects = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = effects.clone();
    let sig = p
        .language
        .operations
        .iter()
        .find(|s| s.id == "Functions.Neg")
        .unwrap();
    let hosts = [HostBinding::new(*sig, move |a| {
        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(-a[0])
    })];
    assert_eq!(
        execute(&p, &hosts, Limits::default())
            .unwrap_err()
            .diagnostic
            .category,
        "host"
    );
    assert_eq!(effects.load(std::sync::atomic::Ordering::SeqCst), 0);
}
#[test]
fn template_expansion_has_structured_depth_and_work_limits() {
    let language = &nitride_cli::functions::LANGUAGE;
    let source = "def twice(x: Scalar)=add(x,x); call twice(neg(3));";
    let tree = parse(language, source, Limits::default()).unwrap();
    assert_eq!(
        compile(
            language,
            &tree,
            Limits {
                depth: 2,
                work: 10000,
                ..Limits::default()
            }
        )
        .unwrap_err()[0]
            .category,
        "limit"
    );
    assert_eq!(
        compile(
            language,
            &tree,
            Limits {
                depth: 128,
                work: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()[0]
            .category,
        "limit"
    );
}

#[test]
fn mutated_template_syntax_returns_a_diagnostic_instead_of_panicking() {
    let language = &nitride_cli::functions::LANGUAGE;
    let mut tree = parse(
        language,
        "def id(x: Scalar)=x; call id(1);",
        Limits::default(),
    )
    .unwrap();
    tree.nodes[0].rule = u16::MAX;
    assert_eq!(
        compile(language, &tree, Limits::default()).unwrap_err()[0].category,
        "grammar"
    );
}

#[test]
fn shared_template_syntax_children_are_rejected() {
    let language = &nitride_cli::functions::LANGUAGE;
    let mut tree = parse(
        language,
        "def pair(x: Scalar)=add(x,x); call pair(1);",
        Limits::default(),
    )
    .unwrap();
    let node = tree
        .nodes
        .iter()
        .find(|n| language.rules[n.rule as usize].name == "Functions.Add")
        .unwrap();
    let start = node.first_child as usize;
    tree.edges[start + 1].node = tree.edges[start].node;
    assert_eq!(
        compile(language, &tree, Limits::default()).unwrap_err()[0].category,
        "grammar"
    );
}

#[test]
fn expansion_work_budget_limits_exponential_body_growth() {
    let language = &nitride_cli::functions::LANGUAGE;
    let mut source = String::from("def f0(x: Scalar)=add(x,x);\n");
    for i in 1..=10 {
        source += &format!(
            "def f{i}(x: Scalar)=add(call f{}(x),call f{}(x));\n",
            i - 1,
            i - 1
        );
    }
    source += "call f10(1);";
    let tree = parse(language, &source, Limits::default()).unwrap();
    assert_eq!(
        compile(
            language,
            &tree,
            Limits {
                depth: 128,
                work: 1000,
                ..Limits::default()
            }
        )
        .unwrap_err()[0]
            .category,
        "limit"
    );
    let program = compile(
        language,
        &tree,
        Limits {
            depth: 128,
            work: 100000,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(
        execute(
            &program,
            &nitride_cli::fixture_bindings_for(language),
            Limits::default()
        )
        .unwrap()
        .values,
        [2048.0]
    );
}
