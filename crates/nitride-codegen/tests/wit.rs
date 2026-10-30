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
    let ids: Vec<&str> = documents.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["Calendar.Clock", "Calendar.Gregorian"]);
    // Effects are documented on the functions that need them.
    assert!(
        documents[0]
            .1
            .contains("  /// Effects the host grants: Clock.\n  today: func() -> date;"),
        "{}",
        documents[0].1
    );
    let (_, document) = &documents[1];
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

/// Parses generated WIT with `wasm-tools` when it is installed; skipped
/// otherwise.
#[test]
fn generated_wit_parses_with_wasm_tools() {
    if std::process::Command::new("wasm-tools")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("wasm-tools not found; skipped");
        return;
    }
    let calendar = nitride_prepare::prepare_project(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../languages/calendar/Calendar.nproj"
    )))
    .unwrap()
    .data
    .to_string();
    let mut documents = wit(&calendar).unwrap();
    documents.extend(nitride_codegen::binding_wit(&calendar).unwrap());
    documents.extend(catalog_wit(
        "catalog module C { type List = represented i64;
           interface Type version 1 { operation Size(List) : Scalar; } }",
    ));
    let folder = std::env::temp_dir().join(format!("nitride-wit-{}", std::process::id()));
    for (i, (_, document)) in documents.iter().enumerate() {
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join(format!("{i}.wit"));
        std::fs::write(&file, document).unwrap();
        let output = std::process::Command::new("wasm-tools")
            .args(["component", "wit"])
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n---\n{document}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&folder);
}
