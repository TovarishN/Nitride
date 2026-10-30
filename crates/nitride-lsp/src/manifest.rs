//! Workspace manifests: `nitride.nproj` and the language files it includes,
//! else Nitrogen's `nitrogen.json` (until Nitrogen reads `.nproj`).
use nitride_prepare::{ProjectLanguage, Source};
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
    /// The `.nbind` files that bind its catalogs' interfaces.
    pub bindings: Vec<PathBuf>,
    pub mode: Mode,
    /// Where a `.nproj` names the start rule.
    pub start_span: Option<[u32; 2]>,
}

/// A manifest problem at a byte span of the manifest text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError {
    pub span: [u32; 2],
    pub message: String,
}

/// The workspace file, then Nitrogen's manifest.
pub const NAMES: [&str; 2] = ["nitride.nproj", "nitrogen.json"];

/// A manifest's valid languages and the problems of its invalid ones; `Err`
/// when the manifest as a whole is unreadable (its languages are kept).
pub type Parsed = Result<(Vec<Entry>, Vec<ManifestError>), Vec<ManifestError>>;

/// The manifests at a workspace folder root, each with its languages and
/// problems: `nitride.nproj` (problems only) and every `.nproj` it includes,
/// else `nitrogen.json`, whose languages are syntax-only. `open` supplies
/// editor buffers, which win over disk.
pub fn read(folder: &Path, open: &dyn Fn(&Path) -> Option<String>) -> Vec<(PathBuf, Parsed)> {
    let text = |path: &Path| open(path).or_else(|| std::fs::read_to_string(path).ok());
    let workspace = folder.join(NAMES[0]);
    if let Some(source) = text(&workspace) {
        return projects(&workspace, source, &text, open);
    }
    let fallback = folder.join(NAMES[1]);
    match text(&fallback) {
        Some(source) => vec![(fallback, nitrogen(&source, folder))],
        None => vec![],
    }
}

fn projects(
    workspace: &Path,
    text: String,
    read: &dyn Fn(&Path) -> Option<String>,
    open: &dyn Fn(&Path) -> Option<String>,
) -> Vec<(PathBuf, Parsed)> {
    let source = Source {
        path: workspace.display().to_string(),
        text,
    };
    let exists = |path: &Path| open(path).is_some() || path.exists();
    let resolved = nitride_prepare::read_workspace(&source, read, &exists);
    if resolved.damaged {
        // Mid-edit, often: the registry keeps the languages it included.
        let problems = resolved.errors.into_iter().map(|e| ManifestError {
            span: e.span.map(|v| v as u32),
            message: e.message,
        });
        return vec![(workspace.to_owned(), Err(problems.collect()))];
    }
    let mut found: Vec<(PathBuf, Parsed)> = vec![(workspace.to_owned(), Ok((vec![], vec![])))];
    for language in resolved.languages {
        found.push((language.path.clone(), Ok((vec![entry(language)], vec![]))));
    }
    for error in resolved.errors {
        let path = PathBuf::from(&error.path);
        let problem = ManifestError {
            span: error.span.map(|v| v as u32),
            message: error.message,
        };
        match found.iter_mut().find(|(p, _)| *p == path) {
            Some((_, Ok((_, problems)))) => problems.push(problem),
            Some((_, Err(problems))) => problems.push(problem),
            // A language file that did not resolve: its earlier languages stay.
            None => found.push((path, Err(vec![problem]))),
        }
    }
    found
}

fn entry(language: ProjectLanguage) -> Entry {
    Entry {
        mode: if language.typed() {
            Mode::Typed
        } else {
            Mode::Syntax
        },
        name: language.name,
        extensions: language.extensions,
        grammars: language.grammars,
        start: language.start,
        start_span: Some(language.start_span.map(|v| v as u32)),
        catalogs: language.catalogs,
        bindings: language.bindings,
    }
}

/// Languages from `nitrogen.json` text, all syntax-only; paths resolve against
/// `folder`. An invalid entry is reported and skipped; the others still load.
fn nitrogen(text: &str, folder: &Path) -> Parsed {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        let offset = offset_of(text, e.line(), e.column());
        vec![ManifestError {
            span: [offset, offset],
            message: format!("invalid manifest JSON: {e}"),
        }]
    })?;
    let invalid = |message: String| ManifestError {
        span: [0, 0],
        message,
    };
    let languages = value["languages"]
        .as_array()
        .ok_or_else(|| vec![invalid("manifest needs a \"languages\" array".into())])?;
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
        Ok(Entry {
            name: string("name")?,
            extensions: strings("extensions")?,
            grammars: strings("grammars")?
                .iter()
                .map(|g| folder.join(g))
                .collect(),
            start: string("start")?,
            catalogs: vec![],
            bindings: vec![],
            mode: Mode::Syntax,
            start_span: None,
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
