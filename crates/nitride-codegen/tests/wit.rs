//! WIT worlds generated from catalog interfaces.
use nitride_codegen::{wit, wit_name};
use serde_json::json;

fn catalog_wit(text: &str) -> Vec<(String, String)> {
    let catalog = nitride_prepare::catalogs(&[nitride_prepare::Source {
        path: "C.ncatalog".into(),
        text: text.into(),
    }])
    .unwrap();
    wit(&json!({ "catalog": catalog }).to_string()).unwrap()
}

#[test]
fn names_are_kebab_case_and_keywords_are_escaped() {
    assert_eq!(wit_name("ParseDate"), "parse-date");
    assert_eq!(wit_name("LeapYear"), "leap-year");
    assert_eq!(wit_name("date_2"), "date2");
    assert_eq!(wit_name("List"), "%list");
    assert_eq!(wit_name("Type"), "%type");
}

#[test]
fn calendar_exports_its_interface_as_a_world() {
    let prepared = nitride_prepare::prepare_project(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../languages/calendar/Calendar.nproj"
    )))
    .unwrap();
    let documents = wit(&prepared.data.to_string()).unwrap();
    assert_eq!(documents.len(), 1);
    let (id, document) = &documents[0];
    assert_eq!(id, "Calendar.Gregorian");
    let hash = prepared.data["catalog"]["interfaces"]["Calendar.Gregorian"]["hash"]
        .as_str()
        .unwrap();
    for line in [
        format!("// Contract Calendar.Gregorian@{hash}"),
        "package nitride:calendar@3.0.0;".into(),
        "interface gregorian {".into(),
        "  type date = s64;".into(),
        "  type weekday = string;".into(),
        "  record span {\n    start: date,\n    end: date,\n  }".into(),
        "  type dates = list<date>;".into(),
        "  enum gregorian-failure {\n    invalid-date,\n    out-of-range,\n    partial-day,\n  }"
            .into(),
        "  parse-date: func(text: string) -> result<date, gregorian-failure>;".into(),
        "  between: func(date1: date, date2: date) -> result<duration, gregorian-failure>;".into(),
        "  mondays: func(span: span) -> result<dates, gregorian-failure>;".into(),
        "world implementation {\n  export gregorian;\n}".into(),
    ] {
        assert!(document.contains(&line), "{line}\n---\n{document}");
    }
    // Components come before the types that use them.
    assert!(document.find("type date").unwrap() < document.find("record span").unwrap());
}

#[test]
fn interfaces_without_failures_return_plain_values() {
    let documents = catalog_wit(
        "catalog module C { type List = represented i64;
           interface Type version 1 { operation Size(List) : Scalar; } }",
    );
    let (_, document) = &documents[0];
    assert!(document.contains("interface %type {"), "{document}");
    assert!(document.contains("  type %list = s64;"), "{document}");
    assert!(document.contains("  type scalar = f64;"), "{document}");
    assert!(
        document.contains("  size: func(%list: %list) -> scalar;"),
        "{document}"
    );
    assert!(!document.contains("enum"), "{document}");
    assert!(catalog_wit("catalog module C { type T; }").is_empty());
}
