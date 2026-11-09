//! Immutable, offline grammar sources bundled with Nitride.
mod version;
pub use version::valid_version;

#[derive(Clone, Copy, Debug)]
pub struct LibrarySource {
    pub name: &'static str,
    pub uri: &'static str,
    pub text: &'static str,
    pub sha256: &'static str,
}
#[derive(Clone, Copy, Debug)]
pub struct Library {
    pub name: &'static str,
    pub version: &'static str,
    pub exports: &'static [&'static str],
    pub sources: &'static [LibrarySource],
}
include!(concat!(env!("OUT_DIR"), "/registry.rs"));

pub fn libraries() -> &'static [Library] {
    LIBRARIES
}
pub fn find(name: &str, version: &str) -> Option<&'static Library> {
    libraries()
        .iter()
        .find(|library| library.name == name && library.version == version)
}
/// Only canonical registry identities are accepted. This never accesses paths.
pub fn source(uri: &str) -> Option<&'static LibrarySource> {
    libraries()
        .iter()
        .flat_map(|library| library.sources)
        .find(|source| source.uri == uri)
}
pub fn manifest() -> &'static str {
    include_str!("../libraries/manifest.json")
}
