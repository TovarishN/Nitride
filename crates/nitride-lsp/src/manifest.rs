//! Workspace manifests: `nitride.json`, else `nitrogen.json`.
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Typed,
    Syntax,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub extensions: Vec<String>,
    pub grammars: Vec<PathBuf>,
    pub start: String,
    /// The `.ncatalog` files of a typed language, in merge order.
    pub catalogs: Vec<PathBuf>,
    pub mode: Mode,
}

/// A manifest problem at a byte span of the manifest text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError {
    pub span: [u32; 2],
    pub message: String,
}

pub const NAMES: [&str; 2] = ["nitride.json", "nitrogen.json"];

/// A manifest's valid languages and the problems of its invalid ones; `Err`
/// when the manifest as a whole is unreadable.
pub type Parsed = Result<(Vec<Entry>, Vec<ManifestError>), ManifestError>;

/// The manifest at a workspace folder root and its languages. Nitride's own
/// `catalogs` and `mode` are read only from `nitride.json`; a `nitrogen.json`
/// language is syntax-only.
pub fn read(folder: &Path) -> (Option<PathBuf>, Parsed) {
    for name in NAMES {
        let path = folder.join(name);
        if let Ok(text) = std::fs::read_to_string(&path) {
            let entries = parse(&text, folder, name == NAMES[0]);
            return (Some(path), entries);
        }
    }
    (None, Ok((vec![], vec![])))
}

/// Languages from manifest text; paths resolve against `folder`. An invalid
/// entry is reported and skipped; the others still load.
pub fn parse(text: &str, folder: &Path, nitride: bool) -> Parsed {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        let offset = offset_of(text, e.line(), e.column());
        ManifestError {
            span: [offset, offset],
            message: format!("invalid manifest JSON: {e}"),
        }
    })?;
    let invalid = |message: String| ManifestError {
        span: [0, 0],
        message,
    };
    let languages = value["languages"]
        .as_array()
        .ok_or_else(|| invalid("manifest needs a \"languages\" array".into()))?;
    let entries = languages.iter().enumerate().map(|(i, language)| {
        let string = |key: &str| {
            language[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid(format!("language {i} needs a string \"{key}\"")))
        };
        let strings = |key: &str| -> Result<Vec<String>, ManifestError> {
            language[key]
                .as_array()
                .and_then(|a| a.iter().map(|v| v.as_str().map(str::to_owned)).collect())
                .ok_or_else(|| invalid(format!("language {i} needs a string array \"{key}\"")))
        };
        if nitride && language.get("catalog").is_some() {
            return Err(invalid(format!(
                "language {i}: \"catalog\" is replaced by \"catalogs\", a list of .ncatalog files"
            )));
        }
        let catalogs = if nitride && language.get("catalogs").is_some() {
            strings("catalogs")?
                .iter()
                .map(|c| folder.join(c))
                .collect()
        } else {
            vec![]
        };
        let mode = match (nitride, language["mode"].as_str()) {
            (false, _) => Mode::Syntax,
            (true, Some("typed")) => Mode::Typed,
            (true, Some("syntax")) => Mode::Syntax,
            (true, None) if !catalogs.is_empty() => Mode::Typed,
            (true, None) => Mode::Syntax,
            (true, Some(other)) => {
                return Err(invalid(format!(
                    "language {i} has unknown mode \"{other}\""
                )));
            }
        };
        Ok(Entry {
            name: string("name")?,
            extensions: strings("extensions")?,
            grammars: strings("grammars")?
                .iter()
                .map(|g| folder.join(g))
                .collect(),
            start: string("start")?,
            catalogs,
            mode,
        })
    });
    let mut valid = Vec::new();
    let mut problems = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => valid.push(entry),
            Err(problem) => problems.push(problem),
        }
    }
    Ok((valid, problems))
}

/// The byte offset of a one-based line and column, as serde_json reports them.
fn offset_of(text: &str, line: usize, column: usize) -> u32 {
    let start: usize = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum();
    (start + column.saturating_sub(1)).min(text.len()) as u32
}
