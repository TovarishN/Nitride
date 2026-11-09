use nitride_core::{Limits, ParseOptions, TriviaPolicy, parse_with_options};
use nitride_prepare::{Source, prepare_sources};
use serde_json::Value;

fn prepared(token: &str, except: &str) -> Value {
    let source = nitride_stdlib::find("Nitride.Lexical", "1.0.0")
        .unwrap()
        .sources[0];
    prepare_sources(&[
        Source { path: source.uri.into(), text: source.text.into() },
        Source { path: "Test.ngr".into(), text: format!("syntax module Test {{ token Name = Nitride.Lexical.{token} {except}; syntax Start = Name; }}") },
    ], "{\"types\":[],\"operations\":[],\"statements\":{}}", "Test.Start", "syntax").unwrap()
}

#[test]
fn library_lexemes_preserve_policy_keyword_and_trivia_boundaries() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/stdlib_cases.json")).unwrap();
    for (token, cases) in cases.as_object().unwrap() {
        let data = prepared(token, "");
        let loaded = nitride_codegen::LoadedLanguage::load(&data.to_string()).unwrap();
        for (group, expected) in [("valid", true), ("invalid", false)] {
            for input in cases[group].as_array().unwrap() {
                let input = input.as_str().unwrap();
                assert_eq!(
                    parse_with_options(
                        loaded.language(),
                        input,
                        Limits::default(),
                        ParseOptions::default()
                    )
                    .is_ok(),
                    expected,
                    "{token}: {input:?}"
                );
            }
        }
    }
    let loaded = nitride_codegen::LoadedLanguage::load(
        &prepared("AsciiIdentifier", "except \"let\"").to_string(),
    )
    .unwrap();
    for (input, expected) in [
        ("let", false),
        ("letter", true),
        ("_let", true),
        ("let2", true),
    ] {
        assert_eq!(
            parse_with_options(
                loaded.language(),
                input,
                Limits::default(),
                ParseOptions::default()
            )
            .is_ok(),
            expected,
            "{input}"
        );
    }
    let mut data = prepared("UnsignedInteger", "");
    let loose = nitride_codegen::LoadedLanguage::load(&data.to_string()).unwrap();
    assert!(
        parse_with_options(
            loose.language(),
            "/*comment*/ 42",
            Limits::default(),
            ParseOptions::default()
        )
        .is_ok()
    );
    nitride_prepare::apply_trivia(&mut data, TriviaPolicy::Json);
    let strict = nitride_codegen::LoadedLanguage::load(&data.to_string()).unwrap();
    assert!(
        parse_with_options(
            strict.language(),
            "/*comment*/ 42",
            Limits::default(),
            ParseOptions::default()
        )
        .is_err()
    );
}
