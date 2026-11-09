use nitride_prepare::{Source, prepare_project, read_language};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "nitride-libraries-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("Test.ngr"),"syntax module Test { token Name = Nitride.Lexical.AsciiIdentifier except \"let\"; syntax Start = Name; }").unwrap();
        Self(path)
    }
    fn project(&self, declarations: &str) -> PathBuf {
        let path = self.0.join("nitride.nproj");
        std::fs::write(&path,format!("language Test {{ extensions .test; {declarations} grammar Test.ngr; start Test.Start; }}")).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn nproj_pin_resolves_offline_without_physical_library_inputs() {
    let f = Fixture::new();
    let path = f.project("library Nitride.Lexical version 1.0.0;");
    let text = std::fs::read_to_string(&path).unwrap();
    let project = read_language(
        &Source {
            path: path.display().to_string(),
            text: text.clone(),
        },
        &Path::exists,
    )
    .unwrap();
    assert_eq!(project.libraries[0].name, "Nitride.Lexical");
    assert_eq!(project.libraries[0].version, "1.0.0");
    assert_eq!(
        &text[project.libraries[0].version_span[0]..project.libraries[0].version_span[1]],
        "1.0.0"
    );
    let prepared = prepare_project(&path).unwrap();
    assert_eq!(prepared.inputs.len(), 2);
    assert!(
        prepared
            .inputs
            .iter()
            .all(|p| p.starts_with(f.0.canonicalize().unwrap()))
    );
    assert_eq!(
        prepared.libraries[0].sources[0].uri,
        "nitride-lib:/Nitride.Lexical/1.0.0/Lexical.ngr"
    );
    let owned = nitride_codegen::LoadedLanguage::load(&prepared.data.to_string()).unwrap();
    assert!(nitride_core::parse(owned.language(), "letter", Default::default()).is_ok());
    assert!(nitride_core::parse(owned.language(), "let", Default::default()).is_err());
}

#[test]
fn json_pins_and_relocated_projects_prepare_identically() {
    let a = Fixture::new();
    let b = Fixture::new();
    let first = prepare_project(&a.project("library Nitride.Lexical version 1.0.0;")).unwrap();
    let path = b.0.join("project.json");
    std::fs::write(&path,r#"{"grammars":["Test.ngr"],"start":"Test.Start","mode":"syntax","libraries":[{"name":"Nitride.Lexical","version":"1.0.0"}]}"#).unwrap();
    let second = prepare_project(&path).unwrap();
    assert_eq!(first.data, second.data);
    assert_eq!(second.inputs.len(), 2);
}

#[test]
fn bad_or_conflicting_pins_do_not_fall_back_and_point_at_the_reference() {
    let f = Fixture::new();
    for (declaration, needle) in [
        ("library Unknown.Library version 1.0.0;", "Unknown.Library"),
        ("library Nitride.Lexical version 9.0.0;", "9.0.0"),
        ("library Nitride.Lexical version 01.0.0;", "01.0.0"),
    ] {
        let path = f.project(declaration);
        let text = std::fs::read_to_string(&path).unwrap();
        let error = prepare_project(&path).unwrap_err();
        assert_eq!(
            &text[error.span[0]..error.span[1]],
            needle,
            "{}",
            error.message
        );
        assert!(error.project);
        assert!(error.code.as_deref().unwrap().starts_with("NLIB"));
    }
    for second in ["1.0.0", "1.0.1"] {
        let path = f.project(&format!(
            "library Nitride.Lexical version 1.0.0; library Nitride.Lexical version {second};"
        ));
        let error = prepare_project(&path).unwrap_err();
        assert_eq!(error.code.as_deref(), Some("NLIB0003"));
    }
    assert!(
        prepare_project(&f.project("")).is_err(),
        "library modules are not implicit imports"
    );
}
