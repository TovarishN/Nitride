use nitride_codegen::LoadedLanguage;
use nitride_core::*;
use nitride_prepare::*;
use std::path::Path;

const EMPTY_CATALOG: &str = r#"{"types":[],"operations":[],"statements":{}}"#;

fn source(path: &str) -> Source {
    let full = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path);
    Source {
        path: path.into(),
        text: std::fs::read_to_string(full).unwrap(),
    }
}

fn parses_sample(prepared: &Prepared, sample: &str) {
    let loaded = LoadedLanguage::load(&prepared.data.to_string()).unwrap();
    let text = source(sample).text;
    parse(loaded.language(), &text, Limits::default()).unwrap();
}

#[test]
fn calls_reference_prepares_tolerantly() {
    let grammar = source("languages/calls/Calls.Reference.ngr");
    let prepared = prepare_sources_tolerant(&[grammar], EMPTY_CATALOG, "Calls.Program").unwrap();
    assert!(!prepared.warnings.is_empty());
    let first = &prepared.warnings[0];
    assert_eq!((first.line, first.column), (5, 3));
    assert_eq!(first.message, "C# semantic member is not admitted");
    assert!(prepared.declarations.contains(&Declaration {
        rule: "Calls.Let".into(),
        field: "Name".into(),
        kinds: vec!["value".into()],
    }));
    parses_sample(&prepared, "languages/calls/sample.calls");
}

#[test]
fn dates_reference_prepares_tolerantly() {
    let grammar = source("languages/dates/Dates.Reference.ngr");
    let prepared = prepare_sources_tolerant(&[grammar], EMPTY_CATALOG, "Dates.Program").unwrap();
    assert!(!prepared.warnings.is_empty());
    parses_sample(&prepared, "languages/dates/sample.dates");
}

#[test]
fn strict_rejects_semantic_member() {
    let grammar = source("languages/calls/Calls.Reference.ngr");
    let error = prepare_sources(&[grammar], EMPTY_CATALOG, "Calls.Program", "syntax").unwrap_err();
    assert_eq!((error.line, error.column), (5, 3));
    assert_eq!(error.message, "C# semantic member is not admitted");
}

#[test]
fn tolerant_reports_each_member() {
    let grammar = Source {
        path: "inline.ngr".into(),
        text: "syntax module M\n{\n  symbols { value }\n  symbol property A for value : int? = null;\n  symbol property B for value : string = \"x\";\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Expr*;\n  extensible syntax Expr\n  {\n    out hover Type : int? = null;\n    | Num = Value:Number\n  }\n}\n".into(),
    };
    let prepared = prepare_sources_tolerant(&[grammar], EMPTY_CATALOG, "M.Program").unwrap();
    let lines: Vec<_> = prepared.warnings.iter().map(|w| w.line).collect();
    assert_eq!(lines, [4, 5, 10]);
}

#[test]
fn grammar_outline_nests_rules() {
    let grammar = source("languages/calls/Calls.ngr");
    let outline = grammar_outline(&grammar).unwrap();
    assert_eq!(outline.len(), 1);
    let module = &outline[0];
    assert_eq!((module.kind, module.name.as_str()), ("module", "Calls"));
    let child = |name: &str| module.children.iter().find(|c| c.name == name).unwrap();
    assert_eq!(child("Number").kind, "token");
    assert_eq!(child("Expr").kind, "extensible");
    let expr = child("Expr");
    assert_eq!(&grammar.text[expr.name_span[0]..expr.name_span[1]], "Expr");
    assert!(expr.span[0] < expr.name_span[0] && expr.name_span[1] < expr.span[1]);
}
