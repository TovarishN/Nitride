use std::process::Command;
#[test]
fn native_library_discovery_reports_exact_bundled_sources() {
    let binary = env!("CARGO_BIN_EXE_nitride");
    let list = Command::new(binary)
        .args(["library", "list", "--json"])
        .output()
        .unwrap();
    assert!(
        list.status.success(),
        "{}",
        String::from_utf8_lossy(&list.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(nitride_prepare::stdlib::manifest()).unwrap();
    assert_eq!(actual, expected);
    let show = Command::new(binary)
        .args(["library", "show", "Nitride.Lexical@1.0.0"])
        .output()
        .unwrap();
    assert!(show.status.success());
    assert_eq!(
        String::from_utf8(show.stdout).unwrap(),
        nitride_prepare::stdlib::find("Nitride.Lexical", "1.0.0")
            .unwrap()
            .sources[0]
            .text
    );
    let bad = Command::new(binary)
        .args(["library", "show", "Nitride.Lexical@9.0.0"])
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("1.0.0"));
}
