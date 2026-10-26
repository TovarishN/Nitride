//! Nitrogen document signatures (`DocumentSignatures.Read`): `#!nitrogen`
//! directives in a document's leading trivia choose its language. They are
//! read only before the first other text, and masked to spaces (line breaks
//! kept) so every other offset is unchanged.

const MARKER: &str = "#!nitrogen";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Grammar,
    Language,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub value: String,
    pub is_path: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub mode: Mode,
    pub reference: Option<Reference>,
    pub span: [u32; 2],
}

#[derive(Clone, Debug, Default)]
pub struct Header {
    pub signatures: Vec<Signature>,
    /// The text with directives (and comments holding them) replaced by spaces.
    pub masked: String,
    /// `NGR0401` problems: span and message.
    pub diagnostics: Vec<([u32; 2], String)>,
}

impl Header {
    pub fn grammar(&self) -> bool {
        self.signatures.iter().any(|s| s.mode == Mode::Grammar)
    }
    pub fn language(&self) -> Option<&Signature> {
        self.signatures.iter().find(|s| s.mode == Mode::Language)
    }
}

/// Whether `text` might carry a grammar signature, cheaply.
pub fn may_declare(text: &str) -> bool {
    text.contains("#!nitrogen grammar")
}

pub fn read(text: &str) -> Header {
    Reader {
        text,
        masked: text.as_bytes().to_vec(),
        header: Header::default(),
        consumed: false,
    }
    .run()
}

struct Reader<'a> {
    text: &'a str,
    masked: Vec<u8>,
    header: Header,
    consumed: bool,
}

fn line_end(text: &str, from: usize, end: usize) -> usize {
    text[from..end].find(['\r', '\n']).map_or(end, |i| from + i)
}

fn is_marker(text: &str, at: usize, end: usize) -> bool {
    text[at..end].starts_with(MARKER)
        && text[at + MARKER.len()..end]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

fn valid_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

impl Reader<'_> {
    fn run(mut self) -> Header {
        let text = self.text;
        let bom = text.starts_with('\u{feff}');
        let mut position = if bom { '\u{feff}'.len_utf8() } else { 0 };
        while position < text.len() {
            position += text[position..].len() - text[position..].trim_start().len();
            if position >= text.len() {
                break;
            }
            let start = position;
            let rest = &text[position..];
            if rest.starts_with("//") {
                let end = line_end(text, position, text.len());
                if self.read_lines(position + 2, end) {
                    self.mask(start, end);
                }
                position = end;
            } else if rest.starts_with("/*") || rest.starts_with("<!--") {
                let html = rest.starts_with("<!--");
                let close = if html { "-->" } else { "*/" };
                let content = position + if html { 4 } else { 2 };
                let close_at = text[content..].find(close).map(|i| content + i);
                let end = close_at.unwrap_or(text.len());
                if self.read_lines(content, end) {
                    self.mask(start, close_at.map_or(end, |c| c + close.len()));
                    if close_at.is_none() {
                        self.error(start, end, "unterminated signature comment");
                    }
                }
                position = close_at.map_or(end, |c| c + close.len());
            } else if is_marker(text, position, text.len()) {
                let end = line_end(text, position, text.len());
                if !self.directive(position, end) {
                    break;
                }
                self.mask(start, end);
                position = end;
            } else {
                break;
            }
        }
        if self.consumed && bom {
            for byte in &mut self.masked[..'\u{feff}'.len_utf8()] {
                *byte = b' ';
            }
        }
        // Only ASCII bytes were written, over whole characters.
        self.header.masked = String::from_utf8(self.masked).unwrap_or_else(|_| text.to_owned());
        self.header
    }

    fn mask(&mut self, start: usize, end: usize) {
        self.consumed = true;
        for byte in &mut self.masked[start..end] {
            if !matches!(*byte, b'\r' | b'\n') {
                *byte = b' ';
            }
        }
    }

    fn error(&mut self, start: usize, end: usize, message: &str) {
        self.header
            .diagnostics
            .push(([start as u32, end as u32], message.to_owned()));
    }

    /// Directives on the lines of a comment; true when any was found.
    fn read_lines(&mut self, mut start: usize, end: usize) -> bool {
        let text = self.text;
        let mut found = false;
        while start < end {
            let next = line_end(text, start, end);
            let content = start + (text[start..next].len() - text[start..next].trim_start().len());
            if is_marker(text, content, next) {
                found |= self.directive(content, next);
            }
            start = next;
            if text[start..end].starts_with('\r') {
                start += 1;
            }
            if text[start..end].starts_with('\n') {
                start += 1;
            }
        }
        found
    }

    /// One `#!nitrogen …` directive in `[start, end)`; false when it is not a
    /// header directive (a region marker), which ends the header.
    fn directive(&mut self, start: usize, end: usize) -> bool {
        let text = self.text;
        let end = start + text[start..end].trim_end().len();
        let mut cursor = start + MARKER.len();
        let skip = |cursor: &mut usize| {
            *cursor += text[*cursor..end].len() - text[*cursor..end].trim_start().len();
        };
        let word = |cursor: &mut usize| -> String {
            skip(cursor);
            let from = *cursor;
            *cursor += text[from..end]
                .find(char::is_whitespace)
                .unwrap_or(end - from);
            text[from..*cursor].to_owned()
        };
        let mode = match word(&mut cursor).as_str() {
            "region" | "end" => return false,
            "grammar" => Some(Mode::Grammar),
            "language" => Some(Mode::Language),
            "augment" => None,
            _ => {
                self.error(
                    start,
                    end,
                    "expected grammar, language, or augment after #!nitrogen",
                );
                return true;
            }
        };
        let mut reference = None;
        if mode != Some(Mode::Grammar) {
            skip(&mut cursor);
            if text[cursor..end].starts_with('"') {
                cursor += 1;
                let mut value = String::new();
                let mut closed = false;
                let mut chars = text[cursor..end].char_indices();
                while let Some((i, c)) = chars.next() {
                    match c {
                        '"' => {
                            closed = true;
                            cursor += i + 1;
                            break;
                        }
                        '\\' => match chars.clone().next() {
                            Some((_, next @ ('"' | '\\'))) => {
                                value.push(next);
                                chars.next();
                            }
                            _ => value.push(c),
                        },
                        _ => value.push(c),
                    }
                }
                if !closed || value.is_empty() {
                    self.error(start, end, "expected a non-empty quoted language path");
                    return true;
                }
                if text[cursor..end]
                    .chars()
                    .next()
                    .is_some_and(|c| !c.is_whitespace())
                {
                    self.error(start, end, "expected whitespace after language path");
                    return true;
                }
                reference = Some(Reference {
                    value,
                    is_path: true,
                });
            } else {
                let name = word(&mut cursor);
                if !valid_name(&name) {
                    self.error(
                        start,
                        end,
                        "expected a quoted local path or a language name",
                    );
                    return true;
                }
                reference = Some(Reference {
                    value: name,
                    is_path: false,
                });
            }
            if mode.is_none() {
                // An augmentation's input selector; augmentations are not served.
                let selector = word(&mut cursor);
                if !selector.is_empty() && selector != "document" && selector != "regions" {
                    self.error(start, end, "expected document or regions input selector");
                    return true;
                }
            }
        }
        skip(&mut cursor);
        if cursor != end {
            self.error(start, end, "unexpected text after Nitrogen signature");
            return true;
        }
        let Some(mode) = mode else {
            return true; // An augmentation: read, masked, not served.
        };
        let conflicting = self.header.signatures.iter().any(|previous| {
            mode == Mode::Grammar || previous.mode == Mode::Grammar || mode == previous.mode
        });
        if conflicting {
            self.error(
                start,
                end,
                "incompatible or duplicate Nitrogen document signature",
            );
        } else {
            self.header.signatures.push(Signature {
                mode,
                reference,
                span: [start as u32, end as u32],
            });
        }
        true
    }
}
