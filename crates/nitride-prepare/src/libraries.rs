//! Exact, offline project library resolution shared by CLI and editor.
use crate::{PrepareError, Source, stdlib};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryReference {
    pub name: String,
    pub version: String,
    pub span: [usize; 2],
    pub name_span: [usize; 2],
    pub version_span: [usize; 2],
}
pub type ResolvedLibrary = stdlib::Library;

pub fn resolve_libraries(
    source: &Source,
    references: &[LibraryReference],
) -> Result<Vec<ResolvedLibrary>, Vec<PrepareError>> {
    resolve_with(source, references, stdlib::libraries())
}
fn resolve_with(
    source: &Source,
    references: &[LibraryReference],
    registry: &[stdlib::Library],
) -> Result<Vec<ResolvedLibrary>, Vec<PrepareError>> {
    let mut seen = std::collections::HashSet::new();
    let mut errors = Vec::new();
    let mut resolved = Vec::new();
    for reference in references {
        let problem = if !seen.insert(&reference.name) {
            Some((
                "NLIB0003",
                reference.span,
                format!("library {} is declared more than once", reference.name),
            ))
        } else if !registry.iter().any(|l| l.name == reference.name) {
            Some((
                "NLIB0001",
                reference.name_span,
                format!(
                    "unknown bundled library {}; available: {}",
                    reference.name,
                    registry
                        .iter()
                        .map(|l| l.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ))
        } else if let Some(library) = registry.iter().find(|l| {
            l.name == reference.name
                && l.version == reference.version
                && stdlib::valid_version(&reference.version)
        }) {
            resolved.push(*library);
            None
        } else {
            Some((
                "NLIB0002",
                reference.version_span,
                format!(
                    "unavailable exact version {}@{}; available: {}",
                    reference.name,
                    reference.version,
                    registry
                        .iter()
                        .filter(|l| l.name == reference.name)
                        .map(|l| l.version)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ))
        };
        if let Some((code, span, message)) = problem {
            let mut error = source.project_error_at(span, message);
            error.code = Some(code.into());
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(resolved)
    } else {
        Err(errors)
    }
}
pub fn library_sources(libraries: &[ResolvedLibrary]) -> Vec<Source> {
    libraries
        .iter()
        .flat_map(|l| l.sources)
        .map(|s| Source {
            path: s.uri.into(),
            text: s.text.into(),
        })
        .collect()
}
pub fn library_metadata(libraries: &[ResolvedLibrary]) -> Value {
    json!(libraries.iter().map(|l| json!({"name":l.name,"version":l.version,"sources":l.sources.iter().map(|s|json!({"name":s.name,"uri":s.uri,"sha256":s.sha256})).collect::<Vec<_>>()})).collect::<Vec<_>>())
}
/// Salt identities even when distinct published versions contain identical text.
pub fn apply_libraries(data: &mut Value, libraries: &[ResolvedLibrary]) {
    if !libraries.is_empty() {
        let identity = format!(
            "{};libraries={}",
            data["source_hash"].as_str().unwrap_or_default(),
            library_metadata(libraries)
        );
        data["source_hash"] = format!("{:x}", Sha256::digest(identity.as_bytes())).into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equal_text_different_versions_have_distinct_fingerprints() {
        let first = stdlib::libraries()[0];
        let second = stdlib::Library {
            version: "1.0.1",
            ..first
        };
        let reference = |version: &str| LibraryReference {
            name: first.name.into(),
            version: version.into(),
            span: [0, 0],
            name_span: [0, 0],
            version_span: [0, 0],
        };
        let source = Source {
            path: "test.nproj".into(),
            text: String::new(),
        };
        let registry = [first, second];
        let old = resolve_with(&source, &[reference("1.0.0")], &registry).unwrap();
        let new = resolve_with(&source, &[reference("1.0.1")], &registry).unwrap();
        assert_eq!(library_sources(&old)[0].text, library_sources(&new)[0].text);
        let mut a = json!({"source_hash":"same"});
        let mut b = a.clone();
        apply_libraries(&mut a, &old);
        apply_libraries(&mut b, &new);
        assert_ne!(a, b);
        assert_eq!(old[0].version, "1.0.0");
    }
}
