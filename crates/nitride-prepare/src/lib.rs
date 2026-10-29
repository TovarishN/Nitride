//! Build-time preparation of the explicitly admitted Nitrogen grammar subset.
#[rustfmt::skip]
mod grammar_seed;
#[rustfmt::skip]
mod catalog_seed;
mod catalog;
mod functions;
mod literal;
mod normalize;
mod parser;
mod project;
pub use project::{PreparedProject, prepare_project, write_prepared};

use serde_json::Value;
use std::fmt;

#[derive(Clone, Debug)]
pub struct Source {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PrepareError {
    pub path: String,
    pub span: [usize; 2],
    pub line: usize,
    pub column: usize,
    pub message: String,
    /// The error concerns the project inputs (start rule, catalog, mode or
    /// profile) rather than a place in a grammar; `path` is then only the
    /// first grammar, or the manifest that failed to read.
    pub project: bool,
}

impl Source {
    fn error(&self, span: [usize; 2], message: impl Into<String>) -> PrepareError {
        let prefix = &self.text[..span[0]];
        PrepareError {
            path: self.path.clone(),
            span,
            line: prefix.bytes().filter(|b| *b == b'\n').count() + 1,
            column: prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
            message: message.into(),
            project: false,
        }
    }
    /// An error about the project inputs, reported against this source.
    fn project_error(&self, message: impl Into<String>) -> PrepareError {
        PrepareError {
            project: true,
            ..self.error([0, 0], message)
        }
    }
}

impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{} [{}..{}]: {}",
            self.path, self.line, self.column, self.span[0], self.span[1], self.message
        )
    }
}
impl std::error::Error for PrepareError {}

pub fn prepare_sources(
    sources: &[Source],
    catalog: &str,
    start: &str,
    mode: &str,
) -> Result<Value, PrepareError> {
    normalize::prepare(sources, catalog, start, mode, false, &|_| Ok(())).map(|p| p.data)
}

/// A prepared language with what tolerant preparation skipped and the
/// grammar's `declares` clauses, which syntax-only output omits.
#[derive(Debug)]
pub struct Prepared {
    pub data: Value,
    pub warnings: Vec<PrepareError>,
    pub declarations: Vec<Declaration>,
    pub references: Vec<ReferenceClause>,
}

/// One `declares` clause: the qualified rule, its name field and symbol kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub rule: String,
    pub field: String,
    pub kinds: Vec<String>,
}

/// Strict preparation that also returns the grammar's declarations.
pub fn prepare_sources_detailed(
    sources: &[Source],
    catalog: &str,
    start: &str,
    mode: &str,
) -> Result<Prepared, PrepareError> {
    normalize::prepare(sources, catalog, start, mode, false, &|_| Ok(()))
}

/// One `references` clause: the qualified rule, its name field and symbol kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceClause {
    pub rule: String,
    pub field: String,
    pub kinds: Vec<String>,
}

/// A rule or module name used in a grammar file, and the module it is used in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarReference {
    /// "rule" or "module".
    pub kind: &'static str,
    pub name: String,
    pub span: [usize; 2],
    pub module: String,
}

/// A name bound inside one member of a grammar file: a helper parameter, a
/// match capture or an element label, with every use within its scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarLocal {
    /// "parameter", "capture" or "field".
    pub kind: &'static str,
    pub name: String,
    /// The binder as written, e.g. `Value: Expr`.
    pub detail: String,
    /// Where the name is bound; a label may bind it more than once.
    pub definitions: Vec<[usize; 2]>,
    /// `$Name` in quotes (the name only), match selectors and clause fields.
    pub uses: Vec<[usize; 2]>,
}

/// The local names of a grammar file and their uses.
pub fn grammar_locals(source: &Source) -> Result<Vec<GrammarLocal>, PrepareError> {
    parser::locals(source)
}

/// Whether a grammar file parses as written. The outline and references of a
/// file that does not are taken from its undamaged lines.
pub fn grammar_parses(source: &Source) -> bool {
    parser::parses(source)
}

/// Every rule and module name a grammar file uses: element references,
/// `extend` targets and function calls, `using` and rule qualifiers.
pub fn grammar_references(source: &Source) -> Result<Vec<GrammarReference>, PrepareError> {
    parser::references(source)
}

/// Syntax-mode preparation for editors: Nitrogen's C# semantic members are
/// skipped, each reported as a warning, and the parser is still built.
pub fn prepare_sources_tolerant(
    sources: &[Source],
    catalog: &str,
    start: &str,
) -> Result<Prepared, PrepareError> {
    normalize::prepare(sources, catalog, start, "syntax", true, &|_| Ok(()))
}

/// The compiled grammar frontend, a syntax-only language for `.ngr` files.
pub fn grammar_language() -> &'static nitride_core::Language<'static> {
    &grammar_seed::LANGUAGE
}

/// The catalog JSON of a project's `.ncatalog` sources: their modules are
/// resolved against each other (`using`) and merged in source order. No
/// sources give the empty catalog of a syntax-only profile.
pub fn catalogs(sources: &[Source]) -> Result<Value, PrepareError> {
    catalog(sources).map(|c| c.value)
}

/// A project's lowered catalogs: the JSON value codegen reads, its compact
/// text (what `catalog_hash` hashes), and the grammar names it uses.
pub struct Catalog {
    pub value: Value,
    pub json: String,
    sources: Vec<Source>,
    uses: Vec<catalog::Use>,
}

/// Parses, resolves and merges a project's `.ncatalog` sources.
pub fn catalog(sources: &[Source]) -> Result<Catalog, PrepareError> {
    let (value, uses) = catalog::lower(sources)?;
    Ok(Catalog {
        json: value.to_string(),
        value,
        sources: sources.to_vec(),
        uses,
    })
}

impl Catalog {
    /// Strict preparation of `sources` with this catalog.
    pub fn prepare(
        &self,
        sources: &[Source],
        start: &str,
        mode: &str,
    ) -> Result<Prepared, PrepareError> {
        normalize::prepare(sources, &self.json, start, mode, false, &|d| self.check(d))
    }
    /// Tolerant syntax-mode preparation of `sources` with this catalog.
    pub fn prepare_tolerant(
        &self,
        sources: &[Source],
        start: &str,
    ) -> Result<Prepared, PrepareError> {
        normalize::prepare(sources, &self.json, start, "syntax", true, &|d| {
            self.check(d)
        })
    }
    /// Every grammar rule and field the catalog names exists in the prepared
    /// language `data`; an error points at the catalog.
    fn check(&self, data: &Value) -> Result<(), PrepareError> {
        fn labels<'a>(e: &'a Value, found: &mut Vec<&'a str>) {
            if let Some(label) = e["label"].as_str() {
                found.push(label);
            }
            match e {
                Value::Array(items) => items.iter().for_each(|i| labels(i, found)),
                Value::Object(map) => map.values().for_each(|v| labels(v, found)),
                _ => {}
            }
        }
        let rules = data["rules"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        for used in &self.uses {
            let source = &self.sources[used.source];
            let Some(rule) = rules.iter().find(|r| r["name"] == used.rule.as_str()) else {
                return Err(
                    source.error(used.span, format!("unknown grammar rule '{}'", used.rule))
                );
            };
            if let Some(field) = &used.field {
                let mut found = Vec::new();
                labels(&rule["expr"], &mut found);
                if !found.contains(&field.as_str()) {
                    return Err(
                        source.error(used.span, format!("'{}' has no field '{field}'", used.rule))
                    );
                }
            }
        }
        Ok(())
    }
}

/// The compiled catalog frontend, a syntax-only language for `.ncatalog` files.
pub fn catalog_language() -> &'static nitride_core::Language<'static> {
    &catalog_seed::LANGUAGE
}

/// A declaration in a grammar file's outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarItem {
    /// "module", "syntax", "token", "extensible", "extend", "alternative",
    /// "function" or "helper".
    pub kind: &'static str,
    pub name: String,
    pub span: [usize; 2],
    pub name_span: [usize; 2],
    pub children: Vec<GrammarItem>,
}

/// The modules of one grammar file and the members they declare.
pub fn grammar_outline(source: &Source) -> Result<Vec<GrammarItem>, PrepareError> {
    parser::outline(source)
}
