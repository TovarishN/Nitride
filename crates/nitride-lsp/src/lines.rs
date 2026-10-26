//! Byte offsets ⇄ LSP positions (zero-based line, UTF-16 column).

pub struct LineIndex {
    text: String,
    /// Byte offset where each line starts.
    starts: Vec<u32>,
}

impl LineIndex {
    /// Lines end at `\n`, `\r\n` or a lone `\r`, as LSP defines them.
    pub fn new(text: &str) -> Self {
        let bytes = text.as_bytes();
        let mut starts = vec![0];
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => starts.push(i as u32 + 1),
                b'\r' if bytes.get(i + 1) != Some(&b'\n') => starts.push(i as u32 + 1),
                _ => {}
            }
            i += 1;
        }
        Self {
            text: text.to_owned(),
            starts,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The position of `byte`; a byte inside a character counts as its start.
    pub fn position(&self, byte: u32) -> (u32, u32) {
        let mut byte = (byte as usize).min(self.text.len());
        while !self.text.is_char_boundary(byte) {
            byte -= 1;
        }
        // Between the `\r` and `\n` of a CRLF is still the end of the line.
        if byte > 0
            && self.text.as_bytes().get(byte) == Some(&b'\n')
            && self.text.as_bytes()[byte - 1] == b'\r'
        {
            byte -= 1;
        }
        let line = self.starts.partition_point(|&s| s as usize <= byte) - 1;
        let start = self.starts[line] as usize;
        let column = self.text[start..byte].encode_utf16().count();
        (line as u32, column as u32)
    }

    /// The byte at `line`/`column`, clamped to the line's content and to the
    /// start of a character.
    pub fn offset(&self, line: u32, column: u32) -> u32 {
        let Some(&start) = self.starts.get(line as usize) else {
            return self.text.len() as u32;
        };
        let end = self.content_end(line as usize);
        let mut units = 0;
        for (i, c) in self.text[start as usize..end].char_indices() {
            if units + c.len_utf16() as u32 > column {
                return start + i as u32;
            }
            units += c.len_utf16() as u32;
        }
        end as u32
    }

    pub fn line_count(&self) -> u32 {
        self.starts.len() as u32
    }

    /// The end of a line's text, before its terminator.
    fn content_end(&self, line: usize) -> usize {
        let end = self
            .starts
            .get(line + 1)
            .map_or(self.text.len(), |&s| s as usize);
        let line_text = &self.text.as_bytes()[self.starts[line] as usize..end];
        end - match line_text {
            [.., b'\r', b'\n'] => 2,
            [.., b'\n' | b'\r'] => 1,
            _ => 0,
        }
    }
}
