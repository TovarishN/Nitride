//! Build-time preparation of the explicitly admitted Nitrogen grammar subset.
#[rustfmt::skip]
mod grammar_seed;
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

#[derive(Debug)]
pub struct PrepareError {
    pub path: String,
    pub span: [usize; 2],
    pub line: usize,
    pub column: usize,
    pub message: String,
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
    normalize::prepare(sources, catalog, start, mode)
}
