use super::{AuthoringError, ProjectPlan, error};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
#[derive(Debug)]
pub struct CreationReport {
    pub created: Vec<PathBuf>,
}
/// Editor targets must be lexical descendants of a root and traverse no symlinks.
/// The CLI also requires a new leaf directory; its explicit parent may be elsewhere.
pub fn workspace_target(target: &Path, roots: &[PathBuf]) -> Result<PathBuf, AuthoringError> {
    if !target.is_absolute()
        || target
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(error(
            "target must be an absolute normalized workspace path",
        ));
    }
    let root = roots
        .iter()
        .find(|root| target.starts_with(root) && target != root.as_path())
        .ok_or_else(|| error("target must be inside an open workspace"))?;
    let root_real = root.canonicalize().map_err(|e| error(e.to_string()))?;
    let mut at = root.clone();
    for part in target.strip_prefix(root).expect("prefix").components() {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(error("target traverses a symlink"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(error(e.to_string())),
        }
    }
    let parent = target
        .parent()
        .ok_or_else(|| error("target needs a parent"))?
        .canonicalize()
        .map_err(|e| error(e.to_string()))?;
    if !parent.starts_with(root_real) {
        return Err(error("target escapes workspace"));
    }
    Ok(target.to_owned())
}
pub fn create_language(
    target: &Path,
    plan: &ProjectPlan,
) -> Result<CreationReport, AuthoringError> {
    create_with(target, plan, &mut |_, _| Ok(()))
}
fn create_with(
    target: &Path,
    plan: &ProjectPlan,
    before: &mut dyn FnMut(usize, &Path) -> std::io::Result<()>,
) -> Result<CreationReport, AuthoringError> {
    let mut names = std::collections::HashSet::new();
    if plan.files.is_empty()
        || plan.files.iter().any(|f| {
            f.relative_path.components().count() != 1
                || !matches!(
                    f.relative_path.components().next(),
                    Some(Component::Normal(_))
                )
                || !names.insert(f.relative_path.to_string_lossy().to_lowercase())
        })
    {
        return Err(error("plan contains unsafe or duplicate paths"));
    }
    std::fs::create_dir(target)
        .map_err(|e| error(format!("cannot create {}: {e}", target.display())))?;
    let mut owned: Vec<(&super::PlannedFile, PathBuf)> = Vec::new();
    let mut files = plan.files.iter().collect::<Vec<_>>();
    files.sort_by_key(|f| f.kind == "project");
    let result = (|| -> std::io::Result<()> {
        for (index, file) in files.iter().enumerate() {
            let path = target.join(&file.relative_path);
            before(index, &path)?;
            let mut handle = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            owned.push((file, path));
            handle.write_all(file.contents.as_bytes())?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(CreationReport {
            created: owned.into_iter().map(|(_, p)| p).collect(),
        }),
        Err(cause) => {
            let mut retained = Vec::new();
            for (file, path) in owned.into_iter().rev() {
                let untouched = std::fs::symlink_metadata(&path)
                    .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
                    && std::fs::read(&path).is_ok_and(|b| b == file.contents.as_bytes());
                if !untouched || std::fs::remove_file(&path).is_err() {
                    retained.push(path);
                }
            }
            if std::fs::remove_dir(target).is_err() {
                retained.push(target.to_owned());
            }
            Err(AuthoringError {
                message: format!("creation failed: {cause}"),
                retained,
            })
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_files_and_modified_owned_files_survive_failure() {
        let root = std::env::temp_dir().join(format!("nitride-create-race-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let plan = super::super::plan_language(&super::super::TemplateOptions {
            name: "Test".into(),
            extension: ".test".into(),
            kind: super::super::TemplateKind::Syntax,
        })
        .unwrap();
        let target = root.join("Test");
        let failure = create_with(&target, &plan, &mut |index, path| {
            if index == 1 {
                std::fs::write(target.join("Test.ngr"), "user edit")?;
                std::fs::write(path, "concurrent file")?;
            }
            Ok(())
        })
        .unwrap_err();
        assert!(!failure.retained.is_empty());
        assert_eq!(
            std::fs::read_to_string(target.join("Test.ngr")).unwrap(),
            "user edit"
        );
        assert_eq!(
            std::fs::read_to_string(target.join("sample.test")).unwrap(),
            "concurrent file"
        );
        assert!(!target.join("nitride.nproj").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
