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

#[test]
fn references_clauses_recorded() {
    let grammar = source("languages/calls/Calls.Reference.ngr");
    let prepared = prepare_sources_tolerant(&[grammar], EMPTY_CATALOG, "Calls.Program").unwrap();
    assert!(prepared.references.contains(&ReferenceClause {
        rule: "Calls.Ref".into(),
        field: "Name".into(),
        kinds: vec!["value".into()],
    }));
}

#[test]
fn outline_lists_alternatives() {
    let outline = grammar_outline(&source("languages/calls/Calls.ngr")).unwrap();
    let expr = outline[0]
        .children
        .iter()
        .find(|c| c.name == "Expr")
        .unwrap();
    let num = expr.children.iter().find(|c| c.name == "Num").unwrap();
    assert_eq!(num.kind, "alternative");
}

#[test]
fn grammar_references_found() {
    let grammar = source("languages/power/Power.ngr");
    let references = grammar_references(&grammar).unwrap();
    let names: Vec<_> = references
        .iter()
        .map(|r| (r.name.as_str(), r.module.as_str()))
        .collect();
    assert!(
        names.contains(&("Portable.Expr", "Portable.Power")),
        "{names:?}"
    );
    assert_eq!(
        names.iter().filter(|(n, _)| *n == "Expr").count(),
        2,
        "{names:?}"
    );
    let first = &references[0];
    assert_eq!(&grammar.text[first.span[0]..first.span[1]], first.name);
}

#[test]
fn grammar_locals_bind_quote_placeholders() {
    let grammar = source("languages/rewrites/Rewrites.ngr");
    let locals = grammar_locals(&grammar).unwrap();
    let text = |span: [usize; 2]| &grammar.text[span[0]..span[1]];
    let local = |kind: &str, name: &str, line: &str| {
        let at = grammar.text.find(line).unwrap();
        locals
            .iter()
            .find(|l| l.kind == kind && l.name == name && l.definitions[0][0] >= at)
            .unwrap_or_else(|| panic!("{kind} {name}: {locals:?}"))
    };
    // A parameter used by a quote placeholder.
    let count = local("parameter", "Count", "syntax function FortnightDuration");
    assert_eq!(count.detail, "Count: Expr");
    assert_eq!(count.uses.len(), 1);
    assert_eq!(text(count.uses[0]), "Count");
    // A parameter used by the selector and the fallback; a capture used by its own arm.
    let value = local("parameter", "Value", "syntax function SwapAddition");
    assert_eq!(value.uses.len(), 2);
    let right = local("capture", "Right", "(Left: Expr, Right: Expr)");
    assert_eq!(right.uses.len(), 2);
    // A label used by a quote and by a helper argument.
    let amount = local("field", "Amount", "| Soon");
    assert_eq!(text(amount.uses[0]), "Amount");
    let inner = local("field", "Inner", "| Twice");
    assert_eq!(inner.uses.len(), 2);
    let value = local("field", "Value", "| Swapped");
    assert_eq!(value.uses.len(), 1);
}

#[test]
fn grammar_locals_bind_rule_check_names() {
    let grammar = source("languages/xml/Xml.ngr");
    let locals = grammar_locals(&grammar).unwrap();
    let text = |span: [usize; 2]| &grammar.text[span[0]..span[1]];
    let local = |kind: &str, name: &str| {
        locals
            .iter()
            .find(|l| l.kind == kind && l.name == name)
            .unwrap_or_else(|| panic!("{kind} {name}: {locals:?}"))
    };
    // A match capture, used by its pattern hole and its message.
    let pass = local("capture", "pass");
    assert_eq!(pass.detail, "pass:Text");
    assert_eq!(text(pass.definitions[0]), "pass");
    assert_eq!(pass.uses.len(), 2, "{pass:?}");
    assert!(pass.uses.iter().all(|&u| text(u) == "pass"));
    let after = grammar.text.find("$pass").unwrap();
    assert_eq!(pass.uses[0][0], after + 1);
    // Fields, used by a check's comparison, its message and its target.
    let open = local("field", "Open");
    assert_eq!(open.uses.len(), 2, "{open:?}");
    let close = local("field", "Close");
    assert_eq!(close.uses.len(), 3, "{close:?}");
}

#[test]
fn grammar_locals_of_every_language() {
    let languages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages");
    for language in std::fs::read_dir(languages).unwrap() {
        for file in std::fs::read_dir(language.unwrap().path()).unwrap() {
            let path = file.unwrap().path();
            if path.extension().is_some_and(|e| e == "ngr") {
                let grammar = Source {
                    path: path.display().to_string(),
                    text: std::fs::read_to_string(&path).unwrap(),
                };
                for local in grammar_locals(&grammar).unwrap_or_default() {
                    for span in local.uses {
                        assert_eq!(&grammar.text[span[0]..span[1]], local.name, "{path:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn grammar_references_include_function_calls() {
    let grammar = Source {
        path: "Calls.ngr".into(),
        text: "syntax module M\n{\n  function Twice(r: Grammar.Rule) : Grammar.Rule = r r;\n  \
               syntax function Same(Value: Expr) : Expr = <[ $Value ]>;\n  token N = ['0'..'9'];\n  \
               syntax Expr = Value:Twice(N) rewrites Same(Value);\n}\n"
            .into(),
    };
    let references = grammar_references(&grammar).unwrap();
    for name in ["Twice", "Same"] {
        let used = references.iter().find(|r| r.name == name).unwrap();
        assert_eq!(&grammar.text[used.span[0]..used.span[1]], name);
        assert_eq!(used.module, "M");
    }
    // `Same(Value: Expr) : Expr` uses Expr as its parameter and result type.
    let types = references.iter().filter(|r| r.name == "Expr").count();
    assert_eq!(types, 2, "{references:?}");
}
