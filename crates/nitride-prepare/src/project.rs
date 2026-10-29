use crate::{PrepareError, Source};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct PreparedProject {
    pub data: Value,
    pub inputs: Vec<PathBuf>,
}

fn error(path: &Path, message: impl Into<String>) -> PrepareError {
    Source {
        path: path.display().to_string(),
        text: String::new(),
    }
    .project_error(message)
}
fn read(path: &Path) -> Result<String, PrepareError> {
    std::fs::read_to_string(path).map_err(|e| error(path, e.to_string()))
}

pub fn prepare_project(path: &Path) -> Result<PreparedProject, PrepareError> {
    let path = path
        .canonicalize()
        .map_err(|e| error(path, e.to_string()))?;
    let text = read(&path)?;
    let manifest: Value = serde_json::from_str(&text)
        .map_err(|e| error(&path, format!("invalid manifest JSON: {e}")))?;
    let required = |key: &str| {
        manifest[key]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| error(&path, format!("manifest requires string '{key}'")))
    };
    let start = required("start")?;
    if manifest.get("catalog").is_some() {
        return Err(error(
            &path,
            "'catalog' is replaced by 'catalogs', a list of .ncatalog files",
        ));
    }
    let mode = manifest
        .get("mode")
        .map(|m| {
            m.as_str()
                .ok_or_else(|| error(&path, "mode must be a string"))
        })
        .transpose()?
        .unwrap_or("typed");
    let grammars = manifest["grammars"]
        .as_array()
        .filter(|g| !g.is_empty())
        .ok_or_else(|| error(&path, "manifest requires nonempty grammars array"))?;
    let catalogs = match manifest.get("catalogs") {
        None => &[][..],
        Some(list) => list
            .as_array()
            .ok_or_else(|| error(&path, "catalogs must be an array"))?,
    };
    let mut inputs = vec![path.clone()];
    let mut load = |entry: &Value, what: &str| -> Result<Source, PrepareError> {
        let name = entry
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| error(&path, format!("{what} path must be a nonempty string")))?;
        let file = path.parent().unwrap().join(name);
        let text = read(&file)?;
        let file = file
            .canonicalize()
            .map_err(|e| error(&file, e.to_string()))?;
        let source = Source {
            path: file.display().to_string(),
            text,
        };
        inputs.push(file);
        Ok(source)
    };
    let sources = grammars
        .iter()
        .map(|g| load(g, "grammar"))
        .collect::<Result<Vec<_>, _>>()?;
    let catalog_sources = catalogs
        .iter()
        .map(|c| load(c, "catalog"))
        .collect::<Result<Vec<_>, _>>()?;
    let catalog = crate::catalog(&catalog_sources)?;
    let data = catalog.prepare(&sources, start, mode)?.data;
    Ok(PreparedProject { data, inputs })
}

pub fn write_prepared(project: &PreparedProject, path: &Path) -> Result<(), PrepareError> {
    let canonical = if path.exists() {
        path.canonicalize()
    } else {
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()
            .map(|p| p.join(path.file_name().unwrap_or_default()))
    }
    .map_err(|e| error(path, e.to_string()))?;
    if project.inputs.contains(&canonical) {
        return Err(error(path, "output would overwrite a project input"));
    }
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = path.with_file_name(format!(
        ".nitride-prepare-{}-{sequence}.tmp",
        std::process::id()
    ));
    // Only the invocation that created a temporary file owns its cleanup.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| error(path, e.to_string()))?;
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        let text = serde_json::to_string_pretty(&project.data).expect("JSON value serializes");
        file.write_all(text.as_bytes())?;
        file.write_all(b"\n")?;
        drop(file);
        std::fs::rename(&temporary, path)
    };
    match write() {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temporary);
            Err(error(path, e.to_string()))
        }
    }
}
