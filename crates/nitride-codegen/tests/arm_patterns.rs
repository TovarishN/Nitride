//! Named patterns in `syntax function` match arms, over the Dates profile.
use nitride_codegen::LoadedLanguage;
use nitride_core::*;
use nitride_prepare::*;
use std::path::Path;

fn read(path: &str) -> Source {
    let full = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path);
    Source {
        path: path.into(),
        text: std::fs::read_to_string(full).unwrap(),
    }
}

/// Dates plus a module `Arms` whose `pick(...)` rewrites through `Pick`.
fn language(members: &str, arms: &str) -> LoadedLanguage {
    let module = format!(
        "syntax module Arms
{{
  using Dates;
{members}
  syntax function Pick(Value: Expr) : Expr = match Value
  {{
{arms}
    | _ => <[ 0 ]>;
  }};
  extend syntax Dates.Expr
  {{
    | Picked = \"pick\" \"(\" Value:Expr \")\" rewrites Pick(Value)
  }}
}}"
    );
    let sources = [
        read("languages/dates/Dates.ngr"),
        Source {
            path: "Arms.ngr".into(),
            text: module,
        },
    ];
    let catalog = catalogs(&[read("languages/dates/Dates.ncatalog")]).unwrap();
    let data = prepare_sources(&sources, &catalog.to_string(), "Dates.Program", "typed").unwrap();
    LoadedLanguage::load(&data.to_string()).unwrap()
}

/// The value of each statement, or the compile error.
fn run(language: &LoadedLanguage, source: &str) -> Result<Vec<String>, String> {
    let language = language.language();
    let tree = parse(language, source, Limits::default()).map_err(|e| e.message)?;
    let program = compile(language, &tree, Limits::default()).map_err(|e| e[0].message.clone())?;
    let report = execute_with_values(
        &program,
        &host::bindings(language),
        &host::host_values(language, 739_912.0),
        Limits::default(),
    )
    .map_err(|f| f.diagnostic.message)?;
    Ok(report.values.iter().map(|v| v.to_string()).collect())
}

const SUM: &str = "  pattern Sum : Expr = (l: Expr, r: Expr) <[ $l + $r ]>;";
const PICK_SUM: &str = "    | (s: Sum) <[ $s ]> => <[ 1 ]>;";

#[test]
fn arm_uses_named_pattern() {
    let language = language(SUM, PICK_SUM);
    assert_eq!(run(&language, "pick(2 + 3);").unwrap(), ["1"]);
    assert_eq!(run(&language, "pick(5);").unwrap(), ["0"]);
}

#[test]
fn arm_named_pattern_alternatives_fall_through() {
    let members = "  pattern Arith : Expr =
    | (l: Expr, r: Expr) <[ $l * $r ]>
    | (l: Expr, r: Expr) <[ $l - $r ]>;";
    let language = language(members, "    | (s: Arith) <[ $s ]> => <[ 1 ]>;");
    assert_eq!(run(&language, "pick(2 * 3);").unwrap(), ["1"]);
    assert_eq!(run(&language, "pick(5 - 3);").unwrap(), ["1"]);
    assert_eq!(run(&language, "pick(5 + 3);").unwrap(), ["0"]);
}

/// A named pattern's alternatives keep check-style `..$x` spreads even when
/// an arm, whose own quote has none, uses it.
#[test]
fn arm_named_pattern_with_spreads() {
    let members = "  pattern AddAny : Expr = (l: Expr) <[ $l + ..$_ ]>;";
    let language = language(members, "    | (s: AddAny) <[ $s ]> => <[ 1 ]>;");
    assert_eq!(run(&language, "pick(2 + 3);").unwrap(), ["1"]);
    assert_eq!(run(&language, "pick(2 * 3);").unwrap(), ["0"]);
}

#[test]
fn arm_descendant_search() {
    let language = language(SUM, "    | (s: ..Sum) <[ ($s) ]> => <[ 1 ]>;");
    assert_eq!(run(&language, "pick(((2 + 3) * 4));").unwrap(), ["1"]);
    assert_eq!(run(&language, "pick((2 * 4));").unwrap(), ["0"]);
}

/// Running out of work fails the rewrite at its use, as Nitrogen's
/// `FileSemantics.Rewrites` returns the match's NQP0006 diagnostic as the
/// rewrite's failure; it does not fall through to the next arm.
#[test]
fn arm_budget_exhaustion() {
    let members = "  pattern Deep : Expr =
    | (n: Deep) <[ ($n) ]>
    | <[ 0 ]>;";
    let language = language(members, "    | (s: Deep) <[ $s ]> => <[ 1 ]>;");
    let deep = format!("pick({}0{});", "(".repeat(3000), ")".repeat(3000));
    let error = run(&language, &deep).unwrap_err();
    assert!(error.contains("NQP0006"), "{error}");
}
