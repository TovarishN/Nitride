use sha2::{Digest, Sha256};
use std::{collections::HashSet, fmt::Write, path::PathBuf};
#[path = "src/version.rs"]
mod version;

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let manifest = root.join("libraries/manifest.json");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed=src/version.rs");
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    let mut generated = String::from("pub static LIBRARIES: &[Library] = &[\n");
    let mut identities = HashSet::new();
    for library in data["libraries"].as_array().expect("libraries array") {
        let name = library["name"].as_str().expect("name");
        let pin = library["version"].as_str().expect("version");
        assert!(name.split('.').all(|part| {
            !part.is_empty()
                && part.bytes().enumerate().all(|(i, b)| {
                    b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit())
                })
        }));
        assert!(version::valid_version(pin), "noncanonical library version");
        assert!(identities.insert((name, pin)), "duplicate library identity");
        write!(
            generated,
            "Library {{ name: {name:?}, version: {pin:?}, exports: &["
        )
        .unwrap();
        let mut exports = HashSet::new();
        for export in library["exports"].as_array().expect("exports") {
            let export = export.as_str().expect("export name");
            assert!(exports.insert(export), "duplicate export");
            write!(generated, "{export:?},").unwrap();
        }
        generated.push_str("], sources: &[\n");
        let mut members = HashSet::new();
        for source in library["sources"].as_array().expect("sources") {
            let member = source["name"].as_str().expect("source name");
            assert!(
                !member.is_empty()
                    && !matches!(member, "." | "..")
                    && member
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            );
            assert!(members.insert(member), "duplicate member");
            let uri = source["uri"].as_str().expect("uri");
            assert_eq!(uri, format!("nitride-lib:/{name}/{pin}/{member}"));
            let path = root.join("libraries").join(name).join(pin).join(member);
            println!("cargo:rerun-if-changed={}", path.display());
            let bytes = std::fs::read(&path).expect("bundled source");
            let digest = source["sha256"].as_str().expect("sha256");
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                digest,
                "bundled source changed without its version manifest"
            );
            writeln!(generated, "LibrarySource {{ name: {member:?}, uri: {uri:?}, sha256: {digest:?}, text: include_str!({:?}) }},", path.to_str().unwrap()).unwrap();
        }
        generated.push_str("] },\n");
    }
    generated.push_str("];\n");
    std::fs::write(
        PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("registry.rs"),
        generated,
    )
    .unwrap();
}
