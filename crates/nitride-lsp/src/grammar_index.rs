//! Rules across the grammar files of a language: where each is defined and
//! every place a grammar uses it.
use nitride_prepare::{GrammarItem, GrammarReference, Source};
use std::path::{Path, PathBuf};

pub struct GrammarFile {
    pub path: PathBuf,
    pub text: String,
    pub outline: Vec<GrammarItem>,
    pub references: Vec<GrammarReference>,
}

pub struct GrammarIndex {
    pub files: Vec<GrammarFile>,
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

/// Indexes `paths`, editor buffers first; unreadable or unparsable files are left out.
pub fn build(paths: &[PathBuf], open: &dyn Fn(&Path) -> Option<String>) -> GrammarIndex {
    let mut files = Vec::new();
    for path in paths {
        let Some(text) = open(path).or_else(|| std::fs::read_to_string(path).ok()) else {
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
            continue;
        };
        files.push(GrammarFile {
            path: path.clone(),
            text: source.text,
            outline,
            references,
        });
    }
    let definitions = definitions(&files);
    GrammarIndex { files, definitions }
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
