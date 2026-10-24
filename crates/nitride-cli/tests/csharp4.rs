use serde_json::Value;

/// Nitride's CSharp4 trees equal actual Nitrogen's on valid C#; both reject the same inputs.
#[test]
fn csharp4_syntax_matches_real_nitrogen() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/csharp4-reference.json")).unwrap();
    let mut mismatches = Vec::new();
    for case in cases.as_array().unwrap() {
        let actual = nitride_cli::syntax_observe(
            &nitride_cli::csharp4::LANGUAGE,
            case["source"].as_str().unwrap(),
        );
        let expected = &case["observation"];
        let same = if expected["ok"] == true && actual["ok"] == true {
            actual["nodes"] == expected["nodes"]
        } else {
            actual["ok"] == expected["ok"]
        };
        if !same {
            mismatches.push(case["name"].as_str().unwrap().to_owned());
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches: {mismatches:?}",
        mismatches.len()
    );
}
