mod support;
use serde_json::{Value, json};
use support::*;
const LIB: &str = "nitride-lib:/Nitride.Lexical/1.0.0/Lexical.ngr";
const GRAMMAR: &str =
    "syntax module Test { token Name = Nitride.Lexical.AsciiIdentifier; syntax Start = Name; }";
fn setup(name: &str) -> std::path::PathBuf {
    let root = scratch(name);
    std::fs::create_dir(root.join("lang")).unwrap();
    std::fs::write(root.join("nitride.nproj"), "workspace { include lang; }").unwrap();
    std::fs::write(root.join("lang/Test.nproj"),"language Test { extensions .test; library Nitride.Lexical version 1.0.0; grammar Test.ngr; start Test.Start; }").unwrap();
    std::fs::write(root.join("lang/Test.ngr"), GRAMMAR).unwrap();
    root
}
fn run(root: &std::path::Path, capable: bool, extra: Vec<Value>) -> Vec<Value> {
    let mut init = initialize(1, root);
    init["params"]["initializationOptions"] = json!({"libraryDocuments":capable});
    let mut input = frame(&init);
    for m in extra {
        input.extend(frame(&m));
    }
    input.extend(frame(&request(9999, "shutdown", Value::Null)));
    input.extend(frame(&notification("exit", Value::Null)));
    Session::raw(input)
}
#[test]
fn pinned_library_prepares_and_navigates_without_becoming_a_path() {
    let root = setup("library-navigation");
    let grammar = root.join("lang/Test.ngr");
    let sample = root.join("ok.test");
    let position = json!({"line":0,"character":GRAMMAR.find("AsciiIdentifier").unwrap()+2});
    for capable in [false, true] {
        let messages = run(
            &root,
            capable,
            vec![
                open(&grammar, GRAMMAR),
                open(&sample, "hello"),
                request(
                    5,
                    "textDocument/definition",
                    json!({"textDocument":{"uri":uri(&grammar)},"position":position}),
                ),
                request(
                    6,
                    "textDocument/rename",
                    json!({"textDocument":{"uri":uri(&grammar)},"position":position,"newName":"Changed"}),
                ),
            ],
        );
        assert_eq!(diagnostics(&messages, &sample), json!([]));
        if capable {
            assert_eq!(response(&messages, 5)["result"]["uri"], LIB);
        } else {
            assert!(response(&messages, 5)["result"].is_null());
        }
        assert!(
            response(&messages, 6).get("error").is_some(),
            "{}",
            response(&messages, 6)
        );
    }
}
#[test]
fn library_sources_are_canonical_and_read_only() {
    let root = setup("library-readonly");
    let canonical = nitride_prepare::stdlib::source(LIB).unwrap().text;
    let messages = run(
        &root,
        true,
        vec![
            notification(
                "textDocument/didOpen",
                json!({"textDocument":{"uri":LIB,"version":1,"text":"BROKEN"}}),
            ),
            notification(
                "textDocument/didChange",
                json!({"textDocument":{"uri":LIB,"version":2},"contentChanges":[{"text":"MORE BROKEN"}]}),
            ),
            request(5, "nitride/librarySource", json!({"uri":LIB})),
            request(
                6,
                "nitride/librarySource",
                json!({"uri":"nitride-lib:/Nitride.Lexical/1.0.0/../Lexical.ngr"}),
            ),
            request(
                7,
                "textDocument/formatting",
                json!({"textDocument":{"uri":LIB}}),
            ),
            request(
                8,
                "textDocument/documentSymbol",
                json!({"textDocument":{"uri":LIB}}),
            ),
        ],
    );
    assert_eq!(response(&messages, 5)["result"], canonical);
    assert!(response(&messages, 6).get("error").is_some());
    assert_eq!(response(&messages, 7)["result"], json!([]));
    assert!(
        !response(&messages, 8)["result"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_pin_is_visible_and_retains_last_good_reader() {
    use nitride_lsp::registry::Registry;
    let root = setup("library-bad-pin");
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&root), &|_| None);
    let old = registry.languages()[0].current.clone().unwrap();
    let project = root.join("lang/Test.nproj");
    let text = std::fs::read_to_string(&project)
        .unwrap()
        .replace("1.0.0", "9.0.0");
    registry.reload(std::slice::from_ref(&root), &|p| {
        if p == project {
            Some(text.clone())
        } else {
            None
        }
    });
    let state = &registry.languages()[0];
    assert_eq!(state.errors[0].code.as_deref(), Some("NLIB0002"));
    assert!(std::sync::Arc::ptr_eq(
        &old,
        state.current.as_ref().unwrap()
    ));
    assert_eq!(
        &text[state.errors[0].span[0]..state.errors[0].span[1]],
        "9.0.0"
    );
}
#[test]
fn ambiguous_owners_do_not_choose_a_library_context() {
    use nitride_lsp::registry::Registry;
    let root = setup("library-ambiguous");
    std::fs::create_dir(root.join("other")).unwrap();
    std::fs::write(
        root.join("nitride.nproj"),
        "workspace { include lang; include other; }",
    )
    .unwrap();
    std::fs::write(
        root.join("other/Other.nproj"),
        "language Other { extensions .other; grammar ../lang/Test.ngr; start Test.Start; }",
    )
    .unwrap();
    let mut registry = Registry::default();
    registry.reload(std::slice::from_ref(&root), &|_| None);
    assert!(
        registry
            .libraries_at(&root.join("lang/Test.ngr"))
            .is_empty()
    );
}

#[test]
fn adding_standalone_workspace_folder_supplies_library_navigation() {
    let root = scratch("standalone-library-folder");
    std::fs::write(root.join("nitride.nproj"), "workspace { }").unwrap();
    let target = root.join("Calculator");
    let plan =
        nitride_prepare::authoring::plan_language(&nitride_prepare::authoring::TemplateOptions {
            name: "Calculator".into(),
            extension: ".calc".into(),
            kind: nitride_prepare::authoring::TemplateKind::Typed,
        })
        .unwrap();
    nitride_prepare::authoring::create_language(&target, &plan).unwrap();
    let grammar = target.join("Calculator.ngr");
    let text = std::fs::read_to_string(&grammar).unwrap();
    let before = &text[..text.find("AsciiIdentifier").unwrap()];
    let line = before.bytes().filter(|b| *b == b'\n').count();
    let character = before.rsplit('\n').next().unwrap().len() + 2;
    let messages = run(
        &root,
        true,
        vec![
            notification(
                "workspace/didChangeWorkspaceFolders",
                json!({"event":{"added":[{"uri":uri(&target),"name":"Calculator"}],"removed":[]}}),
            ),
            open(&grammar, &text),
            request(
                5,
                "textDocument/definition",
                json!({"textDocument":{"uri":uri(&grammar)},"position":{"line":line,"character":character}}),
            ),
        ],
    );
    assert_eq!(
        response(&messages, 5)["result"]["uri"],
        LIB,
        "{}",
        response(&messages, 5)
    );
}

#[test]
fn unlisted_standalone_project_supplies_its_own_library_context() {
    let root = scratch("unlisted-library-project");
    std::fs::write(root.join("nitride.nproj"), "workspace { }").unwrap();
    let target = root.join("Nested");
    let plan =
        nitride_prepare::authoring::plan_language(&nitride_prepare::authoring::TemplateOptions {
            name: "Nested".into(),
            extension: ".nested".into(),
            kind: nitride_prepare::authoring::TemplateKind::Syntax,
        })
        .unwrap();
    nitride_prepare::authoring::create_language(&target, &plan).unwrap();
    let grammar = target.join("Nested.ngr");
    let text = std::fs::read_to_string(&grammar).unwrap();
    let messages = run(
        &root,
        true,
        vec![
            open(&grammar, &text),
            request(
                5,
                "textDocument/definition",
                json!({"textDocument":{"uri":uri(&grammar)},"position":{"line":2,"character":50}}),
            ),
        ],
    );
    assert_eq!(diagnostics(&messages, &grammar), json!([]));
    assert_eq!(response(&messages, 5)["result"]["uri"], LIB);
}
