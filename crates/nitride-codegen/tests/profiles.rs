use nitride_codegen::generate;

#[test]
fn composed_typed_and_existing_syntax_profiles_generate() {
    for input in [
        include_str!("../../../languages/power/language.json"),
        include_str!("../../../languages/composition/language.json"),
        include_str!("../../../languages/xml/language.json"),
    ] {
        assert!(generate(input).is_ok());
    }
}

#[test]
fn syntax_profile_cannot_smuggle_semantic_actions() {
    let mut data: serde_json::Value =
        serde_json::from_str(include_str!("../../../languages/portable/language.json")).unwrap();
    data["mode"] = "syntax".into();
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn conflicting_infix_operators_are_rejected() {
    let mut data: serde_json::Value =
        serde_json::from_str(include_str!("../../../languages/portable/language.json")).unwrap();
    let mut twin = data["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Portable.Add")
        .unwrap()
        .clone();
    twin["name"] = "Portable.AddTwin".into();
    data["rules"].as_array_mut().unwrap().push(twin);
    for rule in data["rules"].as_array_mut().unwrap() {
        if rule["name"] == "Portable.Expr" {
            rule["expr"]["infixes"]
                .as_array_mut()
                .unwrap()
                .push("Portable.AddTwin".into());
        }
    }
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn reference_operator_infix_is_generated() {
    let hash = "0".repeat(64);
    let data = serde_json::json!({
        "version": 1, "mode": "syntax", "source_hash": hash, "catalog_hash": hash,
        "start": "M.E", "builtins": [], "symbol_kinds": [],
        "catalog": {"types": [], "operations": [], "statements": {}},
        "rules": [
            {"name":"M.MultiplyOp","lexical":true,"expr":{"kind":"literal","text":"*"},"clauses":[],"except":[]},
            {"name":"M.Num","lexical":false,"expr":{"kind":"class","negated":false,"ranges":[[48,57]]},"clauses":[],"except":[],"owner":"M.E","precedence":null},
            {"name":"M.Multiply","lexical":false,"expr":{"kind":"sequence","items":[
                {"kind":"label","label":"Left","inner":{"kind":"ref","name":"M.E"}},
                {"kind":"ref","name":"M.MultiplyOp"},
                {"kind":"label","label":"Right","inner":{"kind":"ref","name":"M.E"}}]},
             "clauses":[],"except":[],"owner":"M.E","precedence":160,"associativity":"Left"},
            {"name":"M.E","lexical":false,"expr":{"kind":"pratt","prefixes":["M.Num"],"infixes":["M.Multiply"]},"clauses":[],"except":[]}
        ]
    });
    nitride_codegen::generate(&data.to_string()).unwrap();
}
