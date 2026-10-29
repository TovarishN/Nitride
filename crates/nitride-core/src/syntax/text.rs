//! UTF-8 source snapshots sharing bounded immutable buffers and balanced paths.
use super::sequence::{Measured, Sequence};
use crate::Diagnostic;
use std::{borrow::Cow, collections::HashMap, sync::Arc};

#[derive(Debug, Clone)]
pub struct TextEdit {
    pub range: [u32; 2],
    pub replacement: String,
}
#[derive(Clone)]
struct Piece {
    buffer: Arc<str>,
    start: u32,
    end: u32,
}
impl Measured for Piece {
    fn width(&self) -> u64 {
        u64::from(self.end - self.start)
    }
}
impl Piece {
    fn text(&self) -> &str {
        &self.buffer[self.start as usize..self.end as usize]
    }
}
#[derive(Clone, Default)]
pub struct SourceText {
    pieces: Sequence<Piece>,
}
const CHUNK: usize = 64 * 1024;
fn invalid() -> Diagnostic {
    Diagnostic::new("edit", [0, 0], "invalid UTF-8 byte range")
}
impl SourceText {
    pub fn new(source: &str) -> Result<Self, Diagnostic> {
        if source.len() > u32::MAX as usize {
            return Err(Diagnostic::new(
                "limit",
                [0, 0],
                "source exceeds 32-bit span capacity",
            ));
        }
        let mut pieces = Vec::new();
        let mut start = 0;
        while start < source.len() {
            let mut end = (start + CHUNK).min(source.len());
            while !source.is_char_boundary(end) {
                end -= 1;
            }
            pieces.push(Piece {
                buffer: Arc::from(&source[start..end]),
                start: 0,
                end: (end - start) as u32,
            });
            start = end;
        }
        Ok(Self {
            pieces: Sequence::from_vec(pieces),
        })
    }
    pub fn len(&self) -> u32 {
        self.pieces.width() as u32
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn boundary(&self, at: u32) -> bool {
        if at == self.len() {
            return true;
        }
        self.pieces
            .locate(u64::from(at))
            .is_some_and(|(_, p, n)| p.text().is_char_boundary(n as usize))
    }
    fn validate(&self, range: [u32; 2]) -> Result<(), Diagnostic> {
        if range[0] > range[1]
            || range[1] > self.len()
            || !self.boundary(range[0])
            || !self.boundary(range[1])
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(crate) fn valid_span(&self, span: [u32; 2]) -> bool {
        self.validate(span).is_ok()
    }
    pub fn slice(&self, range: [u32; 2]) -> Result<Cow<'_, str>, Diagnostic> {
        self.validate(range)?;
        let len = (range[1] - range[0]) as usize;
        if len == 0 {
            return Ok(Cow::Borrowed(""));
        }
        let (index, piece, offset) = self.pieces.locate(u64::from(range[0])).unwrap();
        let offset = offset as usize;
        if offset + len <= piece.text().len() {
            return Ok(Cow::Borrowed(&piece.text()[offset..offset + len]));
        }
        let mut text = String::with_capacity(len);
        let mut remaining = len;
        let mut skip = offset;
        for piece in self.pieces.iter_from(index) {
            let take = remaining.min(piece.text().len() - skip);
            text.push_str(&piece.text()[skip..skip + take]);
            remaining -= take;
            skip = 0;
            if remaining == 0 {
                break;
            }
        }
        Ok(Cow::Owned(text))
    }
    pub fn replace(&self, edit: &TextEdit) -> Result<Self, Diagnostic> {
        self.validate(edit.range)?;
        let length = u64::from(self.len()) - u64::from(edit.range[1] - edit.range[0])
            + edit.replacement.len() as u64;
        if length > u64::from(u32::MAX) {
            return Err(Diagnostic::new(
                "limit",
                edit.range,
                "source exceeds 32-bit span capacity",
            ));
        }
        let mut pieces = Vec::new();
        let start = self.pieces.locate(u64::from(edit.range[0]));
        let end = self.pieces.locate(u64::from(edit.range[1]));
        let first = start.map_or(self.pieces.len(), |(i, _, _)| i);
        if let Some((_, p, n)) = start {
            if n > 0 {
                pieces.push(Piece {
                    buffer: p.buffer.clone(),
                    start: p.start,
                    end: p.start + n as u32,
                });
            }
        }
        let inserted = Self::new(&edit.replacement)?;
        pieces.extend(inserted.pieces.iter().cloned());
        let last = match end {
            Some((i, p, n)) if n > 0 => {
                pieces.push(Piece {
                    buffer: p.buffer.clone(),
                    start: p.start + n as u32,
                    end: p.end,
                });
                i + 1
            }
            Some((i, _, _)) => i,
            None => self.pieces.len(),
        };
        Ok(Self {
            pieces: self.pieces.replace(first..last, pieces),
        })
    }
    pub fn piece_count(&self) -> usize {
        self.pieces.len()
    }
    pub(crate) fn visit_allocations(&self, visit: &mut super::memory::Visitor<'_>) {
        use super::memory::{self, Class};
        self.pieces.visit_allocations(Class::Source, visit);
        for piece in self.pieces.iter() {
            let (id, size) = memory::arc(&piece.buffer);
            visit(Class::Source, id, size);
        }
    }
    pub fn retained_buffer_bytes(&self) -> usize {
        let buffers: HashMap<_, _> = self
            .pieces
            .iter()
            .map(|p| (Arc::as_ptr(&p.buffer) as *const () as usize, p.buffer.len()))
            .collect();
        buffers.values().sum()
    }
    #[cfg(test)]
    fn shares_buffer_at(&self, other: &Self, at: u32) -> bool {
        match (
            self.pieces.locate(u64::from(at)),
            other.pieces.locate(u64::from(at)),
        ) {
            (Some((_, a, _)), Some((_, b, _))) => Arc::ptr_eq(&a.buffer, &b.buffer),
            _ => false,
        }
    }
}

/// Cached contiguous piece access keeps ordinary scanning independent of piece count.
pub(crate) struct Reader<'a> {
    source: &'a SourceText,
    cached: Option<(usize, &'a str)>,
    observed: u64,
    length: usize,
    contiguous: Option<&'a str>,
}
impl<'a> Reader<'a> {
    pub(crate) fn new(source: &'a SourceText) -> Self {
        Self {
            source,
            cached: None,
            observed: 0,
            length: source.len() as usize,
            contiguous: None,
        }
    }
    pub(crate) fn contiguous(source: &'a SourceText, text: &'a str) -> Self {
        let mut reader = Self::new(source);
        debug_assert_eq!(reader.length, text.len());
        reader.contiguous = Some(text);
        reader
    }
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.length
    }
    pub(crate) fn contiguous_text(&self) -> Option<&'a str> {
        self.contiguous
    }
    pub(crate) fn observed(&self) -> u64 {
        self.observed
    }
    pub(crate) fn replace_observed(&mut self, value: u64) -> u64 {
        std::mem::replace(&mut self.observed, value)
    }
    pub(crate) fn observe(&mut self, end: u64) {
        if end > self.observed {
            self.observed = end;
        }
    }
    #[inline]
    fn window(&mut self, at: usize) -> Option<&'a str> {
        if let Some(text) = self.contiguous {
            return text.get(at..);
        }
        if let Some((start, text)) = self.cached {
            if at >= start && at < start + text.len() {
                return text.get(at - start..);
            }
        }
        let (_, piece, offset) = self.source.pieces.locate(at as u64)?;
        let start = at - offset as usize;
        let text = piece.text();
        self.cached = Some((start, text));
        text.get(offset as usize..)
    }
    pub(crate) fn character(&mut self, at: usize) -> Option<char> {
        let value = self.window(at).and_then(|s| s.chars().next());
        self.observe(
            (at.saturating_add(value.map_or(1, char::len_utf8))).min(self.len() + 1) as u64,
        );
        value
    }
    pub(crate) fn matches(&mut self, at: usize, text: &str) -> bool {
        self.observe(at.saturating_add(text.len()).min(self.len() + 1) as u64);
        if at > self.len() || text.len() > self.len() - at {
            return false;
        }
        if text.is_empty() {
            return true;
        }
        if let Some(rest) = self.window(at) {
            if rest.len() >= text.len() {
                return rest.starts_with(text);
            }
        }
        self.source
            .slice([at as u32, (at + text.len()) as u32])
            .is_ok_and(|s| s == text)
    }
    pub(crate) fn find(&mut self, at: usize, needle: &str) -> Option<usize> {
        debug_assert!(needle == "\n" || needle == "*/");
        if let Some(text) = self.contiguous {
            let result = text.get(at..).and_then(|s| s.find(needle)).map(|n| at + n);
            self.observe(result.map_or(self.length + 1, |n| n + needle.len()) as u64);
            return result;
        }
        let Some((index, _, offset)) = self.source.pieces.locate(at as u64) else {
            self.observe(self.len() as u64 + 1);
            return None;
        };
        let mut skip = offset as usize;
        let mut position = at;
        let mut previous_star = false;
        for piece in self.source.pieces.iter_from(index) {
            let text = &piece.text()[skip..];
            skip = 0;
            if needle == "*/" && previous_star && text.starts_with('/') {
                self.observe(position as u64 + 1);
                return Some(position - 1);
            }
            if let Some(found) = text.find(needle) {
                self.observe((position + found + needle.len()) as u64);
                return Some(position + found);
            }
            previous_star = text.ends_with('*');
            position += text.len();
        }
        self.observe(self.len() as u64 + 1);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edit_utf8_and_cross_piece_tokens() {
        let source = SourceText::new("a😀bc").unwrap();
        assert!(
            source
                .replace(&TextEdit {
                    range: [2, 3],
                    replacement: "x".into()
                })
                .is_err()
        );
        let edited = source
            .replace(&TextEdit {
                range: [5, 5],
                replacement: "XYZ".into(),
            })
            .unwrap();
        assert_eq!(edited.slice([0, 10]).unwrap(), "a😀XYZbc");
        assert_eq!(edited.slice([1, 8]).unwrap(), "😀XYZ");
        assert_eq!(source.slice([0, 7]).unwrap(), "a😀bc");
        assert!(edited.slice([2, 4]).is_err());
        assert!(
            source
                .replace(&TextEdit {
                    range: [7, 2],
                    replacement: String::new()
                })
                .is_err()
        );
    }
    #[test]
    fn early_edit_copies_bounded_payload() {
        for size in [256 * 1024, 4 * 1024 * 1024, 16 * 1024 * 1024] {
            let source = SourceText::new(&"a".repeat(size)).unwrap();
            let edited = source
                .replace(&TextEdit {
                    range: [1, 2],
                    replacement: "b".into(),
                })
                .unwrap();
            assert_eq!(edited.len(), size as u32);
            assert_eq!(edited.slice([0, 3]).unwrap(), "aba");
            assert_eq!(source.slice([0, 3]).unwrap(), "aaa");
            assert!(edited.shares_buffer_at(&source, size as u32 - 1));
        }
    }
    #[test]
    fn repeated_edits_do_not_pin_removed_buffers() {
        let mut source = SourceText::new("abc").unwrap();
        for _ in 0..10_000 {
            source = source
                .replace(&TextEdit {
                    range: [1, 2],
                    replacement: "x".into(),
                })
                .unwrap();
        }
        assert_eq!(source.slice([0, 3]).unwrap(), "axc");
        assert!(source.piece_count() <= 3);
        assert_eq!(source.retained_buffer_bytes(), 4);
    }
}
