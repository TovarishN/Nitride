use crate::{SourceText, syntax::text::Reader};
#[derive(Clone, Copy)]
pub(super) enum Source<'a> {
    Flat(&'a str),
    Shared(&'a SourceText),
}
impl<'a> Source<'a> {
    pub(super) fn flat(self) -> &'a str {
        match self {
            Self::Flat(s) => s,
            Self::Shared(_) => unreachable!("flat compatibility path"),
        }
    }
    pub(super) fn len(self) -> usize {
        match self {
            Self::Flat(s) => s.len(),
            Self::Shared(s) => s.len() as usize,
        }
    }
    pub(super) fn character(self, at: usize) -> Option<char> {
        match self {
            Self::Flat(s) => s.get(at..)?.chars().next(),
            Self::Shared(s) => Reader::new(s).character(at),
        }
    }
    pub(super) fn matches(self, at: usize, text: &str) -> bool {
        match self {
            Self::Flat(s) => s.get(at..).is_some_and(|s| s.starts_with(text)),
            Self::Shared(s) => Reader::new(s).matches(at, text),
        }
    }
    pub(super) fn find(self, at: usize, text: &str) -> Option<usize> {
        match self {
            Self::Flat(s) => s.get(at..)?.find(text).map(|n| at + n),
            Self::Shared(s) => Reader::new(s).find(at, text),
        }
    }
    pub(super) fn all_whitespace(
        self,
        start: usize,
        end: usize,
        mut step: impl FnMut() -> Result<(), crate::Diagnostic>,
    ) -> Result<bool, crate::Diagnostic> {
        match self {
            Self::Flat(s) => {
                for c in s[start..end].chars() {
                    step()?;
                    if !c.is_whitespace() {
                        return Ok(false);
                    }
                }
            }
            Self::Shared(s) => {
                let mut reader = Reader::new(s);
                let mut at = start;
                while at < end {
                    step()?;
                    let c = reader.character(at).expect("VM source boundary");
                    if !c.is_whitespace() {
                        return Ok(false);
                    }
                    at += c.len_utf8();
                }
            }
        }
        Ok(true)
    }
}
