//! Preparation reports every independent error, not only the first.
use nitride_prepare::*;

const EMPTY_CATALOG: &str = r#"{"types":[],"operations":[],"statements":{}}"#;

fn errors(text: &str, start: &str) -> Vec<PrepareError> {
    let catalog = catalog(&[]).unwrap();
    let source = Source {
        path: "inline.ngr".into(),
        text: text.into(),
    };
    catalog
        .prepare_tolerant_all(&[source], start)
        .err()
        .unwrap_or_default()
}

fn lines(errors: &[PrepareError]) -> Vec<(usize, &str)> {
    errors
        .iter()
        .map(|e| (e.line, e.message.as_str()))
        .collect()
}

#[test]
fn every_failing_alternative_of_an_extend_is_reported() {
    // Both separated lists name a token as their delimiter (CSNitrogen's
    // interpolated strings): each alternative is reported, not only the first.
    let found = errors(
        "syntax module M\n{\n  token Mid = \"}\" \"{\";\n  token Num = ['0'..'9']+;\n  syntax Hole = Value:Num;\n  extensible syntax Expr\n  {\n    | Number = Value:Num\n  }\n  extend syntax Expr\n  {\n    | A = \"a\" (Hole; Mid)+\n    | B = \"b\" (Hole; Mid)+\n  }\n  syntax Program = Value:Expr;\n}\n",
        "M.Program",
    );
    let message = "separated lists require a nonempty literal delimiter";
    assert_eq!(lines(&found), [(12, message), (13, message)]);
}

#[test]
fn members_and_alternatives_report_in_source_order() {
    let found = errors(
        "syntax module M\n{\n  token Num = ['0'..'9']+;\n  syntax A = (Num; Num)*;\n  extensible syntax Expr\n  {\n    | One = (Num; Num)*\n    | Two = Value:Num\n  }\n  syntax B = (Num; Num)+;\n  syntax Program = Value:Expr;\n}\n",
        "M.Program",
    );
    assert_eq!(found.iter().map(|e| e.line).collect::<Vec<_>>(), [4, 7, 10]);
}

#[test]
fn rules_are_checked_independently() {
    let found = errors(
        "syntax module M\n{\n  token Num = ['0'..'9']+;\n  syntax A = Value:Missing;\n  syntax B = Value:Absent;\n  syntax Program = Value:Num;\n}\n",
        "M.Program",
    );
    assert_eq!(
        lines(&found),
        [
            (4, "unknown or ambiguous rule Missing"),
            (5, "unknown or ambiguous rule Absent"),
        ]
    );
}

#[test]
fn a_rule_that_fails_to_decode_does_not_cascade() {
    // `A` is left out after its decoding error; its use in `Program` is not
    // then reported as an unknown rule: resolution waits for decoding to pass.
    let found = errors(
        "syntax module M\n{\n  token Num = ['0'..'9']+;\n  syntax A = (Num; Num)*;\n  syntax Program = Value:A;\n}\n",
        "M.Program",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 4);
}

#[test]
fn first_error_api_reports_the_first_collected() {
    let text = "syntax module M\n{\n  token Num = ['0'..'9']+;\n  syntax A = Value:Missing;\n  syntax B = Value:Absent;\n  syntax Program = Value:Num;\n}\n";
    let source = Source {
        path: "inline.ngr".into(),
        text: text.into(),
    };
    let error = prepare_sources_tolerant(&[source], EMPTY_CATALOG, "M.Program").unwrap_err();
    assert_eq!(error.line, 4);
    assert_eq!(error.message, "unknown or ambiguous rule Missing");
}
