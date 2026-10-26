#![cfg(not(feature = "uninstrumented-profiler"))]
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

#[test]
fn profile_separates_engine_and_export_storage() {
    let report = profile("xml", "<root><item/></root>");
    assert_eq!(report["storage"]["representation"], "legacy-flat");
    assert_eq!(
        report["storage"]["engine_retained_bytes"],
        report["syntax_capacity_bytes"]
    );
    assert!(report["storage"]["export_retained_bytes"].is_null());
    assert_eq!(
        report["storage"]["physical_retained_bytes"],
        report["parse"]["retained_bytes"]
    );
}

#[test]
fn snapshot_profile_keeps_export_out_of_parse_measurement() {
    let path = std::env::temp_dir().join(format!("nitride-snapshot-{}.cs", std::process::id()));
    std::fs::write(&path, "class C { int A(){return 123;} }").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nitride-profile"))
        .args(["--snapshot", "csharp4", path.to_str().unwrap()])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["storage"]["representation"], "shared");
    assert_eq!(report["stats"]["export_nodes_copied"], 0);
    assert_eq!(
        report["storage"]["physical_retained_bytes"],
        report["parse"]["retained_bytes"]
    );
    assert!(report["parse"]["retained_bytes"].as_u64().unwrap() > 0);
}

#[test]
fn recovered_snapshot_storage_matches_allocator() {
    let path = std::env::temp_dir().join(format!(
        "nitride-snapshot-recover-{}.cs",
        std::process::id()
    ));
    std::fs::write(&path, "class C { int A(){ return 1 } }").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nitride-profile"))
        .args(["--snapshot-recover", "csharp4", path.to_str().unwrap()])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["storage"]["physical_retained_bytes"],
        report["parse"]["retained_bytes"]
    );
    assert_eq!(report["stats"]["full_parse_fallbacks"], 0);
    assert_eq!(report["stats"]["memo_descendants_copied"], 0);
}
#[test]
fn profiler_edits_measure_reuse_and_shared_reader_storage() {
    let prefix = std::env::temp_dir().join(format!("nitride-edits-{}", std::process::id()));
    let source = prefix.with_extension("cs");
    let edits = prefix.with_extension("json");
    let text = "class C { int A(){ return 111; } int B(){ return 222; } }";
    let at = text.find("111").unwrap();
    std::fs::write(&source, text).unwrap();
    std::fs::write(
        &edits,
        serde_json::json!({"retain":8,"edits":[{"range":[at,at+3],"replacement":"1111"}]})
            .to_string(),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nitride-profile"))
        .args([
            "--edits",
            "csharp4",
            source.to_str().unwrap(),
            edits.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(edits).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &report["edits"][0];
    assert!(
        first["snapshot"]["stats"]["reused_subtrees"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(first["snapshot"]["stats"]["source_bytes_copied"], 4);
    assert!(
        first["shared_reader_bytes"].as_u64().unwrap()
            < first["logical_reader_bytes"].as_u64().unwrap()
    );
}

#[test]
fn dropping_all_snapshot_readers_releases_all_dynamic_storage() {
    let source = std::env::temp_dir().join(format!("nitride-drop-{}.xml", std::process::id()));
    std::fs::write(&source, "<root><item/><item/></root>").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nitride-profile"))
        .args(["--snapshot-drop", "xml", source.to_str().unwrap()])
        .output()
        .unwrap();
    std::fs::remove_file(source).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["lifecycle"]["net_retained_bytes"], 0);
    assert!(result["lifecycle"]["peak_bytes"].as_u64().unwrap() > 0);
}
