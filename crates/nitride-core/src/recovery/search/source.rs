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
    /// The end of whitespace and comments from `at`; an unterminated block
    /// comment is not trivia. One reader serves the whole run.
    pub(super) fn trivia_end(self, at: usize) -> usize {
        match self {
            Self::Flat(s) => skip_trivia(&mut Flat(s), at),
            Self::Shared(s) => skip_trivia(&mut Reader::new(s), at),
        }
    }
}

trait Cursor {
    fn len(&self) -> usize;
    fn character(&mut self, at: usize) -> Option<char>;
    fn matches(&mut self, at: usize, text: &str) -> bool;
    fn find(&mut self, at: usize, text: &str) -> Option<usize>;
}
struct Flat<'a>(&'a str);
impl Cursor for Flat<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn character(&mut self, at: usize) -> Option<char> {
        Source::Flat(self.0).character(at)
    }
    fn matches(&mut self, at: usize, text: &str) -> bool {
        Source::Flat(self.0).matches(at, text)
    }
    fn find(&mut self, at: usize, text: &str) -> Option<usize> {
        Source::Flat(self.0).find(at, text)
    }
}
impl Cursor for Reader<'_> {
    fn len(&self) -> usize {
        Reader::len(self)
    }
    fn character(&mut self, at: usize) -> Option<char> {
        Reader::character(self, at)
    }
    fn matches(&mut self, at: usize, text: &str) -> bool {
        Reader::matches(self, at, text)
    }
    fn find(&mut self, at: usize, text: &str) -> Option<usize> {
        Reader::find(self, at, text)
    }
}
fn skip_trivia(cursor: &mut impl Cursor, mut at: usize) -> usize {
    loop {
        if let Some(c) = cursor.character(at).filter(|c| c.is_whitespace()) {
            at += c.len_utf8();
        } else if cursor.matches(at, "//") {
            at = cursor.find(at, "\n").unwrap_or(cursor.len());
        } else if cursor.matches(at, "/*") {
            match cursor.find(at + 2, "*/") {
                Some(end) => at = end + 2,
                None => return at,
            }
        } else {
            return at;
        }
    }
}
