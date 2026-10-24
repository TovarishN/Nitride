use serde_json::Value;

fn profile(mode: &str, source: &str) -> Value {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "nitride-profile-{}-{mode}-{sequence}.txt",
        std::process::id()
    ));
    std::fs::write(&path, source).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_nitride-profile"))
        .args([mode, path.to_str().unwrap()])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

#[test]
fn engine_phases_are_measured_before_observation_formatting() {
    let report = profile("power", "let x : Scalar = 2; x ^ 3;");
    assert_eq!(report["values"], serde_json::json!([8.0]));
    for phase in ["parse", "hir", "execution"] {
        let record = &report[phase];
        assert!(record["allocation_count"].as_u64().unwrap() > 0);
        assert!(
            record["peak_bytes"].as_u64().unwrap() >= record["retained_bytes"].as_u64().unwrap()
        );
    }
    assert_eq!(
        report["parse"]["retained_bytes"],
        report["syntax_capacity_bytes"]
    );
}

#[test]
fn syntax_only_profile_never_reports_semantic_execution() {
    let report = profile("xml", "<root><item/></root>");
    assert_eq!(report["semantic_validation"], false);
    assert!(report["hir"].is_null());
    assert!(report["execution"].is_null());
    assert!(report["parse"]["peak_bytes"].as_u64().unwrap() > 0);
}

#[test]
fn repeated_statements_reuse_parser_scratch_allocations() {
    let report = profile("portable", &"1 + 2 * 3;\n".repeat(1000));
    assert_eq!(report["values"].as_array().unwrap().len(), 1000);
    let allocations = report["parse"]["allocation_count"].as_u64().unwrap();
    assert!(allocations < 1000, "parse allocations: {allocations}");
    assert_eq!(
        report["parse"]["retained_bytes"],
        report["syntax_capacity_bytes"]
    );
}

#[test]
fn repeated_xml_elements_reuse_parser_scratch_allocations() {
    let source = format!(
        "<root>{}</root>",
        "<item key=\"a\">value</item>".repeat(1000)
    );
    let report = profile("xml", &source);
    assert_eq!(report["semantic_validation"], false);
    let allocations = report["parse"]["allocation_count"].as_u64().unwrap();
    assert!(allocations < 1000, "parse allocations: {allocations}");
    assert_eq!(
        report["parse"]["retained_bytes"],
        report["syntax_capacity_bytes"]
    );
}
