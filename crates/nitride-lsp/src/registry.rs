//! Workspace languages, prepared at runtime from their grammars and reloaded
//! when a grammar, catalog or manifest changes.
use crate::manifest::{self, Entry, ManifestError, Mode};
use nitride_codegen::LoadedLanguage;
use nitride_prepare::{PrepareError, Prepared, Source};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The catalog of a language without one: no types, operations or statements.
pub const EMPTY_CATALOG: &str = r#"{"types":[],"operations":[],"statements":{}}"#;

pub struct Loaded {
    pub language: LoadedLanguage,
    pub prepared: Prepared,
}

pub struct LanguageState {
    pub entry: Entry,
    pub manifest: PathBuf,
    /// The last language that prepared, kept while a later edit is broken.
    pub current: Option<Arc<Loaded>>,
    /// Why the latest inputs did not prepare (empty when `current` is theirs).
    pub errors: Vec<PrepareError>,
    pub warnings: Vec<PrepareError>,
    inputs: u64,
}

pub struct ManifestState {
    pub path: PathBuf,
    pub errors: Vec<ManifestError>,
}

#[derive(Default)]
pub struct Registry {
    languages: Vec<LanguageState>,
    manifests: Vec<ManifestState>,
}

impl Registry {
    pub fn languages(&self) -> &[LanguageState] {
        &self.languages
    }

    pub fn manifests(&self) -> &[ManifestState] {
        &self.manifests
    }

    /// Re-reads every folder's manifest and re-prepares languages whose
    /// inputs changed. `open` supplies editor buffers, which win over disk.
    pub fn reload(&mut self, folders: &[PathBuf], open: &dyn Fn(&Path) -> Option<String>) {
        let mut previous = std::mem::take(&mut self.languages);
        self.manifests.clear();
        for folder in folders {
            let (path, entries) = manifest::read(folder);
            let Some(path) = path else { continue };
            // An open manifest buffer replaces the file on disk.
            let entries = match open(&path) {
                Some(text) => {
                    let nitride = path.file_name().is_some_and(|n| n == manifest::NAMES[0]);
                    manifest::parse(&text, folder, nitride)
                }
                None => entries,
            };
            self.load(&path, entries, &mut previous, open);
        }
    }

    fn load(
        &mut self,
        path: &Path,
        parsed: manifest::Parsed,
        previous: &mut Vec<LanguageState>,
        open: &dyn Fn(&Path) -> Option<String>,
    ) {
        let (entries, mut errors) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                // An unreadable manifest (often mid-edit) keeps its languages.
                let (kept, rest) = std::mem::take(previous)
                    .into_iter()
                    .partition(|l: &LanguageState| l.manifest == path);
                *previous = rest;
                self.languages.extend(kept);
                self.manifests.push(ManifestState {
                    path: path.to_owned(),
                    errors: vec![error],
                });
                return;
            }
        };
        for entry in entries {
            let old = previous
                .iter()
                .position(|l| l.manifest == path && l.entry.name == entry.name)
                .map(|i| previous.swap_remove(i));
            let state = prepare(entry, path, old, open);
            for error in state.errors.iter().filter(|e| e.project) {
                errors.push(ManifestError {
                    span: [0, 0],
                    message: format!("{}: {}", state.entry.name, error.message),
                });
            }
            self.languages.push(state);
        }
        self.manifests.push(ManifestState {
            path: path.to_owned(),
            errors,
        });
    }

    /// The first language whose extensions match `path`.
    pub fn for_path(&self, path: &Path) -> Option<&LanguageState> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        self.languages.iter().find(|l| {
            l.entry
                .extensions
                .iter()
                .any(|e| name.ends_with(&e.to_lowercase()))
        })
    }

    /// Languages whose grammars include `path`.
    pub fn using_grammar<'a>(&'a self, path: &'a Path) -> impl Iterator<Item = &'a LanguageState> {
        self.languages
            .iter()
            .filter(move |l| l.entry.grammars.iter().any(|g| same_file(g, path)))
    }
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
}

fn read(path: &Path, open: &dyn Fn(&Path) -> Option<String>) -> Result<String, String> {
    open(path)
        .map(Ok)
        .unwrap_or_else(|| std::fs::read_to_string(path))
        .map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn prepare(
    entry: Entry,
    manifest: &Path,
    old: Option<LanguageState>,
    open: &dyn Fn(&Path) -> Option<String>,
) -> LanguageState {
    let mut state = LanguageState {
        current: old.as_ref().and_then(|o| o.current.clone()),
        entry,
        manifest: manifest.to_owned(),
        errors: vec![],
        warnings: vec![],
        inputs: 0,
    };
    let manifest_error = |message: String| PrepareError {
        path: manifest.to_string_lossy().into_owned(),
        span: [0, 0],
        line: 1,
        column: 1,
        message,
        project: true,
    };
    let mut sources = Vec::new();
    for grammar in &state.entry.grammars {
        match read(grammar, open) {
            Ok(text) => sources.push(Source {
                path: grammar.to_string_lossy().into_owned(),
                text,
            }),
            Err(message) => {
                state.errors.push(manifest_error(message));
                return state;
            }
        }
    }
    let catalog = match &state.entry.catalog {
        Some(path) => match read(path, open) {
            Ok(text) => text,
            Err(message) => {
                state.errors.push(manifest_error(message));
                return state;
            }
        },
        None => EMPTY_CATALOG.to_owned(),
    };
    let mut hasher = DefaultHasher::new();
    for source in &sources {
        source.text.hash(&mut hasher);
    }
    (
        &catalog,
        &state.entry.start,
        state.entry.mode == Mode::Typed,
    )
        .hash(&mut hasher);
    state.inputs = hasher.finish();
    if let Some(old) = old.filter(|o| o.inputs == state.inputs) {
        state.errors = old.errors;
        state.warnings = old.warnings;
        return state;
    }
    let prepared = match state.entry.mode {
        Mode::Typed => nitride_prepare::prepare_sources_detailed(
            &sources,
            &catalog,
            &state.entry.start,
            "typed",
        ),
        Mode::Syntax => {
            nitride_prepare::prepare_sources_tolerant(&sources, &catalog, &state.entry.start)
        }
    };
    match prepared {
        Ok(prepared) => match LoadedLanguage::load(&prepared.data.to_string()) {
            Ok(language) => {
                state.warnings = prepared.warnings.clone();
                state.current = Some(Arc::new(Loaded { language, prepared }));
            }
            Err(message) => state.errors.push(manifest_error(message)),
        },
        Err(error) => state.errors.push(error),
    }
    state
}
