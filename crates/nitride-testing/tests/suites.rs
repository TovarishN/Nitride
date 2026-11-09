use nitride_testing::{Input, Phase, parse_suite};
#[test]
fn native_suite_preserves_program_bytes_and_case_spans() {
    let text = "tests version 1 { project \"nitride.nproj\"; test \"raw\" { source <[\nlet x = \"é\";\r\n]>; expect run values [42, true, \"text\", [1, 2]]; } test \"bad\" { expect compile reject { diagnostic \"bind\" code none span [0,2]; } source \"é\"; } }";
    let suite = parse_suite(text).unwrap();
    assert_eq!(suite.project, "nitride.nproj");
    assert_eq!(suite.tests.len(), 2);
    assert_eq!(suite.tests[0].expect.phase, Phase::Run);
    match &suite.tests[0].input {
        Input::Inline {
            text,
            raw_span: Some(_),
        } => assert_eq!(text, "\nlet x = \"é\";\r\n"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        &text[suite.tests[0].name_span[0]..suite.tests[0].name_span[1]],
        "\"raw\""
    );
    assert_eq!(
        suite.tests[1].expect.diagnostics.as_ref().unwrap()[0].code,
        Some(None)
    );
}
#[test]
fn rejects_invalid_native_declarations_and_assertions() {
    for text in [
        "{}",
        "tests version 2 { project \"x\"; test \"a\" {source \"x\"; expect parse accept;} }",
        "tests version 1 { project \"x\"; }",
        "tests version 1 { project \"x\"; project \"y\"; test \"a\" {source \"x\";expect parse accept;} }",
    ] {
        assert!(parse_suite(text).is_err(), "{text}");
    }
    for body in [
        "source \"x\";file \"x\";expect parse accept;",
        "source \"x\";expect run accept;",
        "source \"x\";expect parse reject {}",
        "source \"é\";expect parse reject {diagnostic \"parse\" span [0,1];}",
        "source \"x\";expect run values [01];",
        "source \"x\";expect run values [null];",
    ] {
        let text = format!("tests version 1 {{project \"x\";test \"a\" {{{body}}}}}");
        assert!(parse_suite(&text).is_err(), "{text}");
    }
}
