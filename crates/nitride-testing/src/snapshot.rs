use nitride_prepare::ProjectReader;
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{self, Read},
    path::{Path, PathBuf},
};
pub const MAX_FILE: usize = 16 * 1024 * 1024;
#[derive(Default)]
pub struct Snapshot {
    files: HashMap<PathBuf, String>,
}
pub fn identity(path: &Path) -> io::Result<PathBuf> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let absolute = if path.is_absolute() {
                path.to_owned()
            } else {
                std::env::current_dir()?.join(path)
            };
            Ok(absolute
                .parent()
                .ok_or_else(|| io::Error::other("input needs parent"))?
                .canonicalize()?
                .join(
                    absolute
                        .file_name()
                        .ok_or_else(|| io::Error::other("input needs file name"))?,
                ))
        }
        Err(e) => Err(e),
    }
}
impl Snapshot {
    pub fn from_json(value: &Value) -> Result<Self, String> {
        if value["version"] != 1
            || value
                .as_object()
                .is_none_or(|m| m.keys().any(|k| !matches!(k.as_str(), "version" | "files")))
        {
            return Err("expected snapshot version1 with files".into());
        }
        let files = value["files"]
            .as_array()
            .ok_or("snapshot requires files array")?;
        let mut result = Self::default();
        let mut size = 0usize;
        for file in files {
            if file.as_object().is_none_or(|m| m.len() != 2) {
                return Err("snapshot file requires path and text".into());
            }
            let path = Path::new(
                file["path"]
                    .as_str()
                    .ok_or("snapshot path must be string")?,
            );
            let text = file["text"]
                .as_str()
                .ok_or("snapshot text must be string")?;
            if !path.is_absolute() {
                return Err("snapshot identity must be an absolute physical path".into());
            }
            size = size.saturating_add(text.len());
            if text.len() > MAX_FILE || size > 64 * 1024 * 1024 {
                return Err("snapshot size limit".into());
            }
            let path = identity(path).map_err(|e| e.to_string())?;
            if result.files.insert(path, text.into()).is_some() {
                return Err("duplicate snapshot identity".into());
            }
        }
        Ok(result)
    }
    pub fn read(&self, path: &Path) -> io::Result<String> {
        <Self as ProjectReader>::read(self, path)
    }
}
impl ProjectReader for Snapshot {
    fn read(&self, path: &Path) -> io::Result<String> {
        if let Ok(path) = identity(path)
            && let Some(text) = self.files.get(&path)
        {
            return Ok(text.clone());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take((MAX_FILE + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_FILE {
            return Err(io::Error::other("input exceeds16 MiB"));
        }
        String::from_utf8(bytes).map_err(io::Error::other)
    }
    fn exists(&self, path: &Path) -> bool {
        identity(path).is_ok_and(|p| self.files.contains_key(&p)) || path.exists()
    }
    fn canonical(&self, path: &Path) -> io::Result<PathBuf> {
        identity(path)
    }
}
