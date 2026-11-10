use nitride_prepare::ProjectReader;
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{self, Read},
    path::{Path, PathBuf},
};
pub const MAX_FILE: usize = 16 * 1024 * 1024;
type CachedInput = Result<String, (io::ErrorKind, String)>;
#[derive(Default)]
pub struct Snapshot {
    files: HashMap<PathBuf, String>,
    roots: Option<Vec<PathBuf>>,
    cache: std::sync::Mutex<HashMap<PathBuf, CachedInput>>,
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
            || value.as_object().is_none_or(|m| {
                m.keys()
                    .any(|k| !matches!(k.as_str(), "version" | "files" | "roots"))
            })
        {
            return Err("expected snapshot version1 with files".into());
        }
        let files = value["files"]
            .as_array()
            .ok_or("snapshot requires files array")?;
        let mut result = Self::default();
        if let Some(roots) = value.get("roots") {
            result.roots = Some(
                roots
                    .as_array()
                    .ok_or("snapshot roots must be paths")?
                    .iter()
                    .map(|r| {
                        let path = Path::new(r.as_str().ok_or("snapshot root must be string")?);
                        if !path.is_absolute() {
                            return Err("snapshot root must be absolute".to_owned());
                        }
                        path.canonicalize().map_err(|e| e.to_string())
                    })
                    .collect::<Result<_, _>>()?,
            );
        }

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
            result.allowed(&path).map_err(|e| e.to_string())?;
            if result.files.insert(path, text.into()).is_some() {
                return Err("duplicate snapshot identity".into());
            }
        }
        Ok(result)
    }
    fn allowed(&self, path: &Path) -> io::Result<()> {
        if self
            .roots
            .as_ref()
            .is_some_and(|roots| !roots.iter().any(|root| path.starts_with(root)))
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "test input outside allowed workspace roots: {}",
                    path.display()
                ),
            ));
        }
        Ok(())
    }
    pub fn read(&self, path: &Path) -> io::Result<String> {
        <Self as ProjectReader>::read(self, path)
    }
}
impl ProjectReader for Snapshot {
    fn read(&self, path: &Path) -> io::Result<String> {
        let key = identity(path)?;
        self.allowed(&key)?;
        if let Some(text) = self.files.get(&key) {
            return Ok(text.clone());
        }
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| io::Error::other("snapshot lock poisoned"))?;
        if let Some(result) = cache.get(&key) {
            return result
                .clone()
                .map_err(|(kind, message)| io::Error::new(kind, message));
        }
        let result = (|| -> io::Result<String> {
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take((MAX_FILE + 1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_FILE {
                return Err(io::Error::other("input exceeds16 MiB"));
            }
            String::from_utf8(bytes).map_err(io::Error::other)
        })();
        cache.insert(
            key,
            result
                .as_ref()
                .map(Clone::clone)
                .map_err(|e| (e.kind(), e.to_string())),
        );
        result
    }

    fn exists(&self, path: &Path) -> bool {
        if self.canonical(path).is_err() {
            return false;
        }

        if let Ok(key) = identity(path) {
            if self.files.contains_key(&key) {
                return true;
            }
            if let Ok(cache) = self.cache.lock()
                && let Some(result) = cache.get(&key)
            {
                return result.is_ok();
            }
        }
        path.exists()
    }
    fn canonical(&self, path: &Path) -> io::Result<PathBuf> {
        let key = identity(path)?;
        self.allowed(&key)?;
        Ok(key)
    }
}
