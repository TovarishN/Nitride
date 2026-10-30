#![cfg(not(feature = "components"))]

use std::path::Path;
use std::process::Command;

#[test]
fn native_only_reports_that_wasm_components_are_disabled() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/calendar");
    let error = nitride_cli::run_project(&root.join("Calendar.nproj"), "", &[]).unwrap_err();
    assert!(error.contains("built without component support"), "{error}");
    let output = Command::new(env!("CARGO_BIN_EXE_nitride"))
        .args(["binding", "draft"])
        .arg(root.join("Calendar.nproj"))
        .args(["Calendar.Gregorian", "missing.wasm"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("built without component support")
    );
}
