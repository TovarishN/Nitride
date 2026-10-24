use crate::{PrepareError, Source};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Name,
    Number,
    String,
    Char,
    Punctuation,
    End,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    pub span: [usize; 2],
}

#[derive(Clone)]
pub struct Lexer<'a> {
    pub source: &'a Source,
    pub position: usize,
}

impl Lexer<'_> {
    fn rest(&self) -> &str {
        &self.source.text[self.position..]
    }
    pub fn trivia(&mut self) -> Result<(), PrepareError> {
        loop {
            if self.rest().starts_with("//") {
                self.position += self.rest().find('\n').unwrap_or(self.rest().len());
            } else if self.rest().starts_with("/*") {
                let end = self.rest()[2..].find("*/").ok_or_else(|| {
                    self.source.error(
                        [self.position, self.source.text.len()],
                        "unterminated comment",
                    )
                })?;
                self.position += end + 4;
            } else if let Some(c) = self.rest().chars().next().filter(|c| c.is_whitespace()) {
                self.position += c.len_utf8();
            } else {
                return Ok(());
            }
        }
    }
    pub fn next(&mut self) -> Result<Token, PrepareError> {
        self.trivia()?;
        let start = self.position;
        let Some(c) = self.rest().chars().next() else {
            return Ok(Token {
                kind: Kind::End,
                text: String::new(),
                span: [start, start],
            });
        };
        let kind;
        let text;
        if c.is_ascii_alphabetic() || c == '_' {
            self.position += 1;
            loop {
                while self
                    .rest()
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    self.position += 1;
                }
                let bytes = self.rest().as_bytes();
                if bytes.len() > 1
                    && bytes[0] == b'.'
                    && (bytes[1].is_ascii_alphabetic() || bytes[1] == b'_')
                {
                    self.position += 2;
                } else {
                    break;
                }
            }
            kind = Kind::Name;
            text = self.source.text[start..self.position].into();
        } else if c.is_ascii_digit() {
            while self
                .rest()
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_digit())
            {
                self.position += 1;
            }
            kind = Kind::Number;
            text = self.source.text[start..self.position].into();
        } else if c == '"' || c == '\'' {
            kind = if c == '"' { Kind::String } else { Kind::Char };
            text = self.quoted(c)?;
            if kind == Kind::Char && text.encode_utf16().count() != 1 {
                return Err(self.source.error(
                    [start, self.position],
                    "character literal must contain one BMP character",
                ));
            }
        } else {
            kind = Kind::Punctuation;
            self.position += c.len_utf8();
            if c == '.' && self.rest().starts_with('.') {
                self.position += 1;
            }
            text = self.source.text[start..self.position].into();
        }
        Ok(Token {
            kind,
            text,
            span: [start, self.position],
        })
    }
    fn quoted(&mut self, quote: char) -> Result<String, PrepareError> {
        let start = self.position;
        self.position += 1;
        let mut units = Vec::new();
        loop {
            let Some(c) = self.rest().chars().next() else {
                return Err(self
                    .source
                    .error([start, self.position], "unterminated literal"));
            };
            self.position += c.len_utf8();
            if c == quote {
                break;
            }
            if c == '\n' {
                return Err(self
                    .source
                    .error([start, self.position], "unterminated literal"));
            }
            if c != '\\' {
                let mut buffer = [0; 2];
                units.extend_from_slice(c.encode_utf16(&mut buffer));
                continue;
            }
            let escape_start = self.position - 1;
            let Some(e) = self.rest().chars().next() else {
                return Err(self
                    .source
                    .error([start, self.position], "unterminated escape"));
            };
            self.position += e.len_utf8();
            let unit = match e {
                'n' => 10,
                'r' => 13,
                't' => 9,
                '0' => 0,
                '\\' => 92,
                '"' => 34,
                '\'' => 39,
                'u' => {
                    let hex = self
                        .rest()
                        .get(..4)
                        .filter(|s| s.bytes().all(|b| b.is_ascii_hexdigit()))
                        .ok_or_else(|| {
                            self.source
                                .error([escape_start, self.position], "\\u needs four hex digits")
                        })?;
                    let code = u16::from_str_radix(hex, 16).unwrap();
                    self.position += 4;
                    code
                }
                _ => {
                    return Err(self
                        .source
                        .error([escape_start, self.position], "unknown literal escape"));
                }
            };
            units.push(unit);
        }
        String::from_utf16(&units).map_err(|_| {
            self.source.error(
                [start, self.position],
                "unpaired UTF-16 surrogate is not admitted",
            )
        })
    }
    // Syntax-only semantic regions are opaque. Balance braces, respecting
    // comments and ordinary C# strings/chars; advanced C# string forms fail closed.
    pub fn opaque_block(&mut self, start: usize) -> Result<(), PrepareError> {
        let mut depth = 1usize;
        while depth > 0 {
            self.trivia()?;
            let Some(c) = self.rest().chars().next() else {
                return Err(self
                    .source
                    .error([start, self.position], "unterminated opaque semantic block"));
            };
            if self.rest().starts_with("\"\"\"") || self.rest().starts_with("@\"") {
                return Err(self.source.error(
                    [self.position, self.position],
                    "raw/verbatim C# strings in opaque blocks are not admitted",
                ));
            }
            if c == '"' || c == '\'' {
                self.quoted(c)?;
                continue;
            }
            self.position += c.len_utf8();
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        Ok(())
    }
}
