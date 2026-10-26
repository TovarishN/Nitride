mod support;
use serde_json::json;
use std::path::PathBuf;
use support::*;

fn lang(path: &str) -> PathBuf {
    repo().join("languages").join(path)
}

fn power_workspace(name: &str) -> PathBuf {
    let dir = scratch(name);
    manifest(
        &dir,
        "nitride.json",
        "PortablePower",
        ".powercalc",
        &[lang("portable/Portable.ngr"), lang("power/Power.ngr")],
        "Portable.Program",
        Some(&lang("power/catalog.json")),
    );
    dir
}

#[test]
fn program_syntax_error() {
    let dir = power_workspace("syntax-error");
    let program = dir.join("x.powercalc");
    let messages = Session::run(&dir, vec![open(&program, "let x : Scalar = ;")]);
    let diagnostics = diagnostics(&messages, &program);
    let errors: Vec<_> = diagnostics
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["severity"] == 1)
        .collect();
    assert_eq!(errors.len(), 1, "{diagnostics}");
    assert_eq!(
        errors[0]["range"]["start"],
        json!({"line":0,"character":17})
    );
    assert_eq!(errors[0]["source"], "nitride");
}

#[test]
fn typed_semantic_error() {
    let dir = scratch("semantic-error");
    manifest(
        &dir,
        "nitride.json",
        "Calls",
        ".calls",
        &[lang("calls/Calls.ngr")],
        "Calls.Program",
        Some(&lang("calls/catalog.json")),
    );
    let program = dir.join("x.calls");
    let messages = Session::run(&dir, vec![open(&program, "nope(1);")]);
    let diagnostics = diagnostics(&messages, &program);
    assert!(!diagnostics.as_array().unwrap().is_empty());
    assert_eq!(diagnostics[0]["severity"], 1);
    assert_eq!(
        diagnostics[0]["range"]["start"],
        json!({"line":0,"character":0})
    );
}

#[test]
fn reference_grammar_warnings() {
    let dir = scratch("reference-warnings");
    let grammar = lang("calls/Calls.Reference.ngr");
    manifest(
        &dir,
        "nitrogen.json",
        "Calls",
        ".calls",
        std::slice::from_ref(&grammar),
        "Calls.Program",
        None,
    );
    let program = dir.join("x.calls");
    let sample = std::fs::read_to_string(lang("calls/sample.calls")).unwrap();
    let grammar_text = std::fs::read_to_string(&grammar).unwrap();
    let messages = Session::run(
        &dir,
        vec![open(&grammar, &grammar_text), open(&program, &sample)],
    );
    let warnings = diagnostics(&messages, &grammar);
    assert!(!warnings.as_array().unwrap().is_empty());
    for warning in warnings.as_array().unwrap() {
        assert_eq!(warning["severity"], 2);
        assert_eq!(warning["message"], "C# semantic member is not admitted");
    }
    assert_eq!(diagnostics(&messages, &program), json!([]));
}

#[test]
fn broken_grammar_keeps_last_language() {
    let dir = scratch("broken-grammar");
    let power = dir.join("Power.ngr");
    let original = std::fs::read_to_string(lang("power/Power.ngr")).unwrap();
    std::fs::write(&power, &original).unwrap();
    manifest(
        &dir,
        "nitride.json",
        "PortablePower",
        ".powercalc",
        &[lang("portable/Portable.ngr"), power.clone()],
        "Portable.Program",
        Some(&lang("power/catalog.json")),
    );
    let program = dir.join("x.powercalc");
    let messages = Session::run(
        &dir,
        vec![
            open(&power, &original),
            open(&program, "let x : Scalar = 2;\nx ^ 3;"),
            marker(101),
            change(&power, 2, "syntax module Portable.Power {"),
            marker(102),
            change(&power, 3, &original.replace("\"^\"", "\"**\"")),
            marker(103),
        ],
    );
    let step = |id| {
        all_diagnostics(after(&messages, id), &program)
            .first()
            .cloned()
            .unwrap()
    };
    let grammar_step = |id| {
        all_diagnostics(after(&messages, id), &power)
            .first()
            .cloned()
            .unwrap()
    };
    assert_eq!(all_diagnostics(&messages, &program)[0], json!([]));
    assert_eq!(step(101), json!([]), "old language still parses x ^ 3");
    assert_eq!(grammar_step(101)[0]["severity"], 1);
    assert!(
        !step(102).as_array().unwrap().is_empty(),
        "new language rejects ^"
    );
    assert_eq!(grammar_step(102), json!([]));
}

#[test]
fn deep_nesting_is_a_diagnostic() {
    let dir = power_workspace("deep-nesting");
    let program = dir.join("x.powercalc");
    let text = format!("{}1{};", "(".repeat(100_000), ")".repeat(100_000));
    let messages = Session::run(&dir, vec![open(&program, &text)]);
    assert!(
        !diagnostics(&messages, &program)
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn close_clears_diagnostics() {
    let dir = power_workspace("close");
    let program = dir.join("x.powercalc");
    let messages = Session::run(
        &dir,
        vec![
            open(&program, "let x = ;"),
            notification(
                "textDocument/didClose",
                json!({"textDocument":{"uri":uri(&program)}}),
            ),
        ],
    );
    let published = all_diagnostics(&messages, &program);
    assert!(!published[0].as_array().unwrap().is_empty());
    assert_eq!(published.last().unwrap(), &json!([]));
}
