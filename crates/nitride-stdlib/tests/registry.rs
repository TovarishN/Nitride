use nitride_stdlib::{find, libraries, source, valid_version};
use sha2::{Digest, Sha256};

#[test]
fn exact_offline_registry_and_canonical_readonly_uri() {
    let library = find("Nitride.Lexical", "1.0.0").unwrap();
    assert_eq!(libraries().len(), 1);
    assert_eq!(library.exports.len(), 9);
    let lexical = &library.sources[0];
    assert_eq!(
        lexical.uri,
        "nitride-lib:/Nitride.Lexical/1.0.0/Lexical.ngr"
    );
    assert!(lexical.text.contains("syntax module Nitride.Lexical"));
    assert_eq!(
        format!("{:x}", Sha256::digest(lexical.text)),
        lexical.sha256
    );
    assert_eq!(source(lexical.uri).unwrap().text, lexical.text);
    for uri in [
        "file:/Lexical.ngr",
        "nitride-lib:/Nitride.Lexical/../Lexical.ngr",
        "nitride-lib:/Nitride.Lexical/1.0.0/%4cexical.ngr",
        "nitride-lib:/Nitride.Lexical/1.0.0/Missing.ngr",
        "nitride-lib://Nitride.Lexical/1.0.0/Lexical.ngr",
    ] {
        assert!(source(uri).is_none(), "{uri}");
    }
    assert!(find("Nitride.Lexical", "1.0.1").is_none());
    assert!(find("Nitride.Lexical", "latest").is_none());
    assert!(find("Other", "1.0.0").is_none());
}

#[test]
fn pins_are_stable_canonical_versions_without_ranges_or_fallbacks() {
    for version in ["0.0.0", "1.0.0", "12.34.567"] {
        assert!(valid_version(version), "{version}");
    }
    for version in [
        "",
        "1",
        "1.0",
        "01.0.0",
        "1.00.0",
        "1.0.00",
        "1.0.0-beta",
        "^1.0.0",
        "1.0.0 ",
        "١.0.0",
        "1.0.0.0",
    ] {
        assert!(!valid_version(version), "{version}");
    }
}

#[test]
fn lexical_1_0_manifest_and_source_contract_is_frozen() {
    let manifest: serde_json::Value = serde_json::from_str(nitride_stdlib::manifest()).unwrap();
    let entry = &manifest["libraries"][0];
    assert_eq!(
        format!("{:x}", Sha256::digest(serde_json::to_vec(entry).unwrap())),
        "7f5de4b482f24559d93127cd2b904e29c2c06d019637f5ba9a5a264b0503ef82"
    );
}
