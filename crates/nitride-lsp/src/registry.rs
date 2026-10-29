//! Workspace languages, prepared at runtime from their grammars and reloaded
//! when a grammar, catalog or manifest changes.
use crate::manifest::{self, Entry, ManifestError, Mode};
use crate::roles::Role;
use nitride_codegen::LoadedLanguage;
use nitride_prepare::{PrepareError, Prepared, Source};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The catalog of a language without one: no types, operations or statements.
pub const EMPTY_CATALOG: &str = r#"{"types":[],"operations":[],"statements":{}}"#;

pub struct Loaded {
    pub language: LoadedLanguage,
    pub prepared: Prepared,
    /// Each (rule, literal)'s palette role.
    pub roles: Arc<HashMap<(u16, String), Role>>,
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
    /// Returns whether any language or manifest result changed.
    pub fn reload(&mut self, folders: &[PathBuf], open: &dyn Fn(&Path) -> Option<String>) -> bool {
        let before = self.fingerprint();
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
        before != self.fingerprint()
    }

    /// What analyses depend on: each language's identity, extensions, loaded
    /// tables and problems, and each manifest's problems.
    fn fingerprint(&self) -> Vec<String> {
        let languages = self.languages.iter().map(|l| {
            let loaded = l.current.as_ref().map_or(0, |c| Arc::as_ptr(c) as usize);
            let errors: Vec<_> = l.errors.iter().map(|e| e.to_string()).collect();
            let warnings: Vec<_> = l.warnings.iter().map(|w| w.to_string()).collect();
            format!(
                "{}|{}|{:?}|{loaded}|{errors:?}|{warnings:?}",
                l.manifest.display(),
                l.entry.name,
                l.entry.extensions
            )
        });
        let manifests = self
            .manifests
            .iter()
            .map(|m| format!("{}|{:?}", m.path.display(), m.errors));
        languages.chain(manifests).collect()
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

    /// The grammar and catalog files navigation from `path` spans: every
    /// grammar and catalog of the languages that use it, else the file alone.
    pub fn grammar_set(&self, path: &Path) -> Vec<PathBuf> {
        let mut set: Vec<PathBuf> = Vec::new();
        for language in self.using_grammar(path).chain(self.using_catalog(path)) {
            for file in language
                .entry
                .grammars
                .iter()
                .chain(&language.entry.catalogs)
            {
                if !set.iter().any(|g| same_file(g, file)) {
                    set.push(file.clone());
                }
            }
        }
        if set.is_empty() {
            set.push(path.to_owned());
        }
        set
    }

    /// Languages whose grammars include `path`.
    pub fn using_grammar<'a>(&'a self, path: &'a Path) -> impl Iterator<Item = &'a LanguageState> {
        self.languages
            .iter()
            .filter(move |l| l.entry.grammars.iter().any(|g| same_file(g, path)))
    }
}

impl Registry {
    /// Languages whose catalogs include `path`.
    pub fn using_catalog<'a>(&'a self, path: &'a Path) -> impl Iterator<Item = &'a LanguageState> {
        self.languages
            .iter()
            .filter(move |l| l.entry.catalogs.iter().any(|c| same_file(c, path)))
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
    let mut catalogs = Vec::new();
    for path in &state.entry.catalogs {
        match read(path, open) {
            Ok(text) => catalogs.push(Source {
                path: path.to_string_lossy().into_owned(),
                text,
            }),
            Err(message) => {
                state.errors.push(manifest_error(message));
                return state;
            }
        }
    }
    let mut hasher = DefaultHasher::new();
    for source in sources.iter().chain(&catalogs) {
        source.text.hash(&mut hasher);
    }
    (&state.entry.start, state.entry.mode == Mode::Typed).hash(&mut hasher);
    state.inputs = hasher.finish();
    if let Some(old) = old.filter(|o| o.inputs == state.inputs) {
        state.errors = old.errors;
        state.warnings = old.warnings;
        return state;
    }
    let catalog = match nitride_prepare::catalog(&catalogs) {
        Ok(catalog) => catalog,
        Err(error) => {
            state.errors.push(error);
            return state;
        }
    };
    let prepared = match state.entry.mode {
        Mode::Typed => catalog.prepare(&sources, &state.entry.start, "typed"),
        Mode::Syntax => catalog.prepare_tolerant(&sources, &state.entry.start),
    };
    match prepared {
        Ok(prepared) => match LoadedLanguage::load(&prepared.data.to_string()) {
            Ok(language) => {
                state.warnings = prepared.warnings.clone();
                let declares = |rule: &str| prepared.declarations.iter().any(|d| d.rule == rule);
                let roles = Arc::new(crate::roles::roles(language.language(), &declares));
                state.current = Some(Arc::new(Loaded {
                    language,
                    prepared,
                    roles,
                }));
            }
            Err(message) => state.errors.push(manifest_error(message)),
        },
        Err(error) => state.errors.push(error),
    }
    state
}
