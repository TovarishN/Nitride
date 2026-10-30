#[test]
fn production_training_uses_generated_parser_and_keeps_edit_diagnostics() {
    let source = "class C { string s = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"; }";
    let result = nitride_cli::pgo::observe("csharp4", source).unwrap();
    assert!(result["native_calls"].as_u64().unwrap() > 0);
    assert_eq!(result["original"]["source"], source);
    assert_eq!(result["edited"]["source"], format!("{source} "));
    assert_eq!(result["original"]["diagnostics"], "[]");
    let broken = nitride_cli::pgo::observe("csharp4", "class C { int x }").unwrap();
    assert_ne!(broken["original"]["diagnostics"], "[]");
    assert!(nitride_cli::pgo::observe("missing", source).is_err());
}
