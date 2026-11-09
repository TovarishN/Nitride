use std::process::Command;
#[test]
fn native_templates_dry_run_create_and_execute() {
    let root = std::env::temp_dir().join(format!("nitride-cli-authoring-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let target = root.join("Calculator");
    let binary = env!("CARGO_BIN_EXE_nitride");
    let args = [
        "new",
        "language",
        "Calculator",
        "--directory",
        target.to_str().unwrap(),
        "--extension",
        ".calc",
    ];
    let dry = Command::new(binary)
        .args(args)
        .args(["--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    assert!(!target.exists());
    let plan: serde_json::Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(plan["files"].as_array().unwrap().len(), 6);
    let created = Command::new(binary).args(args).output().unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let run = Command::new(binary)
        .args(["run", "nitride.nproj", "sample.calc"])
        .current_dir(&target)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(result["values"], serde_json::json!([42.0]));
    assert_eq!(result["ok"], true);
    assert!(
        !Command::new(binary)
            .args(args)
            .output()
            .unwrap()
            .status
            .success()
    );
    std::fs::remove_dir_all(root).unwrap();
}
