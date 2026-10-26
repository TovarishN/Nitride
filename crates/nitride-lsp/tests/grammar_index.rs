mod support;
use nitride_lsp::grammar_index::{self, GrammarIndex};
use std::path::PathBuf;
use support::*;

fn power_set() -> (GrammarIndex, PathBuf, PathBuf) {
    let portable = repo().join("languages/portable/Portable.ngr");
    let power = repo().join("languages/power/Power.ngr");
    let index = grammar_index::build(&[portable.clone(), power.clone()], &|_| None);
    (index, portable, power)
}

fn offset_of(index: &GrammarIndex, file: usize, needle: &str, nth: usize) -> u32 {
    index.files[file]
        .text
        .match_indices(needle)
        .nth(nth)
        .unwrap()
        .0 as u32
}

#[test]
fn extend_target_resolves_across_files() {
    let (index, portable, _) = power_set();
    let at = offset_of(&index, 1, "Portable.Expr", 0) + 10;
    let rule = index.at(1, at).unwrap();
    assert_eq!(index.files[rule.file].path, portable);
    assert_eq!(
        (
            rule.module.as_str(),
            rule.item.name.as_str(),
            rule.item.kind
        ),
        ("Portable", "Expr", "extensible")
    );
}

#[test]
fn bare_name_prefers_module() {
    let (index, _, _) = power_set();
    let at = offset_of(&index, 0, "Value:Expr", 0) + 7;
    let rule = index.at(0, at).unwrap();
    assert_eq!(
        (rule.file, rule.module.as_str(), rule.item.name.as_str()),
        (0, "Portable", "Expr")
    );
}

#[test]
fn uses_span_files() {
    let (index, _, _) = power_set();
    let expr = index.definition("Portable.Expr", "Portable.Power").unwrap();
    let uses = index.uses(&expr);
    assert!(uses.iter().any(|(file, _)| *file == 0));
    assert!(uses.iter().any(|(file, _)| *file == 1));
    for (file, span) in uses {
        assert!(index.files[file].text[span[0] as usize..span[1] as usize].ends_with("Expr"));
    }
}
