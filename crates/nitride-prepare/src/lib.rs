//! Build-time preparation of the explicitly admitted Nitrogen grammar subset.
#[rustfmt::skip]
mod grammar_seed;
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
    normalize::prepare(sources, catalog, start, mode, false).map(|p| p.data)
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
    normalize::prepare(sources, catalog, start, mode, false)
}

/// One `references` clause: the qualified rule, its name field and symbol kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceClause {
    pub rule: String,
    pub field: String,
    pub kinds: Vec<String>,
}

/// A rule name used in a grammar file, and the module it is used in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarReference {
    pub name: String,
    pub span: [usize; 2],
    pub module: String,
}

/// Whether a grammar file parses as written. The outline and references of a
/// file that does not are taken from its undamaged lines.
pub fn grammar_parses(source: &Source) -> bool {
    parser::parses(source)
}

/// Every rule name a grammar file uses: element references and `extend` targets.
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
    normalize::prepare(sources, catalog, start, "syntax", true)
}

/// The compiled grammar frontend, a syntax-only language for `.ngr` files.
pub fn grammar_language() -> &'static nitride_core::Language<'static> {
    &grammar_seed::LANGUAGE
}

/// A signed grammar's `language Name { ... }` declaration: the settings
/// Nitride uses (start rule, further grammars, file extensions). The C#-side
/// settings (namespace, sources, usings, tokens, types) are read and ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageDeclaration {
    pub name: String,
    pub name_span: [usize; 2],
    pub span: [usize; 2],
    pub start: Option<String>,
    pub grammars: Vec<String>,
    pub extensions: Vec<String>,
}

/// The language a grammar declares, if any; `source` has its signature masked.
pub fn language_declaration(source: &Source) -> Result<Option<LanguageDeclaration>, PrepareError> {
    parser::language_declaration(source)
}

/// A declaration in a grammar file's outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarItem {
    /// "language", "module", "syntax", "token", "extensible", "extend",
    /// "alternative", "function" or "helper".
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
