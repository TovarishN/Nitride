//! Rules across the grammar files of a language: where each is defined and
//! every place a grammar uses it.
use nitride_prepare::{GrammarItem, GrammarLocal, GrammarReference, Source};
use std::path::{Path, PathBuf};

pub struct GrammarFile {
    pub path: PathBuf,
    /// The URI to report: the open document's, else the normalized path's.
    pub uri: String,
    /// The file does not parse as written; its structure comes from its
    /// undamaged lines.
    pub damaged: bool,
    pub text: String,
    pub outline: Vec<GrammarItem>,
    pub references: Vec<GrammarReference>,
    /// Parameters, captures and labels, each with its uses in this file.
    pub locals: Vec<GrammarLocal>,
}

pub struct GrammarIndex {
    pub files: Vec<GrammarFile>,
    /// Files of the set that could not be read or parsed at all.
    pub unreadable: Vec<PathBuf>,
    definitions: Vec<RuleDef>,
}

/// A rule definition: its file, module and outline item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleDef {
    pub file: usize,
    pub module: String,
    pub item: GrammarItem,
}

/// Rule kinds that define a name (an `extend` item names its target instead).
const DEFINING: [&str; 6] = [
    "syntax",
    "token",
    "extensible",
    "function",
    "helper",
    "alternative",
];

/// A `file:` URI for a path with `.` and `..` resolved lexically.
pub fn default_uri(path: &Path) -> String {
    use std::path::Component;
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other),
        }
    }
    crate::uri::from_path(&normal)
}

/// Indexes `paths`, editor buffers first. `uri_of` names each file for the
/// client; files that cannot be read or parsed at all are listed in `unreadable`.
pub fn build(
    paths: &[PathBuf],
    open: &dyn Fn(&Path) -> Option<String>,
    uri_of: &dyn Fn(&Path) -> String,
) -> GrammarIndex {
    let mut files = Vec::new();
    let mut unreadable = Vec::new();
    for path in paths {
        let Some(text) = open(path).or_else(|| std::fs::read_to_string(path).ok()) else {
            unreadable.push(path.clone());
            continue;
        };
        let source = Source {
            path: path.to_string_lossy().into_owned(),
            text,
        };
        let (Ok(outline), Ok(references)) = (
            nitride_prepare::grammar_outline(&source),
            nitride_prepare::grammar_references(&source),
        ) else {
            unreadable.push(path.clone());
            continue;
        };
        files.push(GrammarFile {
            uri: uri_of(path),
            damaged: !nitride_prepare::grammar_parses(&source),
            locals: nitride_prepare::grammar_locals(&source).unwrap_or_default(),
            path: path.clone(),
            text: source.text,
            outline,
            references,
        });
    }
    let definitions = definitions(&files);
    GrammarIndex {
        files,
        unreadable,
        definitions,
    }
}

/// Every rule definition in `files`, with its module.
fn definitions(files: &[GrammarFile]) -> Vec<RuleDef> {
    let mut found = Vec::new();
    for (file, grammar) in files.iter().enumerate() {
        for module in &grammar.outline {
            for member in &module.children {
                let items = std::iter::once(member).chain(&member.children);
                for item in items.filter(|i| DEFINING.contains(&i.kind)) {
                    found.push(RuleDef {
                        file,
                        module: module.name.clone(),
                        item: item.clone(),
                    });
                }
            }
        }
    }
    found
}

impl GrammarIndex {
    pub fn file(&self, path: &Path) -> Option<usize> {
        self.files
            .iter()
            .position(|f| crate::registry::same_file(&f.path, path))
    }

    pub fn definitions(&self) -> &[RuleDef] {
        &self.definitions
    }

    /// Resolves a rule name used in `module`: `M.R` names rule `R` of module
    /// `M`; a bare `R` is the module's own, else the only `R` anywhere.
    pub fn definition(&self, name: &str, module: &str) -> Option<RuleDef> {
        self.resolve(name, module).cloned()
    }

    fn resolve(&self, name: &str, module: &str) -> Option<&RuleDef> {
        let definitions = &self.definitions;
        if let Some((qualifier, rule)) = name.rsplit_once('.') {
            return definitions
                .iter()
                .find(|d| d.module == qualifier && d.item.name == rule);
        }
        if let Some(own) = definitions
            .iter()
            .find(|d| d.module == module && d.item.name == name)
        {
            return Some(own);
        }
        let mut named = definitions.iter().filter(|d| d.item.name == name);
        let only = named.next()?;
        named.next().is_none().then_some(only)
    }

    /// The rule defined or used at `offset` of `file` (span ends included).
    pub fn at(&self, file: usize, offset: u32) -> Option<RuleDef> {
        let offset = offset as usize;
        let inside = |span: [usize; 2]| span[0] <= offset && offset <= span[1];
        if let Some(def) = self
            .definitions
            .iter()
            .find(|d| d.file == file && inside(d.item.name_span))
        {
            return Some(def.clone());
        }
        let used = self
            .files
            .get(file)?
            .references
            .iter()
            .find(|r| inside(r.span))?;
        self.definition(&used.name, &used.module)
    }

    /// The local defined or used at `offset` of `file` (span ends included,
    /// and a placeholder's `$`), and the span of the name there.
    pub fn local_at(&self, file: usize, offset: u32) -> Option<(&GrammarLocal, [u32; 2])> {
        let offset = offset as usize;
        let grammar = self.files.get(file)?;
        let dollar = grammar.text.as_bytes().get(offset) == Some(&b'$');
        let start = |s: &[usize; 2]| if dollar { s[0].saturating_sub(1) } else { s[0] };
        grammar.locals.iter().find_map(|local| {
            local
                .definitions
                .iter()
                .chain(&local.uses)
                .find(|s| start(s) <= offset && offset <= s[1])
                .map(|s| (local, s.map(|b| b as u32)))
        })
    }

    /// Every use of a rule, in every file, as (file, span).
    pub fn uses(&self, rule: &RuleDef) -> Vec<(usize, [u32; 2])> {
        let mut found = Vec::new();
        for (file, grammar) in self.files.iter().enumerate() {
            for used in &grammar.references {
                if self.resolve(&used.name, &used.module) == Some(rule) {
                    found.push((file, used.span.map(|b| b as u32)));
                }
            }
        }
        found
    }
}
