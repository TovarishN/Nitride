//! Rules across the grammar files of a language, and the members of its
//! catalog files: where each is defined and every place a file uses it.
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

/// A module or rule definition: its file, module and outline item (a
/// module's item has no children).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleDef {
    pub file: usize,
    pub module: String,
    pub item: GrammarItem,
}

/// Rule kinds that define a name (an `extend` item names its target instead).
pub const DEFINING: [&str; 6] = [
    "syntax",
    "token",
    "extensible",
    "function",
    "helper",
    "alternative",
];

/// Catalog member kinds that define a name.
const CATALOG: [&str; 4] = ["type", "operation", "overload", "builtin"];

/// Whether `path` is a catalog file rather than a grammar.
pub fn is_catalog(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "ncatalog")
}

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
            text: crate::signed::grammar_source(&text),
        };
        let catalog = is_catalog(path);
        let (outline, references) = if catalog {
            (
                nitride_prepare::catalog_outline(&source),
                nitride_prepare::catalog_references(&source),
            )
        } else {
            (
                nitride_prepare::grammar_outline(&source),
                nitride_prepare::grammar_references(&source),
            )
        };
        let (Ok(outline), Ok(references)) = (outline, references) else {
            unreadable.push(path.clone());
            continue;
        };
        files.push(GrammarFile {
            uri: uri_of(path),
            damaged: !if catalog {
                nitride_prepare::catalog_parses(&source)
            } else {
                nitride_prepare::grammar_parses(&source)
            },
            locals: if catalog {
                vec![]
            } else {
                nitride_prepare::grammar_locals(&source).unwrap_or_default()
            },
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

/// Every module and rule definition in `files`, with its module.
fn definitions(files: &[GrammarFile]) -> Vec<RuleDef> {
    let mut found = Vec::new();
    for (file, grammar) in files.iter().enumerate() {
        for module in &grammar.outline {
            found.push(RuleDef {
                file,
                module: module.name.clone(),
                item: GrammarItem {
                    children: vec![],
                    ..module.clone()
                },
            });
            for member in &module.children {
                let items = std::iter::once(member).chain(&member.children);
                for item in
                    items.filter(|i| DEFINING.contains(&i.kind) || CATALOG.contains(&i.kind))
                {
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

    /// Resolves a rule name used in `module`, as `resolve` does.
    pub fn definition(&self, name: &str, module: &str) -> Option<RuleDef> {
        self.resolve(&GrammarReference {
            kind: "rule",
            name: name.into(),
            span: [0, 0],
            module: module.into(),
        })
    }

    /// Resolves a name used in `module`. A grammar or catalog module name
    /// names the first module of that kind so called; a built-in, the catalog
    /// built-in so called. For a rule (or catalog member), `M.R` names `R`
    /// of module `M`; a bare `R` is the module's own, else the only `R`
    /// anywhere. A field `Rule.F` is the label `F` in that rule.
    fn resolve(&self, used: &GrammarReference) -> Option<RuleDef> {
        let (name, module) = (used.name.as_str(), used.module.as_str());
        let named = |kind: &str| {
            self.definitions
                .iter()
                .find(|d| d.item.kind == kind && d.item.name == name)
                .cloned()
        };
        let kinds: &[&str] = match used.kind {
            "module" => return named("module"),
            "catalog" => return named("catalog"),
            "builtin" => return named("builtin"),
            "field" => return self.field(name, module),
            "member" => &CATALOG[..3],
            _ => &DEFINING,
        };
        let definitions = self
            .definitions
            .iter()
            .filter(|d| kinds.contains(&d.item.kind));
        if let Some((qualifier, rule)) = name.rsplit_once('.') {
            return definitions
                .clone()
                .find(|d| d.module == qualifier && d.item.name == rule)
                .cloned();
        }
        if let Some(own) = definitions
            .clone()
            .find(|d| d.module == module && d.item.name == name)
        {
            return Some(own.clone());
        }
        let mut named = definitions.filter(|d| d.item.name == name);
        let only = named.next()?;
        named.next().is_none().then(|| only.clone())
    }

    /// The label definition `Rule.F` names, as a "field" definition.
    fn field(&self, name: &str, module: &str) -> Option<RuleDef> {
        let (rule, field) = name.rsplit_once('.')?;
        let rule = self.resolve(&GrammarReference {
            kind: "rule",
            name: rule.into(),
            span: [0, 0],
            module: module.into(),
        })?;
        // A rule's labels lie before the next definition in its file (an
        // alternative's item spans only its name).
        let end = self
            .definitions
            .iter()
            .filter(|d| d.file == rule.file && d.item.span[0] > rule.item.span[0])
            .map(|d| d.item.span[0])
            .min()
            .unwrap_or(usize::MAX)
            .max(rule.item.span[1]);
        let inside = |s: &[usize; 2]| rule.item.span[0] <= s[0] && s[1] <= end;
        let span = *self.files[rule.file]
            .locals
            .iter()
            .filter(|l| l.kind == "field" && l.name == field)
            .flat_map(|l| &l.definitions)
            .find(|s| inside(s))?;
        Some(RuleDef {
            file: rule.file,
            module: rule.module,
            item: GrammarItem {
                kind: "field",
                name: field.into(),
                span,
                name_span: span,
                children: vec![],
            },
        })
    }

    /// The innermost name used at `offset` of `file` (span ends included): a
    /// qualifier's module rather than the qualified rule.
    pub fn reference_at(&self, file: usize, offset: u32) -> Option<&GrammarReference> {
        let offset = offset as usize;
        self.files
            .get(file)?
            .references
            .iter()
            .filter(|r| r.span[0] <= offset && offset <= r.span[1])
            .min_by_key(|r| r.span[1] - r.span[0])
    }

    /// The module or rule defined or used at `offset` of `file` (span ends included).
    pub fn at(&self, file: usize, offset: u32) -> Option<RuleDef> {
        let inside = |span: [usize; 2]| span[0] <= offset as usize && offset as usize <= span[1];
        if let Some(def) = self
            .definitions
            .iter()
            .find(|d| d.file == file && inside(d.item.name_span))
        {
            return Some(def.clone());
        }
        self.resolve(self.reference_at(file, offset)?)
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

    /// Every use of a module or rule, in every file, as (file, span).
    pub fn uses(&self, rule: &RuleDef) -> Vec<(usize, [u32; 2])> {
        let mut found = Vec::new();
        for (file, grammar) in self.files.iter().enumerate() {
            for used in &grammar.references {
                if self.resolve(used).as_ref() == Some(rule) {
                    found.push((file, used.span.map(|b| b as u32)));
                }
            }
        }
        found
    }
}
