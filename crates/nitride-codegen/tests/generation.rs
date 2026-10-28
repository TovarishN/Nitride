use nitride_codegen::generate;

#[test]
fn generates_static_language_from_export() {
    let source = &nitride_prepare::prepare_project(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../languages/portable/project.json"
    )))
    .unwrap()
    .data
    .to_string();
    let result = generate(source).expect("supported normalized grammar");
    assert!(result.contains("pub static LANGUAGE"));
    assert_eq!(generate(source).unwrap(), result);
}

#[test]
fn unknown_format_version_is_rejected() {
    let mut data: serde_json::Value = serde_json::from_str(
        &nitride_prepare::prepare_project(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../languages/portable/project.json"
        )))
        .unwrap()
        .data
        .to_string(),
    )
    .unwrap();
    data["version"] = 99.into();
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn zero_width_repeat_is_rejected_even_after_consuming_prefix() {
    let mut data: serde_json::Value = serde_json::from_str(
        &nitride_prepare::prepare_project(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../languages/portable/project.json"
        )))
        .unwrap()
        .data
        .to_string(),
    )
    .unwrap();
    data["rules"][0]["expr"] = serde_json::json!({"kind":"sequence","items":[{"kind":"literal","text":"x"},{"kind":"repeat","min":0,"max":100,"inner":{"kind":"literal","text":""}}]});
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn unknown_operation_type_and_field_are_rejected() {
    let original: serde_json::Value = serde_json::from_str(
        &nitride_prepare::prepare_project(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../languages/portable/project.json"
        )))
        .unwrap()
        .data
        .to_string(),
    )
    .unwrap();
    let mut data = original.clone();
    data["catalog"]["operations"][0]["result"] = "Missing.Type".into();
    assert!(generate(&data.to_string()).is_err());
    let mut data = original;
    data["catalog"]["statements"]["Portable.Show"]["value"] = "Missing".into();
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn incompatible_symbol_kinds_are_not_erased() {
    let mut data: serde_json::Value = serde_json::from_str(
        &nitride_prepare::prepare_project(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../languages/portable/project.json"
        )))
        .unwrap()
        .data
        .to_string(),
    )
    .unwrap();
    for rule in data["rules"].as_array_mut().unwrap() {
        for clause in rule["clauses"].as_array_mut().unwrap() {
            if clause["kind"] == "References" {
                clause["symbol_kinds"] = serde_json::json!(["other"]);
            }
        }
    }
    assert!(generate(&data.to_string()).is_err());
}

#[test]
fn unknown_or_incompatible_fixture_host_handlers_fail_generation() {
    for handler in ["add", "unknown"] {
        let mut data: serde_json::Value = serde_json::from_str(
            &nitride_prepare::prepare_project(std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../languages/portable/project.json"
            )))
            .unwrap()
            .data
            .to_string(),
        )
        .unwrap();
        data["catalog"]["operations"][0]["handler"] = handler.into();
        assert!(generate(&data.to_string()).is_err());
    }
}

#[test]
fn emits_native_scanners_only_for_acyclic_lexical_graphs() {
    let mut data: serde_json::Value = serde_json::from_str(
        &nitride_prepare::prepare_project(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../languages/portable/project.json"
        )))
        .unwrap()
        .data
        .to_string(),
    )
    .unwrap();
    let base = data["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Portable.Let")
        .unwrap()
        .clone();
    let rules = data["rules"].as_array_mut().unwrap();
    for (name, expr) in [
        (
            "Native.Leaf",
            serde_json::json!({"kind":"class","ranges":[[97,122],[65,90]],"negated":false}),
        ),
        (
            "Native.Outer",
            serde_json::json!({"kind":"repeat","min":1,"max":10,"inner":{"kind":"ref","name":"Native.Leaf"}}),
        ),
        (
            "Native.SyntaxRef",
            serde_json::json!({"kind":"ref","name":base["name"]}),
        ),
        (
            "Native.Cycle",
            serde_json::json!({"kind":"sequence","items":[{"kind":"literal","text":"a"},{"kind":"repeat","min":0,"max":1,"inner":{"kind":"ref","name":"Native.Cycle"}}]}),
        ),
    ] {
        let mut r = base.clone();
        r["name"] = name.into();
        r["lexical"] = true.into();
        r["expr"] = expr;
        r["clauses"] = serde_json::json!([]);
        r["owner"] = serde_json::Value::Null;
        rules.push(r);
    }
    let n = rules.len();
    let result = generate(&data.to_string()).unwrap();
    assert!(
        result.contains("pub static LEXERS"),
        "no compiled scanner table"
    );
    assert!(result.contains(&format!("fn nitride_lex_{}(", n - 4)));
    assert!(result.contains(&format!("fn nitride_lex_{}(", n - 3)));
    assert!(!result.contains(&format!("fn nitride_lex_{}(", n - 2)));
    assert!(!result.contains(&format!("fn nitride_lex_{}(", n - 1)));
}

#[test]
fn generates_syntax_bodies_and_infix_tails_with_bounded_fallback() {
    let mut data: serde_json::Value = serde_json::from_str(
        &nitride_prepare::prepare_project(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../languages/portable/project.json"
        )))
        .unwrap()
        .data
        .to_string(),
    )
    .unwrap();
    let rules = data["rules"].as_array().unwrap();
    let id = |name: &str| rules.iter().position(|r| r["name"] == name).unwrap();
    let (ordinary, add, pratt, lexical) = (
        id("Portable.Let"),
        id("Portable.Add"),
        id("Portable.Expr"),
        id("Portable.Number"),
    );
    let mut oversized = rules[ordinary].clone();
    oversized["name"] = "Native.DeepSyntax".into();
    oversized["clauses"] = serde_json::json!([]);
    let mut expr = serde_json::json!({"kind":"literal","text":"x"});
    for _ in 0..65 {
        expr = serde_json::json!({"kind":"label","label":"deep","inner":expr});
    }
    oversized["expr"] = expr;
    let large = rules.len();
    data["rules"].as_array_mut().unwrap().push(oversized);
    let mut wide = data["rules"][ordinary].clone();
    wide["name"] = "Native.WideSyntax".into();
    wide["clauses"] = serde_json::json!([]);
    wide["expr"] = serde_json::json!({"kind":"sequence","items":vec![serde_json::json!({"kind":"literal","text":"x"});4096]});
    data["rules"].as_array_mut().unwrap().push(wide);
    let output = generate(&data.to_string()).unwrap();
    assert!(
        output.contains("pub static PARSER"),
        "no generated syntax table"
    );
    for index in [ordinary, add] {
        assert!(output.contains(&format!("fn nitride_syntax_{index}(")));
    }
    assert!(output.contains(&format!("fn nitride_tail_{add}(")));
    for index in [pratt, lexical, large, large + 1] {
        assert!(!output.contains(&format!("fn nitride_syntax_{index}(")));
    }
}

#[test]
fn fused_ascii_tables_keep_guards_fallback_and_ref_boundaries() {
    let mut data: serde_json::Value = serde_json::from_str(
        &nitride_prepare::prepare_project(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../languages/portable/project.json"
        )))
        .unwrap()
        .data
        .to_string(),
    )
    .unwrap();
    let base = data["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Portable.Let")
        .unwrap()
        .clone();
    let cls = serde_json::json!({"kind":"class","ranges":[[97,122]],"negated":false});
    let pure = serde_json::json!({"kind":"sequence","items":[{"kind":"predicate","positive":false,"inner":{"kind":"literal","text":"😀"}},cls]});
    let rules = data["rules"].as_array_mut().unwrap();
    let start = rules.len();
    for (name, expr) in [
        ("Fused.Pure", pure.clone()),
        (
            "Fused.Ref",
            serde_json::json!({"kind":"ref","name":"Fused.Pure"}),
        ),
        (
            "Fused.Suffix",
            serde_json::json!({"kind":"sequence","items":[{"kind":"literal","text":"a"},{"kind":"literal","text":"b"}]}),
        ),
        (
            "Fused.UntakenRef",
            serde_json::json!({"kind":"choice","items":[pure,{"kind":"ref","name":"Fused.Pure"}]}),
        ),
    ] {
        let mut rule = base.clone();
        rule["name"] = name.into();
        rule["lexical"] = true.into();
        rule["owner"] = serde_json::Value::Null;
        rule["clauses"] = serde_json::json!([]);
        rule["expr"] = expr;
        rules.push(rule);
    }
    let result = generate(&data.to_string()).unwrap();
    assert!(result.contains(&format!("static NITRIDE_ASCII_{start}:")));
    assert!(!result.contains(&format!("static NITRIDE_ASCII_{}:", start + 1)));
    assert!(!result.contains(&format!("static NITRIDE_ASCII_{}:", start + 2)));
    assert!(result.contains(&format!("static NITRIDE_ASCII_{}:", start + 3)));
    let function = &result[result.find(&format!("fn nitride_lex_{start}(")).unwrap()..];
    assert!(function.find("c.enter(d)?").unwrap() < function.find("c.ascii_decision(").unwrap());
    assert!(
        function.contains("AsciiDecision::new(4,2,4,true,true)")
            || result.contains("AsciiDecision::new(4,2,4,true,true)")
    );
    assert_eq!(generate(&data.to_string()).unwrap(), result);
}
