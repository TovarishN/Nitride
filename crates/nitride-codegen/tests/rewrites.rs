use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../languages/rewrites/language.json")).unwrap()
}

fn rule<'a>(d: &'a mut Value, name: &str) -> &'a mut Value {
    d["rules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["name"] == name)
        .unwrap()
}

#[test]
fn helper_tables_and_rewrite_actions_are_generated() {
    let generated = nitride_codegen::generate(&fixture().to_string()).unwrap();
    assert!(generated.contains(r#"nitride_core::Helper {name:"Rewrites.SwapAddition""#));
    assert!(generated.contains("selector:Some(0)"));
    // Inline templates become helpers named after their rule.
    assert!(generated.contains(r#"nitride_core::Helper {name:"Rewrites.Soon""#));
    assert!(generated.contains(r#"Action::Rewrite {helper:"#));
}

#[test]
fn malformed_helpers_and_rewrites_are_rejected() {
    let edits: Vec<fn(&mut Value)> = vec![
        |d| d["helpers"][0]["result"] = json!("Dates.Digit"),
        |d| d["helpers"][0]["parameters"][0]["category"] = json!("Dates.Missing"),
        |d| d["helpers"][0]["parameters"] = json!([{"name":"Count","category":"Dates.Expr"},{"name":"Count","category":"Dates.Expr"}]),
        |d| d["helpers"][1]["match"]["selector"] = json!("Missing"),
        |d| d["helpers"][1]["match"]["arms"][0]["captures"][0]["name"] = json!("Value"),
        |d| d["helpers"][1]["result"] = json!("Dates.Program"),
        |d| d["helpers"][1] = d["helpers"][0].clone(),
        |d| rule(d, "Rewrites.Fortnights")["clauses"][0]["target"] = json!("Rewrites.Missing"),
        |d| rule(d, "Rewrites.Fortnights")["clauses"][0]["args"] = json!([]),
        |d| rule(d, "Rewrites.Fortnights")["clauses"][0]["args"] = json!(["Missing"]),
        |d| {
            let clause = rule(d, "Rewrites.Fortnights")["clauses"][0].clone();
            rule(d, "Dates.Show")["clauses"] = json!([clause]);
        },
        |d| d["mode"] = json!("syntax"),
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
