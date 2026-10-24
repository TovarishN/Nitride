use serde_json::Value;
use std::path::PathBuf;

#[test]
fn prepare_command_exports_validated_json() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_nitride"))
        .args([
            "prepare",
            root.join("languages/power/project.json").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let data: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(data["start"], "Portable.Program");
    assert!(
        data["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"] == "Portable.Power.Pow")
    );
}

#[test]
fn invalid_preparation_keeps_previous_output_and_reports_source() {
    let root = std::env::temp_dir().join(format!("nitride-cli-prepare-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("project.json"),
        r#"{"mode":"syntax","start":"Bad.Start","grammars":["Bad.ngr"],"catalog":"catalog.json"}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("catalog.json"),
        r#"{"types":[],"operations":[],"statements":{}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("Bad.ngr"),
        "// café 😀\nsyntax module Bad { syntax Start = Missing; }",
    )
    .unwrap();
    std::fs::write(root.join("language.json"), "keep this").unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_nitride"))
        .arg("prepare")
        .arg(root.join("project.json"))
        .arg("--output")
        .arg(root.join("language.json"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    let message = String::from_utf8_lossy(&result.stderr);
    assert!(message.contains("Bad.ngr:2:"), "{message}");
    assert!(message.contains("Missing"), "{message}");
    assert_eq!(
        std::fs::read_to_string(root.join("language.json")).unwrap(),
        "keep this"
    );
    std::fs::remove_dir_all(root).unwrap();
}
