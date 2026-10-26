use serde_json::Value;
#[test]
fn profiler_declares_whether_allocator_metrics_are_instrumented() {
    let path =
        std::env::temp_dir().join(format!("nitride-profiler-mode-{}.txt", std::process::id()));
    std::fs::write(&path, "1 + 2;").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nitride-profile"))
        .args(["portable", path.to_str().unwrap()])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let instrumented = !cfg!(feature = "uninstrumented-profiler");
    assert_eq!(report["parse"]["allocation_instrumented"], instrumented);
    assert!(report["parse"]["elapsed_ns"].as_u64().unwrap() > 0);
    if instrumented {
        assert!(report["parse"]["peak_bytes"].as_u64().unwrap() > 0);
    } else {
        assert!(report["parse"]["peak_bytes"].is_null());
        assert!(report["storage"]["physical_retained_bytes"].is_null());
    }
}

#[test]
fn profiler_native_lexers_are_explicit_and_counted() {
    let path = std::env::temp_dir().join(format!(
        "nitride-native-profiler-{}.txt",
        std::process::id()
    ));
    std::fs::write(&path, "class C { int F() { return 123; } }").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nitride-profile"))
        .args([
            "--native-lexers",
            "--snapshot",
            "csharp4",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["compiled_lexers"], true);
    assert!(report["stats"]["native_lexical_calls"].as_u64().unwrap() > 0);
}

#[test]
fn profiler_generated_syntax_is_explicit_and_counted() {
    let path = std::env::temp_dir().join(format!(
        "nitride-syntax-profiler-{}.txt",
        std::process::id()
    ));
    std::fs::write(&path, "class C { int F(){return 1+2*3;} }").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nitride-profile"))
        .args([
            "--native-syntax",
            "--snapshot",
            "csharp4",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let r: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(r["compiled_syntax"], true);
    assert_eq!(r["compiled_lexers"], true);
    for field in [
        "native_syntax_calls",
        "native_syntax_tails",
        "native_lexical_calls",
    ] {
        assert!(r["stats"][field].as_u64().unwrap() > 0, "{field}");
    }
}
