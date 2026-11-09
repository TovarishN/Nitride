mod support;
use serde_json::json;
use support::*;
#[test]
fn editor_creation_uses_shared_recipe_and_bounds_targets() {
    let root = scratch("authoring-command");
    let target = root.join("Calculator");
    let command = |path: &std::path::Path| json!({"command":"nitride.createLanguage","arguments":[{"directory":uri(path),"name":"Calculator","extension":".calc","kind":"typed"}]});
    let messages = Session::run(
        &root,
        vec![
            request(5, "workspace/executeCommand", command(&target)),
            request(6, "workspace/executeCommand", command(&target)),
            request(
                7,
                "workspace/executeCommand",
                command(&root.parent().unwrap().join("outside")),
            ),
        ],
    );
    assert!(
        response(&messages, 5).get("error").is_none(),
        "{}",
        response(&messages, 5)
    );
    assert!(response(&messages, 6).get("error").is_some());
    assert!(response(&messages, 7).get("error").is_some());
    let prepared = nitride_prepare::prepare_project(&target.join("nitride.nproj")).unwrap();
    assert_eq!(prepared.libraries.len(), 1);
}
