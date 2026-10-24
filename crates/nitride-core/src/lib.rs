pub mod execution;
pub mod grammar;
pub mod parser;
pub mod semantic;
pub mod syntax;
pub use execution::*;
pub use grammar::*;
pub use parser::parse;
pub use semantic::*;
pub use syntax::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub category: &'static str,
    pub span: [u32; 2],
    pub message: String,
}

impl Diagnostic {
    pub fn new(category: &'static str, span: [u32; 2], message: impl Into<String>) -> Self {
        Self {
            category,
            span,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub depth: u32,
    pub work: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            depth: 128,
            work: 10_000_000,
        }
    }
}

mod templates;
