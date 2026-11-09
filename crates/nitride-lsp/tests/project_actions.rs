mod support;
use serde_json::{Value, json};
use support::*;
fn capabilities() -> Value {
    json!({"workspace":{"workspaceEdit":{"documentChanges":true,"resourceOperations":["create"]}},"textDocument":{"codeAction":{"resolveSupport":{"properties":["edit"]}}}})
}
fn session(root: &std::path::Path, messages: Vec<Value>) -> Vec<Value> {
    let mut init = initialize(1, root);
    init["params"]["capabilities"] = capabilities();
    let mut input = frame(&init);
    for message in messages {
        input.extend(frame(&message));
    }
    input.extend(frame(&request(9999, "shutdown", Value::Null)));
    input.extend(frame(&notification("exit", Value::Null)));
    Session::raw(input)
}
fn ask(path: &std::path::Path) -> Value {
    request(
        5,
        "textDocument/codeAction",
        json!({"textDocument":{"uri":uri(path)},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":200}},"context":{"diagnostics":[]}}),
    )
}
#[test]
fn missing_referenced_grammar_offers_create_only_scaffold() {
    let root = scratch("project-create-action");
    let path = root.join("nitride.nproj");
    let text = "language Test { grammar Test.ngr; start Test.Program; }";
    std::fs::write(&path, text).unwrap();
    let messages = session(&root, vec![open(&path, text), ask(&path)]);
    let actions = response(&messages, 5)["result"].as_array().unwrap();
    let action = actions
        .iter()
        .find(|a| a["title"].as_str().unwrap_or_default().contains("Test.ngr"))
        .unwrap();
    let resolved = session(
        &root,
        vec![
            open(&path, text),
            request(6, "codeAction/resolve", action.clone()),
        ],
    );
    let edit = &response(&resolved, 6)["result"]["edit"]["documentChanges"];
    assert_eq!(edit[0]["kind"], "create");
    assert_eq!(
        edit[0]["options"],
        json!({"overwrite":false,"ignoreIfExists":false})
    );
    assert!(
        edit[1]["edits"][0]["newText"]
            .as_str()
            .unwrap()
            .contains("syntax Program")
    );
    std::fs::write(root.join("Test.ngr"), "user content").unwrap();
    let stale = session(
        &root,
        vec![
            open(&path, text),
            request(6, "codeAction/resolve", action.clone()),
        ],
    );
    assert!(response(&stale, 6)["result"].get("edit").is_none());
}

#[test]
fn pin_action_is_explicit_versioned_and_revalidated() {
    let root = scratch("project-pin-action");
    let project = root.join("nitride.nproj");
    let grammar = root.join("Test.ngr");
    let manifest = "// { comment }\nlanguage Test { grammar Test.ngr; start Test.Program; }";
    let text = "syntax module Test { syntax Program = Nitride.Lexical.UnsignedInteger; }";
    std::fs::write(&project, manifest).unwrap();
    std::fs::write(&grammar, text).unwrap();
    let messages = session(
        &root,
        vec![
            open(&project, manifest),
            open(&grammar, text),
            ask(&grammar),
        ],
    );
    let action = response(&messages, 5)["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["data"]["code"] == "NLIB0004")
        .unwrap()
        .clone();
    let messages = session(
        &root,
        vec![
            open(&project, manifest),
            open(&grammar, text),
            request(6, "codeAction/resolve", action.clone()),
        ],
    );
    let edit = &response(&messages, 6)["result"]["edit"]["documentChanges"][0];
    assert_eq!(edit["textDocument"]["version"], 1);
    assert_eq!(edit["textDocument"]["uri"], uri(&project));
    let messages = session(
        &root,
        vec![
            open(&project, manifest),
            open(&grammar, text),
            change(&project, 2, &format!("{manifest}\n// newer")),
            request(6, "codeAction/resolve", action),
        ],
    );
    assert!(response(&messages, 6)["result"].get("edit").is_none());
}
#[test]
fn project_actions_require_capabilities_range_and_unopened_absent_targets() {
    let root = scratch("project-actions-safety");
    let project = root.join("nitride.nproj");
    let text = "language Test { grammar Test.ngr; start Test.Program; catalog Test.ncatalog; }";
    std::fs::write(&project, text).unwrap();
    let messages = Session::run(&root, vec![open(&project, text), ask(&project)]);
    assert_eq!(response(&messages, 5)["result"], json!([]));
    let mut unrelated = ask(&project);
    unrelated["params"]["context"]["only"] = json!(["source.fixAll"]);
    let messages = session(
        &root,
        vec![
            open(&project, text),
            open(&root.join("Test.ngr"), "unsaved user grammar"),
            unrelated,
        ],
    );
    assert_eq!(response(&messages, 5)["result"], json!([]));
    let messages = session(
        &root,
        vec![
            open(&project, text),
            open(&root.join("Test.ngr"), "unsaved user grammar"),
            ask(&project),
        ],
    );
    let actions = response(&messages, 5)["result"].as_array().unwrap();
    assert!(
        actions
            .iter()
            .all(|a| !a["title"].as_str().unwrap().contains("Test.ngr"))
    );
    assert!(
        actions
            .iter()
            .any(|a| a["title"].as_str().unwrap().contains("Test.ncatalog"))
    );
    let mut unrelated = ask(&project);
    unrelated["params"]["range"] =
        json!({"start":{"line":0,"character":0},"end":{"line":0,"character":2}});
    let messages = session(&root, vec![open(&project, text), unrelated]);
    assert_eq!(response(&messages, 5)["result"], json!([]));
}
#[cfg(unix)]
#[test]
fn project_actions_reject_symlink_parents_and_outside_targets() {
    let root = scratch("project-action-symlink");
    std::os::unix::fs::symlink(root.parent().unwrap(), root.join("escape")).unwrap();
    let project = root.join("nitride.nproj");
    let text = "language Test { grammar escape/Evil.ngr, ../Outside.ngr; start Test.Program; }";
    std::fs::write(&project, text).unwrap();
    let messages = session(&root, vec![open(&project, text), ask(&project)]);
    assert_eq!(response(&messages, 5)["result"], json!([]));
}

#[test]
fn closed_project_pin_resolves_only_after_unchanged_buffer_opens() {
    let root = scratch("project-pin-open");
    let project = root.join("nitride.nproj");
    let grammar = root.join("Test.ngr");
    let manifest = "language Test { grammar Test.ngr; start Test.Program; }";
    let text = "syntax module Test { syntax Program = Nitride.Lexical.UnsignedInteger; }";
    std::fs::write(&project, manifest).unwrap();
    std::fs::write(&grammar, text).unwrap();
    let messages = session(&root, vec![open(&grammar, text), ask(&grammar)]);
    let action = response(&messages, 5)["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["data"]["code"] == "NLIB0004")
        .unwrap()
        .clone();
    assert!(action["data"]["projectVersion"].is_null());
    let messages = session(
        &root,
        vec![
            open(&grammar, text),
            request(6, "codeAction/resolve", action.clone()),
        ],
    );
    assert!(response(&messages, 6)["result"].get("edit").is_none());
    let messages = session(
        &root,
        vec![
            open(&grammar, text),
            open(&project, manifest),
            request(6, "codeAction/resolve", action),
        ],
    );
    assert_eq!(
        response(&messages, 6)["result"]["edit"]["documentChanges"][0]["textDocument"]["version"],
        1
    );
}

#[cfg(unix)]
#[test]
fn workspace_root_alias_is_allowed_but_child_symlinks_are_not() {
    let root = scratch("project-root-alias-real");
    let alias = root.with_file_name(format!("nitride-project-alias-{}", std::process::id()));
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let path = alias.join("nitride.nproj");
    let text = "language Test { grammar Test.ngr; start Test.Program; }";
    std::fs::write(&path, text).unwrap();
    let messages = session(&alias, vec![open(&path, text), ask(&path)]);
    assert!(
        response(&messages, 5)["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["title"] == "Create Test.ngr")
    );
    std::fs::remove_file(alias).unwrap();
}

#[test]
fn resolved_local_lexical_module_never_gets_a_conflicting_bundled_pin() {
    let root = scratch("local-lexical-module");
    let project = root.join("nitride.nproj");
    let grammar = root.join("Test.ngr");
    let manifest = "language Test { grammar Test.ngr; start Test.Program; }";
    let text = "syntax module Nitride.Lexical { token UnsignedInteger = \"x\"; } syntax module Test { syntax Program = Nitride.Lexical.UnsignedInteger; }";
    std::fs::write(&project, manifest).unwrap();
    std::fs::write(&grammar, text).unwrap();
    nitride_prepare::prepare_project(&project).unwrap();
    let messages = session(
        &root,
        vec![
            open(&project, manifest),
            open(&grammar, text),
            ask(&grammar),
        ],
    );
    assert!(
        response(&messages, 5)["result"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["data"]["code"] != "NLIB0004")
    );
}

#[test]
fn retained_registry_owner_cannot_pin_a_former_grammar() {
    let root = scratch("pin-stale-owner");
    let project = root.join("nitride.nproj");
    let grammar = root.join("Test.ngr");
    let manifest = "language Test { grammar Test.ngr; start Test.Program; }";
    let changed = manifest.replace("Test.ngr", "Other.ngr");
    let text = "syntax module Test { syntax Program = Nitride.Lexical.UnsignedInteger; }";
    std::fs::write(&project, manifest).unwrap();
    std::fs::write(&grammar, text).unwrap();
    let messages = session(
        &root,
        vec![
            open(&project, manifest),
            open(&grammar, text),
            change(&project, 2, &changed),
            ask(&grammar),
        ],
    );
    assert!(
        response(&messages, 5)["result"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["data"]["code"] != "NLIB0004")
    );
}
fn resolved_missing_file(
    root: &std::path::Path,
    project: &std::path::Path,
    text: &str,
    name: &str,
) -> String {
    let messages = session(root, vec![open(project, text), ask(project)]);
    let action = response(&messages, 5)["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"] == format!("Create {name}"))
        .unwrap()
        .clone();
    let messages = session(
        root,
        vec![
            open(project, text),
            request(6, "codeAction/resolve", action),
        ],
    );
    response(&messages, 6)["result"]["edit"]["documentChanges"][1]["edits"][0]["newText"]
        .as_str()
        .unwrap()
        .to_owned()
}
#[test]
fn missing_start_scaffold_is_admitted_by_project_preparation() {
    let root = scratch("missing-start-admitted");
    let project = root.join("nitride.nproj");
    let text = "language Test { grammar Missing.ngr; start Test.Program; }";
    std::fs::write(&project, text).unwrap();
    let contents = resolved_missing_file(&root, &project, text, "Missing.ngr");
    std::fs::write(root.join("Missing.ngr"), contents).unwrap();
    nitride_prepare::prepare_project(&project).unwrap();
}
#[test]
fn missing_start_never_creates_a_duplicate_existing_module() {
    let root = scratch("missing-start-existing-module");
    let project = root.join("nitride.nproj");
    let text = "language Test { grammar Existing.ngr, Test.ngr; start Test.Program; }";
    std::fs::write(&project, text).unwrap();
    std::fs::write(
        root.join("Existing.ngr"),
        "syntax module Test { syntax Other = \"x\"; }",
    )
    .unwrap();
    let contents = resolved_missing_file(&root, &project, text, "Test.ngr");
    assert!(!contents.contains("syntax module Test\n"), "{contents}");
    std::fs::write(root.join("Test.ngr"), contents).unwrap();
    let error = nitride_prepare::prepare_project(&project).unwrap_err();
    assert!(!error.message.contains("duplicate module"), "{error}");
    assert!(error.message.contains("start rule"), "{error}");
}
