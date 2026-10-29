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

/// The catalog.json files these catalogs replaced, lowered byte-for-byte equal
/// before the JSON was deleted; Dates and Calls together use every section.
const DATES_JSON: &str = r#"{"types":["Core.Scalar","Core.Text","Dates.Date","Dates.Duration"],"operations":[{"id":"Dates.ParseDate","inputs":["Core.Text"],"result":"Dates.Date","handler":"iso-date"},{"id":"Dates.Days","inputs":["Core.Scalar"],"result":"Dates.Duration","handler":"identity"},{"id":"Dates.InDays","inputs":["Dates.Duration"],"result":"Core.Scalar","handler":"identity"},{"id":"Dates.AddScalar","inputs":["Core.Scalar","Core.Scalar"],"result":"Core.Scalar","handler":"add"},{"id":"Dates.Later","inputs":["Dates.Date","Dates.Duration"],"result":"Dates.Date","handler":"add"},{"id":"Dates.AddDurations","inputs":["Dates.Duration","Dates.Duration"],"result":"Dates.Duration","handler":"add"},{"id":"Dates.SubScalar","inputs":["Core.Scalar","Core.Scalar"],"result":"Core.Scalar","handler":"sub"},{"id":"Dates.Earlier","inputs":["Dates.Date","Dates.Duration"],"result":"Dates.Date","handler":"sub"},{"id":"Dates.Between","inputs":["Dates.Date","Dates.Date"],"result":"Dates.Duration","handler":"sub"},{"id":"Dates.SubDurations","inputs":["Dates.Duration","Dates.Duration"],"result":"Dates.Duration","handler":"sub"},{"id":"Dates.MulScalar","inputs":["Core.Scalar","Core.Scalar"],"result":"Core.Scalar","handler":"mul"},{"id":"Dates.Scale","inputs":["Core.Scalar","Dates.Duration"],"result":"Dates.Duration","handler":"mul"}],"overloads":{"Dates.Plus":["Dates.AddScalar","Dates.Later","Dates.AddDurations"],"Dates.Minus":["Dates.SubScalar","Dates.Earlier","Dates.Between","Dates.SubDurations"],"Dates.Times":["Dates.MulScalar","Dates.Scale"]},"statements":{"Dates.Let":{"kind":"let","value":"Value","infer":true},"Dates.Show":{"kind":"show","value":"Value"}},"values":{"value":{"pi":{"type":"Core.Scalar","handler":"pi"},"today":{"type":"Dates.Date","handler":"today"}}},"checks":{"Dates.Day":[{"code":"DATE0001","field":"Value","predicate":"iso-date","message":"not a calendar date"}]}}"#;
const CALLS_JSON: &str = r#"{"types":["Core.Scalar","Calls.Time"],"operations":[{"id":"Calls.Duration","inputs":["Core.Scalar"],"result":"Calls.Time","handler":"identity"},{"id":"Calls.NegScalar","inputs":["Core.Scalar"],"result":"Core.Scalar","handler":"neg"},{"id":"Calls.NegTime","inputs":["Calls.Time"],"result":"Calls.Time","handler":"neg"},{"id":"Calls.SumScalar","inputs":["Core.Scalar","Core.Scalar"],"result":"Core.Scalar","handler":"add"},{"id":"Calls.SumTime","inputs":["Calls.Time","Calls.Time"],"result":"Calls.Time","handler":"add"},{"id":"Calls.Zero","inputs":[],"result":"Core.Scalar","handler":"zero"}],"statements":{"Calls.Let":{"kind":"let","value":"Value","infer":true},"Calls.Local":{"kind":"let","value":"Value","infer":true},"Calls.Show":{"kind":"show","value":"Value"}},"builtins":{"function":{"neg":["Calls.NegScalar","Calls.NegTime"],"sys.neg":["Calls.NegScalar","Calls.NegTime"],"sum":["Calls.SumScalar","Calls.SumTime"],"duration":["Calls.Duration"],"zero":["Calls.Zero"],"calc":["Calls.Zero","Calls.NegScalar","Calls.SumScalar"]}},"calls":{"Calls.Call0":{"args":[]},"Calls.Call1":{"args":["Arg"]},"Calls.Call2":{"args":["Left","Right"]},"Calls.AnyCall":{"args":["Arg"]},"Calls.FallbackCall":{"args":["Arg"]}}}"#;

#[test]
fn catalogs_lower_to_the_json_they_replaced() {
    for (project, file, expected) in [
        ("dates", "Dates.ncatalog", DATES_JSON),
        ("calls", "Calls.ncatalog", CALLS_JSON),
    ] {
        let path = root().join("languages").join(project).join(file);
        let source = Source {
            path: path.display().to_string(),
            text: std::fs::read_to_string(&path).unwrap(),
        };
        let lowered = catalogs(&[source]).unwrap_or_else(|e| panic!("{e}"));
        let expected: Value = serde_json::from_str(expected).unwrap();
        assert_eq!(lowered, expected, "{project}");
    }
}

#[test]
fn no_catalogs_are_the_empty_syntax_only_catalog() {
    assert_eq!(
        catalogs(&[]).unwrap(),
        json!({"types":[],"operations":[],"statements":{}})
    );
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

const CONTRACT: &str = "catalog module C {
  type Date;
  failure Invalid \"not a date\";
  interface Cal version 2
  {
    operation Parse(Text) : Date fails Invalid;
    operation Next(Date) : Date;
    example Next(Parse(\"1\")) == Parse(\"2\");
    example Parse(\"x\") fails Invalid;
  }
  operation Days(Scalar) : Scalar = host identity;
}";

#[test]
fn interfaces_lower_with_their_contract_hash_and_examples() {
    let lowered = lower(&[("C.ncatalog", CONTRACT)]).unwrap();
    assert_eq!(
        lowered["operations"][0],
        json!({"id":"C.Parse","inputs":["Core.Text"],"result":"C.Date","interface":"C.Cal","fails":["C.Invalid"]})
    );
    assert_eq!(lowered["operations"][2]["handler"], "identity");
    assert_eq!(lowered["failures"], json!({"C.Invalid":"not a date"}));
    let interface = &lowered["interfaces"]["C.Cal"];
    assert_eq!(interface["version"], 2);
    assert_eq!(interface["operations"], json!(["C.Parse", "C.Next"]));
    assert_eq!(interface["hash"].as_str().unwrap().len(), 64);
    assert_eq!(
        interface["examples"][0],
        json!({"source":"example Next(Parse(\"1\")) == Parse(\"2\");",
            "value":{"call":"C.Next","args":[{"call":"C.Parse","args":[{"text":"1"}]}]},
            "equals":{"call":"C.Parse","args":[{"text":"2"}]}})
    );
    assert_eq!(interface["examples"][1]["fails"], "C.Invalid");

    // Examples are not part of the contract; signatures are.
    let fewer = CONTRACT.replace("    example Parse(\"x\") fails Invalid;\n", "");
    let same = lower(&[("C.ncatalog", &fewer)]).unwrap();
    assert_eq!(same["interfaces"]["C.Cal"]["hash"], interface["hash"]);
    let wider = CONTRACT.replace("Next(Date) : Date;", "Next(Date) : Date fails Invalid;");
    let other = lower(&[("C.ncatalog", &wider)]).unwrap();
    assert_ne!(other["interfaces"]["C.Cal"]["hash"], interface["hash"]);
}

#[test]
fn interface_errors_carry_the_catalog_span() {
    for (from, to, message, at) in [
        (
            "fails Invalid;",
            "fails Missing;",
            "unknown failure 'Missing'",
            "Missing",
        ),
        (
            "Next(Parse(\"1\")) ==",
            "Next(Parse(\"1\"), 1) ==",
            "'Next' takes 1 inputs, not 2",
            "Next",
        ),
        (
            "== Parse(\"2\");",
            "== Days(2);",
            "the two sides of an example are 'C.Date' and 'Core.Scalar'",
            "example Next(Parse(\"1\")) == Days(2);",
        ),
        (
            "example Parse(\"x\") fails Invalid;",
            "example Next(Parse(\"x\")) fails Nope;",
            "unknown failure 'Nope'",
            "Nope",
        ),
        (
            "Next(Parse(\"1\"))",
            "Next(1)",
            "input 1 of 'Next' is 'C.Date', not 'Core.Scalar'",
            "Next",
        ),
        (
            "version 2",
            "version 0",
            "an interface version is a positive integer",
            "0",
        ),
        (
            "type Date;",
            "type Date; failure Invalid \"again\";",
            "'Invalid' is already declared at C.ncatalog:2",
            "Invalid",
        ),
    ] {
        let text = CONTRACT.replace(from, to);
        assert_ne!(text, CONTRACT, "{from}");
        let error = catalogs(&sources(&[("C.ncatalog", &text)])).unwrap_err();
        assert_eq!(error.message, message, "{to}");
        assert_eq!(&text[error.span[0]..error.span[1]], at, "{to}");
    }

    let unreported = CONTRACT.replace(
        "example Parse(\"x\") fails Invalid;",
        "example Days(1) fails Invalid;",
    );
    let error = catalogs(&sources(&[("C.ncatalog", &unreported)])).unwrap_err();
    assert_eq!(
        error.message,
        "no operation in this example reports 'Invalid'"
    );

    let empty = "catalog module C { interface Cal version 1 { } }";
    let error = catalogs(&sources(&[("C.ncatalog", empty)])).unwrap_err();
    assert_eq!(error.message, "interface 'C.Cal' declares no operations");
    assert_eq!(&empty[error.span[0]..error.span[1]], "Cal");
}

const REPRESENTED: &str = "catalog module R {
  type Day = represented i64;
  type Name = represented text;
  type Flag = represented bool;
  type Plain = represented f64;
  interface Cal version 1
  {
    operation Parse(Text) : Day;
    operation Named(Day) : Name;
    operation Odd(Day) : Flag;
    example Named(Parse(\"1\")) == \"one\";
    example Odd(Parse(\"1\")) == true;
  }
}";

#[test]
fn representations_are_lowered_and_part_of_the_contract() {
    let lowered = lower(&[("R.ncatalog", REPRESENTED)]).unwrap();
    // f64 is the default and is not recorded.
    assert_eq!(
        lowered["representations"],
        json!({"R.Day":"i64","R.Flag":"bool","R.Name":"text"})
    );
    let examples = &lowered["interfaces"]["R.Cal"]["examples"];
    assert_eq!(examples[0]["equals"], json!({"text":"one"}));
    assert_eq!(examples[1]["equals"], json!({"boolean":true}));

    let hash = &lowered["interfaces"]["R.Cal"]["hash"];
    let wider = REPRESENTED.replace("Day = represented i64", "Day = represented f64");
    let other = lower(&[("R.ncatalog", &wider)]).unwrap();
    assert_ne!(&other["interfaces"]["R.Cal"]["hash"], hash);
    assert!(
        other
            .get("representations")
            .is_some_and(|r| r.get("R.Day").is_none())
    );

    // Catalogs without representations lower as before.
    assert!(
        lower(&[("C.ncatalog", CONTRACT)])
            .unwrap()
            .get("representations")
            .is_none()
    );
}

#[test]
fn representation_errors_carry_the_catalog_span() {
    for (from, to, message, at) in [
        (
            "Day = represented i64",
            "Day = represented u8",
            "unknown representation 'u8'; expected one of f64, i64, bool, text",
            "u8",
        ),
        (
            "== true;",
            "== \"yes\";",
            "the two sides of an example are 'R.Flag' and 'Core.Text'",
            "example Odd(Parse(\"1\")) == \"yes\";",
        ),
        (
            "== \"one\";",
            "== false;",
            "the two sides of an example are 'R.Name' and 'a boolean literal'",
            "example Named(Parse(\"1\")) == false;",
        ),
        (
            "example Odd(Parse(\"1\")) == true;",
            "example true == true;",
            "an example's value cannot be a text or boolean literal",
            "example true == true;",
        ),
    ] {
        let text = REPRESENTED.replace(from, to);
        assert_ne!(text, REPRESENTED, "{from}");
        let error = catalogs(&sources(&[("R.ncatalog", &text)])).unwrap_err();
        assert_eq!(error.message, message, "{to}");
        assert_eq!(&text[error.span[0]..error.span[1]], at, "{to}");
    }
}
