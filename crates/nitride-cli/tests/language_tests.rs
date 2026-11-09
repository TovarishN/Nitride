use std::io::Write;
use std::process::{Command, Stdio};
#[test]
fn native_test_command_lists_executes_and_streams() {
    let root = std::env::temp_dir().join(format!("nitride-cli-tests-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let plan =
        nitride_prepare::authoring::plan_language(&nitride_prepare::authoring::TemplateOptions {
            name: "Calculator".into(),
            extension: ".calc".into(),
            kind: nitride_prepare::authoring::TemplateKind::Typed,
        })
        .unwrap();
    let target = root.join("Calculator");
    nitride_prepare::authoring::create_language(&target, &plan).unwrap();
    let suite = target.join("tests.ntest");
    std::fs::write(&suite,"tests version 1 {project \"nitride.nproj\";test \"value\" { source \"show 42;\"; expect run values [42]; }}").unwrap();
    let binary = env!("CARGO_BIN_EXE_nitride");
    for flags in [vec!["--json"], vec!["--list", "--json"], vec!["--events"]] {
        let result = Command::new(binary)
            .arg("test")
            .arg(&suite)
            .args(&flags)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if flags == ["--events"] {
            let lines = String::from_utf8(result.stdout).unwrap();
            let events = lines
                .lines()
                .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(events.last().unwrap()["event"], "runFinished");
            assert_eq!(events[1]["result"]["status"], "passed");
        } else {
            let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(value["version"], 1);
        }
    }
    let result = Command::new(binary)
        .args(["test", "--case", "missing"])
        .arg(&suite)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let mut child = Command::new(binary)
        .args(["test", "--json", "--snapshot-stdin"])
        .arg(&suite)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let payload = serde_json::json!({"version":1,"files":[{"path":suite.canonicalize().unwrap(),"text":"tests version 1 {project \"nitride.nproj\";test \"fail\"{source \"show 42;\";expect run values [43];}}"}]});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(1));
    std::fs::remove_dir_all(root).unwrap();
}
