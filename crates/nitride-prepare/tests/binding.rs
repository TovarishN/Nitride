use nitride_prepare::{Source, bindings, catalogs};
use serde_json::{Value, json};
use std::path::Path;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn source(path: &str, text: &str) -> Source {
    Source {
        path: path.into(),
        text: text.into(),
    }
}

const CATALOG: &str = "catalog module C {
  type Day = represented i64;
  interface Cal version 2
  {
    operation ParseDate(Text) : Day;
    operation Next(Day) : Day;
  }
}";

const BINDING: &str = "// A component under another name for one export.
binding C.Cal version 2 for wasm component \"cal.wasm\"
{
  ParseDate = parse-iso;
}";

fn catalog() -> Value {
    catalogs(&[source("C.ncatalog", CATALOG)]).unwrap()
}

#[test]
fn bindings_name_their_contract_component_and_exports() {
    let catalog = catalog();
    let lowered = bindings(&[source("cal.nbind", BINDING)], &catalog).unwrap();
    let hash = catalog["interfaces"]["C.Cal"]["hash"].as_str().unwrap();
    assert_eq!(
        lowered,
        json!([{
            "interface": "C.Cal",
            "contract": format!("C.Cal@{hash}"),
            "export": "nitride:c/cal@2.0.0",
            "target": "wasm-component",
            "component": "cal.wasm",
            "exports": {"C.ParseDate": "parse-iso", "C.Next": "next"},
        }])
    );
    // Without renames every export has its WIT name.
    let plain = "binding C.Cal version 2 for wasm component \"cal.wasm\";";
    let lowered = bindings(&[source("cal.nbind", plain)], &catalog).unwrap();
    assert_eq!(
        lowered[0]["exports"],
        json!({"C.ParseDate": "parse-date", "C.Next": "next"})
    );
    assert_eq!(bindings(&[], &catalog).unwrap(), json!([]));
}

#[test]
fn binding_errors_carry_the_binding_span() {
    let catalog = catalog();
    for (from, to, message, at) in [
        (
            "C.Cal version",
            "C.Week version",
            "unknown interface 'C.Week'",
            "C.Week",
        ),
        (
            "version 2",
            "version 1",
            "'C.Cal' is version 2 in the catalog",
            "1",
        ),
        (
            "ParseDate = parse-iso;",
            "Parse = parse-iso;",
            "'Parse' is not an operation of 'C.Cal'",
            "Parse",
        ),
        (
            "ParseDate = parse-iso;",
            "ParseDate = parse-iso; ParseDate = parse;",
            "'ParseDate' is already renamed",
            "ParseDate",
        ),
        (
            "\"cal.wasm\"",
            "\"\"",
            "a component path is not empty",
            "\"\"",
        ),
    ] {
        let text = BINDING.replace(from, to);
        assert_ne!(text, BINDING, "{from}");
        let error = bindings(&[source("cal.nbind", &text)], &catalog).unwrap_err();
        assert_eq!(error.message, message, "{to}");
        assert_eq!(&text[error.span[0]..error.span[1]], at, "{to}");
    }

    let error = bindings(
        &[source("a.nbind", BINDING), source("b.nbind", BINDING)],
        &catalog,
    )
    .unwrap_err();
    assert_eq!(error.message, "'C.Cal' is already bound in a.nbind");
    assert_eq!(error.path, "b.nbind");

    // Export names are WIT names.
    let text = BINDING.replace("parse-iso", "ParseIso");
    assert!(bindings(&[source("cal.nbind", &text)], &catalog).is_err());
}

#[test]
fn binding_frontend_reproduces_its_seed() {
    let prepared =
        nitride_prepare::prepare_project(&root().join("languages/binding/project.json")).unwrap();
    let generated = nitride_codegen::generate(&prepared.data.to_string()).unwrap();
    let seed =
        std::fs::read_to_string(root().join("crates/nitride-prepare/src/binding_seed.rs")).unwrap();
    assert_eq!(
        generated, seed,
        "run bootstrap regeneration after changing the binding grammar"
    );
}

#[test]
fn projects_record_their_checked_bindings() {
    for manifest in ["Calendar.nproj", "project.json"] {
        let prepared =
            nitride_prepare::prepare_project(&root().join("languages/calendar").join(manifest))
                .unwrap();
        let bindings = prepared.data["bindings"].as_array().unwrap();
        assert_eq!(bindings.len(), 1, "{manifest}");
        assert_eq!(bindings[0]["exports"]["Calendar.ParseDate"], "parse-iso");
        assert!(
            prepared
                .inputs
                .iter()
                .any(|p| p.ends_with("calendar/gregorian.nbind")),
            "{manifest}"
        );
    }
    // Projects without bindings prepare as before.
    let dates =
        nitride_prepare::prepare_project(&root().join("languages/dates/Dates.nproj")).unwrap();
    assert!(dates.data.get("bindings").is_none());
}
