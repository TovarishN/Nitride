use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../languages/dates/language.json")).unwrap()
}

fn rule<'a>(d: &'a mut Value, name: &str) -> &'a mut Value {
    d["rules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["name"] == name)
        .unwrap()
}

fn operation<'a>(d: &'a mut Value, id: &str) -> &'a mut Value {
    d["catalog"]["operations"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["id"] == id)
        .unwrap()
}

#[test]
fn value_check_and_text_tables_are_generated_from_explicit_catalog() {
    let generated = nitride_codegen::generate(&fixture().to_string()).unwrap();
    assert!(
        generated
            .contains(r#"nitride_core::BuiltinValue {kind:"value",name:"today",ty:"Dates.Date"}"#)
    );
    assert!(generated.contains("predicate:nitride_core::Predicate::IsoDate"));
    assert!(generated.contains("texts:&[true]"));
}

#[test]
fn malformed_value_check_and_text_metadata_is_rejected() {
    let edits: Vec<fn(&mut Value)> = vec![
        |d| d["catalog"]["values"]["value"]["pi"] = json!("Missing.Type"),
        |d| d["catalog"]["values"]["value"]["pi"] = json!("Core.Text"),
        |d| d["catalog"]["values"]["value"]["e"] = json!("Core.Scalar"),
        |d| d["catalog"]["values"]["missing"] = json!({}),
        |d| {
            d["catalog"]["values"]["value"]
                .as_object_mut()
                .unwrap()
                .remove("pi");
        },
        |d| d["catalog"]["builtins"] = json!({"value":{"pi":["Dates.AddScalar"]}}),
        |d| d["catalog"]["checks"]["Dates.Day"][0]["predicate"] = json!("csharp"),
        |d| d["catalog"]["checks"]["Dates.Day"][0]["field"] = json!("Missing"),
        |d| d["catalog"]["checks"]["Dates.Day"][0]["code"] = json!("DATE 1"),
        |d| d["catalog"]["checks"]["Dates.Day"][0]["message"] = json!(""),
        |d| d["catalog"]["checks"]["Dates.Date"] = d["catalog"]["checks"]["Dates.Day"].clone(),
        |d| d["catalog"]["checks"]["Dates.Missing"] = json!([]),
        |d| rule(d, "Dates.Day")["clauses"][0]["text_args"] = json!([1]),
        |d| rule(d, "Dates.Day")["clauses"][0]["text_args"] = json!([0, 0]),
        |d| rule(d, "Dates.Day")["clauses"][0]["text_args"] = json!([]),
        |d| {
            rule(d, "Dates.Day")["clauses"][0]
                .as_object_mut()
                .unwrap()
                .remove("text_args");
        },
        |d| rule(d, "Dates.Days")["clauses"][0]["text_args"] = json!([0]),
        |d| rule(d, "Dates.Add")["clauses"][0]["text_args"] = json!([0]),
        |d| d["catalog"]["overloads"]["Dates.Plus"] = json!(["Dates.ParseDate"]),
        |d| operation(d, "Dates.InDays")["result"] = json!("Core.Text"),
        |d| operation(d, "Dates.Days")["handler"] = json!("iso-date"),
        |d| operation(d, "Dates.ParseDate")["handler"] = json!("identity"),
        |d| rule(d, "Dates.Num")["clauses"][0]["target"] = json!("Core.Text"),
    ];
    for (index, edit) in edits.into_iter().enumerate() {
        let mut d = fixture();
        edit(&mut d);
        assert!(
            nitride_codegen::generate(&d.to_string()).is_err(),
            "variant {index} was admitted"
        );
    }
}

#[test]
fn syntax_only_profiles_cannot_carry_values_or_checks() {
    let mut d: Value =
        serde_json::from_str(include_str!("../../../languages/xml/language.json")).unwrap();
    d["catalog"]["checks"] =
        json!({"Xml.Document":[{"code":"X1","field":"Root","predicate":"iso-date","message":"m"}]});
    assert!(
        nitride_codegen::generate(&d.to_string())
            .unwrap_err()
            .contains("syntax-only")
    );
}
