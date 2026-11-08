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
    pub language: Arc<LoadedLanguage>,
    pub prepared: Prepared,
    /// Each binding to a WebAssembly component, from the language's `.nbind`
    /// files (prepared data's form), with the component's path.
    pub components: Vec<(serde_json::Value, PathBuf)>,
    /// Each (rule, literal)'s palette role.
    pub roles: Arc<HashMap<(u16, String), Role>>,
}

#[derive(Clone)]
pub struct LanguageState {
    pub entry: Entry,
    pub manifest: PathBuf,
    /// The last language that prepared, kept while a later edit is broken.
    pub current: Option<Arc<Loaded>>,
    /// Why the latest inputs did not prepare (empty when `current` is theirs).
    pub errors: Vec<PrepareError>,
    pub warnings: Vec<PrepareError>,
    /// Declared by a signed grammar (`manifest` is that file), not a manifest.
    pub signed: bool,
    inputs: u64,
}

#[derive(Clone)]
pub struct ManifestState {
    pub path: PathBuf,
    pub errors: Vec<ManifestError>,
}

#[derive(Clone, Default)]
pub struct Registry {
    languages: Vec<LanguageState>,
    manifests: Vec<ManifestState>,
    /// The language files each `nitride.nproj` last included, kept while it
    /// does not parse.
    included: HashMap<PathBuf, Vec<PathBuf>>,
    /// Declared files survive missing inputs and mid-edit project syntax,
    /// independently of whether a language has successfully resolved.
    input_references: HashMap<PathBuf, Vec<PathBuf>>,
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
        self.reload_with(folders, open, &[])
    }

    /// As `reload`, also finding signed grammars among the open documents
    /// `open_paths` (which may not be on disk yet).
    pub fn reload_with(
        &mut self,
        folders: &[PathBuf],
        open: &dyn Fn(&Path) -> Option<String>,
        open_paths: &[PathBuf],
    ) -> bool {
        let before = self.fingerprint();
        let mut previous = std::mem::take(&mut self.languages);
        self.manifests.clear();
        let mut included = HashMap::new();
        for folder in folders {
            let manifests = manifest::read(folder, open);
            if let Some((workspace, parsed)) = manifests.first()
                && workspace
                    .file_name()
                    .is_some_and(|n| n == manifest::NAMES[0])
            {
                let files = match parsed {
                    // An unreadable workspace keeps the languages it included.
                    Err(_) => self.included.remove(workspace).unwrap_or_default(),
                    Ok(_) => manifests[1..].iter().map(|(p, _)| p.clone()).collect(),
                };
                if parsed.is_err() {
                    let (kept, rest) = std::mem::take(&mut previous)
                        .into_iter()
                        .partition(|l: &LanguageState| files.contains(&l.manifest));
                    previous = rest;
                    self.languages.extend(kept);
                }
                included.insert(workspace.clone(), files);
            }
            for (path, entries) in manifests {
                self.load(&path, entries, &mut previous, open);
            }
        }
        self.included = included;
        for path in discover(folders, open, open_paths) {
            self.load_signed(&path, &mut previous, open);
        }
        self.refresh_input_references(open);
        before != self.fingerprint()
    }

    fn refresh_input_references(&mut self, open: &dyn Fn(&Path) -> Option<String>) {
        let mut previous = std::mem::take(&mut self.input_references);
        let paths: std::collections::BTreeSet<_> = self
            .manifests
            .iter()
            .map(|m| m.path.clone())
            .chain(self.included.values().flatten().cloned())
            .collect();
        for path in paths {
            if !path.extension().is_some_and(|ext| ext == "nproj") {
                continue;
            }
            let references = read(&path, open)
                .ok()
                .and_then(|text| {
                    nitride_prepare::project_references(&Source {
                        path: path.to_string_lossy().into_owned(),
                        text,
                    })
                    .ok()
                })
                .map(|references| {
                    references
                        .into_iter()
                        .filter(|r| matches!(r.kind, "grammar" | "catalog" | "binding"))
                        .filter_map(|r| r.target)
                        .collect()
                });
            if let Some(references) = references.or_else(|| previous.remove(&path)) {
                self.input_references.insert(path, references);
            }
        }
    }

    /// The language a signed grammar declares, if it declares one.
    fn load_signed(
        &mut self,
        path: &Path,
        previous: &mut Vec<LanguageState>,
        open: &dyn Fn(&Path) -> Option<String>,
    ) {
        let old = previous
            .iter()
            .position(|l| l.signed && l.manifest == path)
            .map(|i| previous.swap_remove(i));
        let Ok(text) = read(path, open) else { return };
        let masked = crate::signed::read(&text).masked;
        let source = Source {
            path: path.to_string_lossy().into_owned(),
            text: masked,
        };
        let declaration = match nitride_prepare::language_declaration(&source) {
            Ok(Some(declaration)) => declaration,
            Ok(None) => return, // A signed grammar that declares no language.
            Err(_) => {
                // Unparsable mid-edit: keep the language it declared.
                self.languages.extend(old);
                return;
            }
        };
        let folder = path.parent().unwrap_or(Path::new(""));
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let sidecar = folder.join(format!("{stem}.ncatalog"));
        let catalogs: Vec<_> = sidecar.exists().then_some(sidecar).into_iter().collect();
        let entry = Entry {
            trivia: nitride_core::TriviaPolicy::Nitrogen,
            name: declaration.name.clone(),
            extensions: declaration.extensions.clone(),
            grammars: std::iter::once(path.to_owned())
                .chain(declaration.grammars.iter().map(|g| folder.join(g)))
                .collect(),
            start: declaration.start.clone().unwrap_or_default(),
            mode: if catalogs.is_empty() {
                Mode::Syntax
            } else {
                Mode::Typed
            },
            catalogs,
            bindings: vec![],
            start_span: None,
        };
        let at_name = |message: String| located(path, &source.text, declaration.name_span, message);
        let mut state = if declaration.start.is_none() {
            LanguageState {
                current: old.and_then(|o| o.current),
                errors: vec![at_name(format!(
                    "signed language {} needs a start rule",
                    entry.name
                ))],
                warnings: vec![],
                signed: true,
                inputs: 0,
                manifest: path.to_owned(),
                entry,
            }
        } else {
            prepare(entry, path, old, open)
        };
        state.signed = true;
        // There is no manifest to show project errors (start rule, catalog):
        // they belong to the declaration.
        for error in &mut state.errors {
            if error.project {
                *error = at_name(error.message.clone());
            }
        }
        self.languages.push(state);
    }

    /// Signed languages declared with `name`.
    pub fn signed(&self, name: &str) -> Vec<&LanguageState> {
        self.languages
            .iter()
            .filter(|l| l.signed && l.entry.name == name)
            .collect()
    }

    /// The language a signed grammar file declares.
    pub fn signed_at(&self, path: &Path) -> Option<&LanguageState> {
        self.languages
            .iter()
            .find(|l| l.signed && same_file(&l.manifest, path))
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
                    errors: error,
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
                // A `.nproj` points at its start rule; JSON manifests at their start.
                let span = state
                    .entry
                    .start_span
                    .filter(|_| error.message.contains("start rule"))
                    .unwrap_or([0, 0]);
                errors.push(ManifestError {
                    span,
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
    /// Languages whose bindings include `path`.
    pub fn using_binding<'a>(&'a self, path: &'a Path) -> impl Iterator<Item = &'a LanguageState> {
        self.languages
            .iter()
            .filter(move |l| l.entry.bindings.iter().any(|b| same_file(b, path)))
    }

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

/// A preparation error at `span` of a file whose text is `text`.
fn located(path: &Path, text: &str, span: [usize; 2], message: String) -> PrepareError {
    let prefix = &text[..span[0].min(text.len())];
    PrepareError {
        path: path.to_string_lossy().into_owned(),
        span,
        line: prefix.matches('\n').count() + 1,
        column: prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
        message,
        project: false,
        code: None,
    }
}

/// Directories the signed-grammar scan never enters.
const SKIPPED: [&str; 6] = ["bin", "obj", "node_modules", "target", "artifacts", "dist"];
/// Files the scan looks at, at most.
const MAX_SCANNED: usize = 20_000;
/// Directories the scan lists, at most.
const MAX_DIRECTORIES: usize = 5_000;
/// Larger files are not grammars.
const MAX_GRAMMAR_BYTES: u64 = 1024 * 1024;
/// A signature lies in the leading trivia: the start of a file is enough to find one.
const HEAD_BYTES: usize = 4096;

/// Signed grammars (files whose header has a grammar signature) in the
/// workspace folders, plus open documents that are.
pub fn discover(
    folders: &[PathBuf],
    open: &dyn Fn(&Path) -> Option<String>,
    open_paths: &[PathBuf],
) -> Vec<PathBuf> {
    let signed = |text: &str| crate::signed::declares_grammar(text);
    let mut found: Vec<PathBuf> = Vec::new();
    let mut scanned = 0;
    let mut pending: Vec<PathBuf> = folders.to_vec();
    let mut listed = 0;
    while let Some(directory) = pending.pop() {
        listed += 1;
        if listed > MAX_DIRECTORIES {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !name.starts_with('.') && !SKIPPED.contains(&name.as_str()) {
                    pending.push(entry.path());
                }
                continue;
            }
            if !kind.is_file() || scanned >= MAX_SCANNED {
                continue;
            }
            scanned += 1;
            if entry
                .metadata()
                .map_or(true, |m| m.len() > MAX_GRAMMAR_BYTES)
            {
                continue;
            }
            let path = entry.path();
            let head = match open(&path) {
                Some(text) => text,
                None => {
                    use std::io::Read;
                    let mut head = Vec::with_capacity(HEAD_BYTES);
                    let read = std::fs::File::open(&path)
                        .and_then(|f| f.take(HEAD_BYTES as u64).read_to_end(&mut head));
                    if read.is_err() {
                        continue;
                    }
                    String::from_utf8_lossy(&head).into_owned()
                }
            };
            if signed(&head) {
                found.push(path);
            }
        }
    }
    for path in open_paths {
        let inside = folders.iter().any(|f| path.starts_with(f));
        if inside
            && !found.iter().any(|f| same_file(f, path))
            && open(path).is_some_and(|t| signed(&t))
        {
            found.push(path.clone());
        }
    }
    found.sort();
    found
}

/// Declared inputs and authoring file kinds. Data JSON is an input only when
/// registered as a manifest/grammar/catalog/binding, not merely by its suffix.
pub(crate) fn language_input_path(registry: &Registry, path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if manifest::NAMES.contains(&name.as_str())
        || name.ends_with(".ngr")
        || name.ends_with(".nproj")
        || name.ends_with(".ncatalog")
        || name.ends_with(".nbind")
    {
        return true;
    }
    let key = input_identity(path);
    let matches_input = |input: &PathBuf| {
        if input == path {
            return true;
        }
        let input = input_identity(input);
        if cfg!(windows) {
            input
                .to_string_lossy()
                .eq_ignore_ascii_case(&key.to_string_lossy())
        } else {
            input == key
        }
    };
    registry
        .input_references
        .values()
        .flatten()
        .any(matches_input)
        || registry.languages.iter().any(|l| {
            std::iter::once(&l.manifest)
                .chain(&l.entry.grammars)
                .chain(&l.entry.catalogs)
                .chain(&l.entry.bindings)
                .any(matches_input)
        })
}

/// Resolve through the nearest existing ancestor so deleting a file or folder
/// does not lose the identity of a registered input expressed with `..`.
fn input_identity(path: &Path) -> PathBuf {
    use std::path::Component;
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let parts: Vec<_> = absolute.components().collect();
    for end in (1..=parts.len()).rev() {
        let prefix: PathBuf = parts[..end].iter().collect();
        if let Ok(mut resolved) = prefix.canonicalize() {
            for part in &parts[end..] {
                match part {
                    Component::ParentDir => {
                        resolved.pop();
                    }
                    Component::CurDir => {}
                    _ => resolved.push(part.as_os_str()),
                }
            }
            return resolved;
        }
    }
    absolute
}

/// Whether an LSP create/change/delete can affect a language, including a file
/// whose head newly declares a signed grammar.
pub fn watched_change_matters(registry: &Registry, path: &Path, kind: u64) -> bool {
    if language_input_path(registry, path) {
        return true;
    }
    if kind == 3 {
        // Gone: only a signed grammar's usual name says it may have mattered.
        return path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".syntax"));
    }
    use std::io::Read;
    let mut head = Vec::with_capacity(HEAD_BYTES);
    let read =
        std::fs::File::open(path).and_then(|f| f.take(HEAD_BYTES as u64).read_to_end(&mut head));
    read.is_ok() && crate::signed::declares_grammar(&String::from_utf8_lossy(&head))
}

fn read(path: &Path, open: &dyn Fn(&Path) -> Option<String>) -> Result<String, String> {
    open(path)
        .map(Ok)
        .unwrap_or_else(|| std::fs::read_to_string(path))
        .map_err(|e| format!("cannot read {}: {e}", path.display()))
}

pub(crate) fn prepare(
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
        signed: false,
        inputs: 0,
    };
    let manifest_error = |message: String| PrepareError {
        path: manifest.to_string_lossy().into_owned(),
        span: [0, 0],
        line: 1,
        column: 1,
        message,
        project: true,
        code: None,
    };
    let mut sources = Vec::new();
    for grammar in &state.entry.grammars {
        match read(grammar, open) {
            Ok(text) => sources.push(Source {
                path: grammar.to_string_lossy().into_owned(),
                // A signed grammar's signature is trivia to preparation.
                text: crate::signed::grammar_source(&text),
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
    // A binding file that cannot be read is reported on the manifest; values
    // just go without its operations.
    let bindings: Vec<Source> = state
        .entry
        .bindings
        .iter()
        .filter_map(|path| {
            Some(Source {
                path: path.to_string_lossy().into_owned(),
                text: read(path, open).ok()?,
            })
        })
        .collect();
    let mut hasher = DefaultHasher::new();
    for source in sources.iter().chain(&catalogs).chain(&bindings) {
        source.text.hash(&mut hasher);
    }
    (
        &state.entry.start,
        state.entry.mode == Mode::Typed,
        state.entry.trivia,
    )
        .hash(&mut hasher);
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
        Mode::Typed => catalog.prepare_all(&sources, &state.entry.start, "typed"),
        Mode::Syntax => catalog.prepare_tolerant_all(&sources, &state.entry.start),
    };
    match prepared {
        Ok(mut prepared) => {
            nitride_prepare::apply_trivia(&mut prepared.data, state.entry.trivia);
            match LoadedLanguage::load(&prepared.data.to_string()) {
                Ok(language) => {
                    state.warnings = prepared.warnings.clone();
                    let declares =
                        |rule: &str| prepared.declarations.iter().any(|d| d.rule == rule);
                    let roles = Arc::new(crate::roles::roles(language.language(), &declares));
                    let components = components(&bindings, &catalog.value);
                    state.current = Some(Arc::new(Loaded {
                        language: Arc::new(language),
                        prepared,
                        components,
                        roles,
                    }));
                }
                Err(message) => state.errors.push(manifest_error(message)),
            }
        }
        Err(errors) => state.errors.extend(errors),
    }
    state
}

/// The component bindings of `.nbind` sources, each with its component's
/// path (written relative to its `.nbind`). A file whose bindings do not check
/// adds none; its problems are its own diagnostics.
fn components(
    sources: &[Source],
    catalog: &serde_json::Value,
) -> Vec<(serde_json::Value, PathBuf)> {
    let mut found = Vec::new();
    for source in sources {
        let Ok(bindings) = nitride_prepare::bindings(std::slice::from_ref(source), catalog) else {
            continue;
        };
        let folder = Path::new(&source.path).parent().unwrap_or(Path::new(""));
        for binding in bindings.as_array().into_iter().flatten() {
            if let Some(component) = binding["component"].as_str() {
                found.push((binding.clone(), folder.join(component)));
            }
        }
    }
    found
}

#[cfg(test)]
mod input_classification_tests {
    use super::*;
    #[test]
    fn data_json_is_not_an_input_but_registered_json_metadata_is() {
        let mut registry = Registry::default();
        assert!(!watched_change_matters(
            &registry,
            Path::new("/virtual/data.json"),
            2
        ));
        let entry = Entry {
            name: "Test".into(),
            extensions: vec![".test".into()],
            grammars: vec!["/virtual/grammar.json".into()],
            start: "Test.P".into(),
            catalogs: vec!["/virtual/sub/../types.json".into()],
            bindings: vec!["/virtual/host.json".into()],
            mode: Mode::Typed,
            trivia: nitride_core::TriviaPolicy::Nitrogen,
            start_span: None,
        };
        registry.languages.push(LanguageState {
            entry,
            manifest: "/virtual/project.json".into(),
            current: None,
            errors: vec![],
            warnings: vec![],
            signed: false,
            inputs: 0,
        });
        for path in [
            "/virtual/grammar.json",
            "/virtual/types.json",
            "/virtual/host.json",
            "/virtual/project.json",
            "/virtual/nitrogen.json",
        ] {
            assert!(
                watched_change_matters(&registry, Path::new(path), 2),
                "{path}"
            );
            assert!(
                watched_change_matters(&registry, Path::new(path), 3),
                "deleted {path}"
            );
        }
        assert!(!watched_change_matters(
            &registry,
            Path::new("/virtual/data.json"),
            2
        ));
        if cfg!(windows) {
            assert!(watched_change_matters(
                &registry,
                Path::new("/VIRTUAL/TYPES.JSON"),
                3
            ));
        }
    }
}

#[cfg(test)]
mod missing_input_tests {
    use super::*;
    #[test]
    fn cold_missing_json_input_is_tracked_before_language_resolution() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nitride-missing-input-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("Lang")).unwrap();
        let workspace = root.join("nitride.nproj");
        let project = root.join("Lang/Lang.nproj");
        let catalog = root.join("types.json");
        let source = "language Test { extensions .test; grammar ../Test.ngr; catalog ../types.json; start Test.P; }";
        std::fs::write(&workspace, "workspace { include Lang; }").unwrap();
        std::fs::write(&project, source).unwrap();
        std::fs::write(
            root.join("Test.ngr"),
            "syntax module Test { syntax P = \"ok\"; }",
        )
        .unwrap();
        let mut registry = Registry::default();
        registry.reload(std::slice::from_ref(&root), &|_| None);
        assert!(
            registry.languages().is_empty(),
            "missing catalog prevents a LanguageState"
        );
        assert!(
            watched_change_matters(&registry, &catalog, 1),
            "creation must recover a cold missing input"
        );
        std::fs::write(&project, "language Test {").unwrap();
        registry.reload(std::slice::from_ref(&root), &|_| None);
        assert!(
            watched_change_matters(&registry, &catalog, 1),
            "keep references while project is malformed"
        );
        std::fs::write(&project, source).unwrap();
        std::fs::write(&workspace, "workspace {").unwrap();
        registry.reload(std::slice::from_ref(&root), &|_| None);
        assert!(
            watched_change_matters(&registry, &catalog, 1),
            "keep included project references while workspace is malformed"
        );
        std::fs::write(&workspace, "workspace { include Lang; }").unwrap();
        std::fs::write(&catalog, "catalog module Test { }").unwrap();
        assert!(watched_change_matters(&registry, &catalog, 1));
        registry.reload(std::slice::from_ref(&root), &|_| None);
        assert!(
            registry
                .for_path(&root.join("x.test"))
                .unwrap()
                .current
                .is_some()
        );
        std::fs::write(&workspace, "workspace {}").unwrap();
        registry.reload(std::slice::from_ref(&root), &|_| None);
        assert!(
            !watched_change_matters(&registry, &catalog, 2),
            "removed project must not keep stale JSON references"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
