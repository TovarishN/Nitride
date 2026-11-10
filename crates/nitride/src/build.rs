//! Cargo build-script support. Enabled only by the `build` feature.
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct BuildError(String);
impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for BuildError {}

/// Prepare a project and emit its Rust tables and interface bindings below
/// Cargo's `OUT_DIR`. Relative project paths resolve from `CARGO_MANIFEST_DIR`.
/// `output` must be a single ASCII `.rs` filename. Every project input is
/// registered with `cargo:rerun-if-changed`. The returned path is the generated file.
///
/// Generated code names `nitride_core`; a facade-only consumer must declare
/// `extern crate nitride as nitride_core;` at its crate root, then include this
/// output in a module. See the standalone calculator and embedding guide.
pub fn compile_project(project: impl AsRef<Path>, output: &str) -> Result<PathBuf, BuildError> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| {
        BuildError(
            "CARGO_MANIFEST_DIR is missing; call compile_project from a Cargo build script".into(),
        )
    })?;
    let out = std::env::var_os("OUT_DIR").ok_or_else(|| {
        BuildError("OUT_DIR is missing; call compile_project from a Cargo build script".into())
    })?;
    let (path, inputs) = generate_into(
        project.as_ref(),
        output,
        Path::new(&manifest),
        Path::new(&out),
    )?;
    for input in inputs {
        println!("cargo:rerun-if-changed={}", input.display());
    }
    Ok(path)
}

fn generate_into(
    project: &Path,
    output: &str,
    manifest: &Path,
    out: &Path,
) -> Result<(PathBuf, Vec<PathBuf>), BuildError> {
    if !output.ends_with(".rs")
        || output.len() <= 3
        || !output
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(BuildError(
            "output must be a single ASCII .rs filename below OUT_DIR".into(),
        ));
    }
    let project = if project.is_absolute() {
        project.to_owned()
    } else {
        manifest.join(project)
    };
    let prepared = nitride_prepare::prepare_project(&project)
        .map_err(|error| BuildError(error.to_string()))?;
    let json = prepared.data.to_string();
    let mut generated = nitride_codegen::generate(&json).map_err(BuildError)?;
    generated.push_str(&nitride_codegen::interfaces(&json).map_err(BuildError)?);
    let path = out.join(output);
    std::fs::write(&path, generated)
        .map_err(|error| BuildError(format!("cannot write {}: {error}", path.display())))?;
    Ok((path, prepared.inputs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "nitride-build-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn prepares_split_grammar_tracks_all_inputs_and_regenerates_changes() {
        let scratch = Scratch::new();
        let root = &scratch.0;
        std::fs::create_dir(root.join("out")).unwrap();
        std::fs::write(
            root.join("project.json"),
            r#"{"mode":"syntax","start":"Tiny.Start","grammars":["Main.ngr","Tokens.ngr"]}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("Main.ngr"),
            "syntax module Tiny { syntax Start = Tokens.Number; }",
        )
        .unwrap();
        std::fs::write(
            root.join("Tokens.ngr"),
            "syntax module Tokens { token Number = ['0'..'9']+; }",
        )
        .unwrap();
        let (output, inputs) = generate_into(
            Path::new("project.json"),
            "tiny.rs",
            root,
            &root.join("out"),
        )
        .unwrap();
        assert_eq!(output, root.join("out/tiny.rs"));
        for name in ["project.json", "Main.ngr", "Tokens.ngr"] {
            assert!(
                inputs.contains(&root.join(name).canonicalize().unwrap()),
                "{name}"
            );
        }
        let before = std::fs::read(&output).unwrap();
        std::fs::write(
            root.join("Tokens.ngr"),
            "syntax module Tokens { token Number = ['a'..'z']+; }",
        )
        .unwrap();
        generate_into(
            Path::new("project.json"),
            "tiny.rs",
            root,
            &root.join("out"),
        )
        .unwrap();
        assert_ne!(std::fs::read(output).unwrap(), before);
    }

    #[test]
    fn rejects_output_traversal_before_writing_anything() {
        let scratch = Scratch::new();
        for output in [
            "../escape.rs",
            "/escape.rs",
            "a/b.rs",
            r"a\b.rs",
            "C:escape.rs",
            "",
            ".rs",
        ] {
            let error = generate_into(Path::new("missing.json"), output, &scratch.0, &scratch.0)
                .unwrap_err();
            assert!(error.to_string().contains("filename"), "{output}: {error}");
        }
        assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 0);
    }

    #[test]
    fn invalid_project_does_not_replace_existing_output() {
        let scratch = Scratch::new();
        std::fs::write(scratch.0.join("project.json"), "not JSON").unwrap();
        let out = scratch.0.join("language.rs");
        std::fs::write(&out, "previous output").unwrap();
        let error = generate_into(
            Path::new("project.json"),
            "language.rs",
            &scratch.0,
            &scratch.0,
        )
        .unwrap_err();
        assert!(error.to_string().contains("project.json"));
        assert_eq!(std::fs::read_to_string(out).unwrap(), "previous output");
    }
}
