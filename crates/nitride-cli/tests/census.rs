//! `nitride-profile --census` reports parser decisions for one strict parse.
#![cfg(feature = "census")]
use std::path::Path;
use std::process::Command;

fn census(native: bool, path: &Path) -> serde_json::Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nitride-profile"));
    if native {
        command.arg("--native-syntax");
    }
    let output = command
        .args(["--census", "csharp4"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn sample() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/csharp4/corpus/Mono2004/Boolean.cs")
}

#[test]
fn census_reports_csharp4_decisions() {
    let report = census(false, &sample());
    assert_eq!(report["diagnostics"], 0);
    let decisions = report["decisions"].as_array().unwrap();
    assert!(!decisions.is_empty());
    for decision in decisions {
        let count = |key: &str| decision[key].as_u64().unwrap();
        assert!(count("single") <= count("entries"), "{decision}");
        assert!(count("attempts") >= count("single"), "{decision}");
        assert!(count("failed_attempts") <= count("attempts"), "{decision}");
    }
    assert!(
        decisions
            .iter()
            .any(|d| d["rule_name"].as_str().unwrap().contains("Statement"))
    );
}

/// Generated and interpreted parses reach the same decision points, keyed
/// identically. Counts may differ slightly: the two paths hit memoized
/// results at different points.
#[test]
fn census_matches_between_generated_and_interpreted() {
    let interpreted = census(false, &sample());
    let generated = census(true, &sample());
    let close = |a: u64, b: u64| a.abs_diff(b) <= 3.max(a.max(b) / 20);
    let keyed = |report: &serde_json::Value| {
        report["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                (
                    (
                        d["rule"].as_u64().unwrap(),
                        d["kind"].as_str().unwrap().to_owned(),
                        d["ordinal"].as_u64().unwrap(),
                    ),
                    d["entries"].as_u64().unwrap(),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let (a, b) = (keyed(&interpreted), keyed(&generated));
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    for (key, entries) in &a {
        assert!(close(*entries, b[key]), "{key:?}: {entries} vs {}", b[key]);
    }
    for key in [
        "predicate_evaluations",
        "context_entries",
        "repeat_exits",
        "repeat_consumed_exits",
    ] {
        let (x, y) = (
            interpreted[key].as_u64().unwrap(),
            generated[key].as_u64().unwrap(),
        );
        assert!(close(x, y), "{key}: {x} vs {y}");
    }
}
