use nitride_prepare::{ProjectReader, prepare_project_with};
use std::path::{Path, PathBuf};
struct Reader {
    root: PathBuf,
}
impl ProjectReader for Reader {
    fn read(&self, path: &Path) -> std::io::Result<String> {
        match path.file_name().unwrap().to_str().unwrap() {
            "nitride.nproj" => Ok("language Test { grammar Test.ngr; start Test.Program; }".into()),
            "Test.ngr" => Ok("syntax module Test { syntax Program = \"overlay\"; }".into()),
            _ => std::fs::read_to_string(path),
        }
    }
    fn exists(&self, path: &Path) -> bool {
        self.read(path).is_ok()
    }
    fn canonical(&self, path: &Path) -> std::io::Result<PathBuf> {
        Ok(self.root.join(path.file_name().unwrap()))
    }
}
#[test]
fn preparation_accepts_new_unsaved_project_inputs_without_writing() {
    let root = std::env::temp_dir().join(format!("nitride-project-overlay-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let reader = Reader { root: root.clone() };
    let prepared = prepare_project_with(&root.join("nitride.nproj"), &reader).unwrap();
    assert_eq!(prepared.inputs.len(), 2);
    assert!(!root.join("Test.ngr").exists());
    let language = nitride_codegen::LoadedLanguage::load(&prepared.data.to_string()).unwrap();
    nitride_core::parse(
        language.language(),
        "overlay",
        nitride_core::Limits::default(),
    )
    .unwrap();
    std::fs::remove_dir(root).unwrap();
}
