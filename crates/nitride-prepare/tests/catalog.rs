use nitride_prepare::{Source, catalogs};
use serde_json::{Value, json};
use std::path::Path;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sources(files: &[(&str, &str)]) -> Vec<Source> {
    files
        .iter()
        .map(|(path, text)| Source {
            path: (*path).into(),
            text: (*text).into(),
        })
        .collect()
}

fn lower(files: &[(&str, &str)]) -> Result<Value, String> {
    catalogs(&sources(files)).map_err(|e| e.message)
}

#[test]
fn every_catalog_lowers_to_its_json() {
    for (project, file) in [
        ("calls", "Calls.ncatalog"),
        ("dates", "Dates.ncatalog"),
        ("functions", "Functions.ncatalog"),
        ("inference", "Inference.ncatalog"),
        ("namespaces", "Namespaces.ncatalog"),
        ("portable", "Portable.ncatalog"),
        ("power", "Power.ncatalog"),
    ] {
        let dir = root().join("languages").join(project);
        let path = dir.join(file);
        let source = Source {
            path: path.display().to_string(),
            text: std::fs::read_to_string(&path).unwrap(),
        };
        let lowered = catalogs(&[source]).unwrap_or_else(|e| panic!("{e}"));
        let expected: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("catalog.json")).unwrap())
                .unwrap();
        assert_eq!(lowered, expected, "{project}");
    }
}

#[test]
fn no_catalogs_are_the_empty_syntax_only_catalog() {
    let empty: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("languages/grammar/catalog.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(catalogs(&[]).unwrap(), empty);
}

const DATES: &str = "catalog module Dates {
  type Date; type Duration;
  operation Between(Date, Date) : Duration = host sub;
  operation Scale(Scalar, Duration) : Duration = host mul;
  let Let(Value) infer;
}";

#[test]
fn using_makes_another_modules_names_visible() {
    let catalog = lower(&[
        ("Dates.ncatalog", DATES),
        (
            "Rewrites.ncatalog",
            "catalog module Rewrites { using Dates;
               type Week;
               operation Weeks(Duration) : Week = host identity;
               overload Span = Weeks | Between | Dates.Scale;
               show Show(Value); }",
        ),
    ])
    .unwrap();
    assert_eq!(
        catalog["types"],
        json!([
            "Core.Scalar",
            "Dates.Date",
            "Dates.Duration",
            "Rewrites.Week"
        ])
    );
    let ids: Vec<_> = catalog["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["Dates.Between", "Dates.Scale", "Rewrites.Weeks"]);
    assert_eq!(
        catalog["operations"][2]["inputs"],
        json!(["Dates.Duration"])
    );
    assert_eq!(
        catalog["overloads"]["Rewrites.Span"],
        json!(["Rewrites.Weeks", "Dates.Between", "Dates.Scale"])
    );
    assert_eq!(catalog["statements"]["Dates.Let"]["kind"], "let");
    assert_eq!(catalog["statements"]["Rewrites.Show"]["kind"], "show");
}

#[test]
fn qualified_names_need_no_using_and_using_may_be_mutual() {
    let catalog = lower(&[
        (
            "A.ncatalog",
            "catalog module A { using B; type T; operation F(U) : T = host identity; }",
        ),
        (
            "B.ncatalog",
            "catalog module B { using A; type U; operation G(A.T) : U = host identity; }",
        ),
    ])
    .unwrap();
    assert_eq!(catalog["operations"][0]["inputs"], json!(["B.U"]));
    assert_eq!(catalog["operations"][1]["inputs"], json!(["A.T"]));
    let catalog = lower(&[
        ("A.ncatalog", "catalog module A { type T; }"),
        (
            "B.ncatalog",
            "catalog module B { operation G(A.T) : Text = host iso-date; }",
        ),
    ])
    .unwrap();
    assert_eq!(catalog["types"], json!(["Core.Scalar", "Core.Text", "A.T"]));
}

#[test]
fn own_members_shadow_used_ones_and_two_used_ones_are_ambiguous() {
    let a = ("A.ncatalog", "catalog module A { type Time; }");
    let b = ("B.ncatalog", "catalog module B { type Time; }");
    let own = lower(&[
        a,
        (
            "C.ncatalog",
            "catalog module C { using A; type Time; operation F(Time) : Time = host identity; }",
        ),
    ])
    .unwrap();
    assert_eq!(own["operations"][0]["result"], "C.Time");
    let error = lower(&[
        a,
        b,
        (
            "C.ncatalog",
            "catalog module C { using A; using B; operation F(Time) : Scalar = host identity; }",
        ),
    ])
    .unwrap_err();
    assert_eq!(error, "'Time' is defined in both 'A' and 'B'");
}

#[test]
fn resolution_errors_carry_the_catalog_span() {
    let text = "catalog module A { operation F(Missing) : Scalar = host identity; }";
    let error = catalogs(&sources(&[("A.ncatalog", text)])).unwrap_err();
    assert_eq!(error.message, "unknown type 'Missing'");
    assert_eq!(&text[error.span[0]..error.span[1]], "Missing");
    assert_eq!(error.path, "A.ncatalog");
    for (body, message) in [
        ("using B;", "catalog module 'B' is not in this project"),
        ("overload G = Nope;", "unknown operation 'Nope'"),
        (
            "type T; using B;",
            "'using' must precede the module's members",
        ),
        (
            "operation F() : Scalar = host random;",
            "unknown operation handler 'random'; expected one of zero, identity, neg, add, sub, mul, div, pow, iso-date",
        ),
        (
            "builtin value pi : Scalar = host random;",
            "unknown value handler 'random'; expected one of pi, e, tau, today",
        ),
        (
            "check Day is iso-date else X1 \"m\";",
            "a check names its field: 'Rule.Field'",
        ),
    ] {
        let error =
            lower(&[("A.ncatalog", &format!("catalog module A {{ {body} }}"))]).unwrap_err();
        assert_eq!(error, message, "{body}");
    }
}

#[test]
fn merging_keeps_manifest_order() {
    let catalog = lower(&[
        (
            "B.ncatalog",
            "catalog module B { type U; operation G() : Scalar = host zero; }",
        ),
        (
            "A.ncatalog",
            "catalog module A { type T; operation F() : Scalar = host zero; }",
        ),
    ])
    .unwrap();
    assert_eq!(catalog["types"], json!(["Core.Scalar", "B.U", "A.T"]));
    assert_eq!(catalog["operations"][0]["id"], "B.G");
    assert_eq!(catalog["operations"][1]["id"], "A.F");
}

#[test]
fn conflicting_declarations_are_rejected() {
    for (files, message) in [
        (
            vec!["catalog module A { type T; type T; }"],
            "'T' is already declared at 0.ncatalog:1",
        ),
        (
            vec!["catalog module A { operation F() : Scalar = host zero; overload F = F; }"],
            "'F' is already declared at 0.ncatalog:1",
        ),
        (
            vec!["catalog module A { }", "catalog module A { }"],
            "catalog module 'A' is already declared at 0.ncatalog:1",
        ),
        (
            vec![
                "catalog module A { let Dates.Let(Value); }",
                "catalog module B { show Dates.Let(Value); }",
            ],
            "a statement policy for 'Dates.Let' is already declared at 0.ncatalog:1",
        ),
        (
            vec![
                "catalog module A { operation F() : Scalar = host zero; builtin function f = F; }",
                "catalog module B { builtin function f = A.F; }",
            ],
            "builtin function 'f' is already declared at 0.ncatalog:1",
        ),
        (
            vec!["catalog module A { call C(X); call C(Y); }"],
            "a call for 'A.C' is already declared at 0.ncatalog:1",
        ),
        (
            vec![
                "catalog module A { check D.V is iso-date else X1 \"a\"; check D.V is iso-date else X2 \"b\"; }",
            ],
            "a check on 'A.D.V' is already declared at 0.ncatalog:1",
        ),
    ] {
        let names: Vec<_> = (0..files.len()).map(|i| format!("{i}.ncatalog")).collect();
        let files: Vec<_> = names.iter().map(String::as_str).zip(files).collect();
        assert_eq!(lower(&files).unwrap_err(), message);
    }
}

#[test]
fn checks_on_two_fields_of_a_rule_share_its_entry() {
    let catalog = lower(&[(
        "A.ncatalog",
        "catalog module A { check D.V is iso-date else X1 \"a \\\"b\\\"\"; check D.W is iso-date else X2 \"c\"; }",
    )])
    .unwrap();
    assert_eq!(
        catalog["checks"]["A.D"],
        json!([
            {"code":"X1","field":"V","predicate":"iso-date","message":"a \"b\""},
            {"code":"X2","field":"W","predicate":"iso-date","message":"c"}
        ])
    );
}

#[test]
fn syntax_errors_and_keyword_boundaries() {
    assert!(lower(&[("A.ncatalog", "catalog module A { typeT; }")]).is_err());
    assert!(lower(&[("A.ncatalog", "catalogmodule A { }")]).is_err());
    assert!(lower(&[("A.ncatalog", "catalog module A { type T }")]).is_err());
    let catalog = lower(&[(
        "A.ncatalog",
        "// c\ncatalog module A { /* c */ type typeX; }",
    )])
    .unwrap();
    assert_eq!(catalog["types"], json!(["Core.Scalar", "A.typeX"]));
}

#[test]
fn catalog_frontend_reproduces_its_seed() {
    let prepared =
        nitride_prepare::prepare_project(&root().join("languages/catalog/project.json")).unwrap();
    let generated = nitride_codegen::generate(&prepared.data.to_string()).unwrap();
    let seed =
        std::fs::read_to_string(root().join("crates/nitride-prepare/src/catalog_seed.rs")).unwrap();
    assert_eq!(
        generated, seed,
        "run bootstrap regeneration after changing the catalog grammar"
    );
}
