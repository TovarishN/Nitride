use crate::{SourceText, syntax::text::Reader};
#[derive(Clone, Copy)]
pub(super) enum Source<'a> {
    Flat(&'a str),
    /// The text, with its contiguous form when the caller still has one.
    Shared(&'a SourceText, Option<&'a str>),
}
fn reader<'a>(source: &'a SourceText, flat: Option<&'a str>) -> Reader<'a> {
    flat.map_or_else(
        || Reader::new(source),
        |text| Reader::contiguous(source, text),
    )
}
impl<'a> Source<'a> {
    pub(super) fn flat(self) -> &'a str {
        match self {
            Self::Flat(s) => s,
            Self::Shared(..) => unreachable!("flat compatibility path"),
        }
    }
    pub(super) fn len(self) -> usize {
        match self {
            Self::Flat(s) => s.len(),
            Self::Shared(s, _) => s.len() as usize,
        }
    }
    pub(super) fn character(self, at: usize) -> Option<char> {
        match self {
            Self::Flat(s) => s.get(at..)?.chars().next(),
            Self::Shared(s, flat) => reader(s, flat).character(at),
        }
    }
    pub(super) fn matches(self, at: usize, text: &str) -> bool {
        match self {
            Self::Flat(s) => s.get(at..).is_some_and(|s| s.starts_with(text)),
            Self::Shared(s, flat) => reader(s, flat).matches(at, text),
        }
    }
    pub(super) fn find(self, at: usize, text: &str) -> Option<usize> {
        match self {
            Self::Flat(s) => s.get(at..)?.find(text).map(|n| at + n),
            Self::Shared(s, flat) => reader(s, flat).find(at, text),
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
            Self::Shared(s, flat) => {
                let mut reader = reader(s, flat);
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
    /// A reader for a run of nearby queries, keeping its current piece.
    pub(super) fn cursor(self) -> Cursor<'a> {
        match self {
            Self::Flat(s) => Cursor::Flat(s),
            Self::Shared(s, flat) => Cursor::Shared(reader(s, flat)),
        }
    }
    /// The end of whitespace and comments from `at`; an unterminated block
    /// comment is not trivia.
    pub(super) fn trivia_end(self, mut at: usize) -> usize {
        let mut cursor = self.cursor();
        loop {
            if let Some(c) = cursor.character(at).filter(|c| c.is_whitespace()) {
                at += c.len_utf8();
            } else if cursor.matches(at, "//") {
                at = cursor.find(at, "\n").unwrap_or(self.len());
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
}

pub(super) enum Cursor<'a> {
    Flat(&'a str),
    Shared(Reader<'a>),
}
impl Cursor<'_> {
    pub(super) fn ascii_whitespace_end(&mut self, at: usize, limit: usize) -> usize {
        match self {
            Self::Flat(s) => {
                let bytes = &s.as_bytes()[at.min(s.len())..limit.min(s.len())];
                at + bytes
                    .iter()
                    .take_while(|&&b| matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r'))
                    .count()
            }
            Self::Shared(r) => r.ascii_whitespace_end(at, limit),
        }
    }
    pub(super) fn character(&mut self, at: usize) -> Option<char> {
        match self {
            Self::Flat(s) => Source::Flat(s).character(at),
            Self::Shared(r) => r.character(at),
        }
    }
    pub(super) fn matches(&mut self, at: usize, text: &str) -> bool {
        match self {
            Self::Flat(s) => Source::Flat(s).matches(at, text),
            Self::Shared(r) => r.matches(at, text),
        }
    }
    pub(super) fn find(&mut self, at: usize, text: &str) -> Option<usize> {
        match self {
            Self::Flat(s) => Source::Flat(s).find(at, text),
            Self::Shared(r) => r.find(at, text),
        }
    }
}
