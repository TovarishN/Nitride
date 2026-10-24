//! Decode UTF-16-compatible .ngr literal contents after generated recognition.
use crate::{PrepareError, Source};

pub fn decode(source: &Source, span: [usize; 2], character: bool) -> Result<String, PrepareError> {
    let mut decoder = Decoder {
        source,
        position: span[0],
    };
    let value = decoder.quoted(source.text[span[0]..].chars().next().unwrap())?;
    if character && value.encode_utf16().count() != 1 {
        return Err(source.error(span, "character literal must contain one BMP character"));
    }
    Ok(value)
}
struct Decoder<'a> {
    source: &'a Source,
    position: usize,
}
impl Decoder<'_> {
    fn rest(&self) -> &str {
        &self.source.text[self.position..]
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
}
