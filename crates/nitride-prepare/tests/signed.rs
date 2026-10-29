use nitride_codegen::LoadedLanguage;
use nitride_core::*;
use nitride_prepare::*;

const EMPTY: &str = r#"{"types":[],"operations":[],"statements":{}}"#;

/// A signed grammar as the editor passes it: the `#!nitrogen grammar` line masked.
const SIGNED: &str = "                  \nlanguage Calc { start Calc.Program; grammars \"a.ngr\", \"b.ngr\"; extensions \".calc\"; namespace X.Y; sources \"S.cs\"; usings A.B; tokens value \"variable\"; }\nsyntax module Calc\n{\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Number*;\n}\n";

fn source() -> Source {
    Source {
        path: "Calc.syntax".into(),
        text: SIGNED.into(),
    }
}

#[test]
fn declaration_read() {
    let declaration = language_declaration(&source())
        .unwrap()
        .expect("a declaration");
    assert_eq!(declaration.name, "Calc");
    assert_eq!(
        &SIGNED[declaration.name_span[0]..declaration.name_span[1]],
        "Calc"
    );
    assert_eq!(declaration.start.as_deref(), Some("Calc.Program"));
    assert_eq!(declaration.grammars, ["a.ngr", "b.ngr"]);
    assert_eq!(declaration.extensions, [".calc"]);
}

#[test]
fn no_declaration_in_plain_grammar() {
    let plain = Source {
        path: "P.ngr".into(),
        text: "syntax module P\n{\n  token N = ['0'..'9']+;\n}\n".into(),
    };
    assert_eq!(language_declaration(&plain).unwrap(), None);
}

#[test]
fn preparation_ignores_declaration() {
    let prepared = prepare_sources_tolerant(&[source()], EMPTY, "Calc.Program").unwrap();
    assert!(prepared.warnings.is_empty(), "{:?}", prepared.warnings);
    let loaded = LoadedLanguage::load(&prepared.data.to_string()).unwrap();
    parse(loaded.language(), "1 2 3", Limits::default()).unwrap();
    prepare_sources(&[source()], EMPTY, "Calc.Program", "syntax").unwrap();
}

#[test]
fn outline_lists_language() {
    let outline = grammar_outline(&source()).unwrap();
    assert_eq!(
        (outline[0].kind, outline[0].name.as_str()),
        ("language", "Calc")
    );
    assert_eq!(
        (outline[1].kind, outline[1].name.as_str()),
        ("module", "Calc")
    );
}
