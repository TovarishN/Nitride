use nitride_codegen::LoadedLanguage;
use nitride_lsp::roles::{Role, roles};
use nitride_prepare::{Prepared, Source, prepare_sources_detailed, prepare_sources_tolerant};
use std::collections::HashMap;
use std::path::Path;

const EMPTY: &str = r#"{"types":[],"operations":[],"statements":{}}"#;

fn read(path: &str) -> Source {
    let full = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../languages")
        .join(path);
    Source {
        path: path.into(),
        text: std::fs::read_to_string(full).unwrap(),
    }
}

/// (rule name, literal) → role for a prepared language.
fn table(prepared: &Prepared) -> HashMap<(String, String), Role> {
    let loaded = LoadedLanguage::load(&prepared.data.to_string()).unwrap();
    let language = loaded.language();
    let declares = |rule: &str| prepared.declarations.iter().any(|d| d.rule == rule);
    roles(language, &declares)
        .into_iter()
        .map(|((rule, literal), role)| {
            (
                (language.rules[rule as usize].name.to_owned(), literal),
                role,
            )
        })
        .collect()
}

fn typed(grammars: &[&str], catalog: &str, start: &str) -> HashMap<(String, String), Role> {
    let sources: Vec<_> = grammars.iter().map(|g| read(g)).collect();
    let catalog = read(catalog).text;
    table(&prepare_sources_detailed(&sources, &catalog, start, "typed").unwrap())
}

fn inline(text: &str, start: &str) -> HashMap<(String, String), Role> {
    let source = Source {
        path: "inline.ngr".into(),
        text: text.into(),
    };
    table(&prepare_sources_tolerant(&[source], EMPTY, start).unwrap())
}

fn role(table: &HashMap<(String, String), Role>, rule: &str, literal: &str) -> Role {
    *table
        .get(&(rule.to_owned(), literal.to_owned()))
        .unwrap_or_else(|| panic!("no role for {rule} {literal}"))
}

fn calls() -> HashMap<(String, String), Role> {
    typed(&["calls/Calls.ngr"], "calls/catalog.json", "Calls.Program")
}

#[test]
fn let_is_declaration_keyword() {
    assert_eq!(role(&calls(), "Calls.Let", "let"), Role::DeclarationKeyword);
}

#[test]
fn call_is_section_keyword() {
    assert_eq!(
        role(&calls(), "Calls.AnyCall", "call"),
        Role::SectionKeyword
    );
}

#[test]
fn duration_is_operation_word() {
    let portable = typed(
        &["portable/Portable.ngr"],
        "portable/catalog.json",
        "Portable.Program",
    );
    assert_eq!(
        role(&portable, "Portable.Duration", "duration"),
        Role::Operation
    );
    // Inside the operation's argument (none here) words would keep their positional role.
}

#[test]
fn punctuation_by_spelling() {
    let calls = calls();
    assert_eq!(role(&calls, "Calls.Let", ";"), Role::Terminator);
    assert_eq!(role(&calls, "Calls.Let", "="), Role::Operator);
    assert_eq!(role(&calls, "Calls.Call2", ","), Role::Separator);
    assert_eq!(role(&calls, "Calls.Call2", "("), Role::Parenthesis);
}

#[test]
fn word_choice_is_constant() {
    let grammar = "syntax module U\n{\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Unit*;\n  syntax Unit = Value:Number Scale:(\"days\" / \"weeks\");\n}\n";
    let table = inline(grammar, "U.Program");
    assert_eq!(role(&table, "U.Unit", "days"), Role::ConstantWord);
}

#[test]
fn modifier_after_element() {
    let grammar = "syntax module U\n{\n  token Number = ['0'..'9']+;\n  syntax Program = Items:Opt*;\n  syntax Opt = \"set\" Name:Number \"every\" Value:Number;\n}\n";
    let table = inline(grammar, "U.Program");
    assert_eq!(role(&table, "U.Opt", "set"), Role::SectionKeyword);
    assert_eq!(role(&table, "U.Opt", "every"), Role::ModifierWord);
}

#[test]
fn alias_literals_inline() {
    let grammar = "syntax module A\n{\n  syntax Program = Items:Setting*;\n  syntax Setting = \"mode\" Value:Mode;\n  syntax Mode = \"fast\" / \"slow\";\n}\n";
    let table = inline(grammar, "A.Program");
    assert_eq!(role(&table, "A.Setting", "fast"), Role::ConstantWord);
    assert_eq!(role(&table, "A.Setting", "mode"), Role::SectionKeyword);
}
