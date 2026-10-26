//! Formatting: whitespace at the start and end of lines, blank-line runs and
//! the end of the file. Lines are indented by their `{ }` block; lines inside
//! an unclosed `(` or `[` keep their indentation, as do lines that start
//! inside a token or block comment. Comment-only lines take the indentation
//! of the next code line. Tokens, line breaks and spaces within lines never
//! change.
use crate::analysis::Analysis;
use crate::server::range;
use serde_json::{Value, json};

/// Per byte: inside a token or comment (so not literal text).
/// Mask values: inside a token, inside a comment; 0 is literal text.
const TOKEN: u8 = 2;
const COMMENT: u8 = 1;

fn opaque(analysis: &Analysis) -> Vec<u8> {
    let text = analysis.lines.text();
    let bytes = text.as_bytes();
    let mut mask = vec![0u8; bytes.len()];
    if let Some(tree) = &analysis.tree {
        for (id, node) in tree.nodes.iter().enumerate() {
            if node.child_count == 0 && tree.rule(id as u32).lexical {
                let [start, end] = node.span.map(|b| (b as usize).min(bytes.len()));
                mask[start..end].iter_mut().for_each(|m| *m = TOKEN);
            }
        }
    }
    // Comments in the text between tokens.
    let mut i = 0;
    while i + 1 < bytes.len() {
        if mask[i] != 0 {
            i += 1;
            continue;
        }
        let end = if bytes[i..].starts_with(b"//") {
            text[i..].find('\n').map_or(bytes.len(), |n| i + n)
        } else if bytes[i..].starts_with(b"/*") {
            text[i + 2..]
                .find("*/")
                .map_or(bytes.len(), |n| i + 2 + n + 2)
        } else {
            i += 1;
            continue;
        };
        mask[i..end].iter_mut().for_each(|m| *m = COMMENT);
        i = end;
    }
    mask
}

/// The line ending the file uses: its first one, else `\n`.
fn line_ending(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if i > 0 && text.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

struct Line {
    start: usize,
    /// End of the content, before the line ending.
    end: usize,
    /// Inside a token or comment at its first byte.
    opaque: bool,
    /// A token runs on past the line's end: its trailing whitespace is the token's.
    token_continues: bool,
    braces: usize,
    /// Unclosed `(` and `[` at its start.
    brackets: usize,
}

fn lines(analysis: &Analysis, mask: &[u8]) -> Vec<Line> {
    let text = analysis.lines.text();
    let index = &analysis.lines;
    let (mut braces, mut brackets) = (0usize, 0usize);
    let mut out = Vec::new();
    for line in 0..index.line_count() {
        let start = index.offset(line, 0) as usize;
        let end = index.offset(line, u32::MAX) as usize;
        let masked = |i: usize| mask.get(i).is_some_and(|m| *m != 0);
        let continues = start > 0 && masked(start - 1);
        out.push(Line {
            start,
            end,
            opaque: continues && masked(start),
            token_continues: mask.get(end).is_some_and(|m| *m == TOKEN),
            braces,
            brackets,
        });
        let next = index.offset(line + 1, 0) as usize;
        let stop = if line + 1 < index.line_count() {
            next
        } else {
            text.len()
        };
        let literal = text.as_bytes()[start..stop]
            .iter()
            .zip(&mask[start..stop])
            .filter(|(_, opaque)| **opaque == 0);
        for (byte, _) in literal {
            match byte {
                b'{' => braces += 1,
                b'}' => braces = braces.saturating_sub(1),
                b'(' | b'[' => brackets += 1,
                b')' | b']' => brackets = brackets.saturating_sub(1),
                _ => {}
            }
        }
    }
    out
}

/// `TextEdit`s formatting lines `first..=last`; with `end_of_file`, the
/// end-of-file rule applies when the last line is among them.
pub fn edits(
    analysis: &Analysis,
    first: u32,
    last: u32,
    tab_size: u32,
    spaces: bool,
    end_of_file: bool,
) -> Vec<Value> {
    let text = analysis.lines.text();
    let mask = opaque(analysis);
    let lines = lines(analysis, &mask);
    let unit = if spaces {
        " ".repeat(tab_size.clamp(1, 16) as usize)
    } else {
        "\t".into()
    };
    let blank = |l: &Line| text[l.start..l.end].trim().is_empty();
    let comment = |l: &Line| {
        let t = text[l.start..l.end].trim_start();
        t.starts_with("//") || t.starts_with("/*")
    };
    let level = |l: &Line| {
        let starts_closing = text[l.start..l.end].trim_start().starts_with('}');
        l.braces.saturating_sub(usize::from(starts_closing))
    };
    // Trailing blank lines belong to the end-of-file rule.
    let content_lines = lines.iter().rposition(|l| !blank(l)).map_or(0, |i| i + 1);
    let last = (last as usize).min(lines.len().saturating_sub(1));
    let mut edits = Vec::new();
    let mut previous_blank = false;
    for (n, line) in lines.iter().enumerate() {
        let is_blank = blank(line);
        let in_range = (first as usize) <= n && n <= last;
        if n >= content_lines {
            break;
        }
        if !in_range || line.opaque {
            previous_blank = is_blank;
            continue;
        }
        if is_blank {
            if previous_blank {
                // A second blank line in a run: delete it, line ending included.
                let next = lines.get(n + 1).map_or(text.len(), |l| l.start);
                edits.push(json!({"range": range(&analysis.lines, [line.start as u32, next as u32]), "newText": ""}));
            } else if line.end > line.start {
                edits.push(json!({"range": range(&analysis.lines, [line.start as u32, line.end as u32]), "newText": ""}));
            }
            previous_blank = true;
            continue;
        }
        previous_blank = false;
        let content = &text[line.start..line.end];
        let leading = content.len() - content.trim_start().len();
        let trailing_at = line.start + content.trim_end().len();
        if trailing_at < line.end && !line.token_continues {
            edits.push(json!({"range": range(&analysis.lines, [trailing_at as u32, line.end as u32]), "newText": ""}));
        }
        if line.brackets > 0 {
            continue;
        }
        // A comment line is indented like the next code line.
        let target = if comment(line) {
            lines[n..content_lines]
                .iter()
                .find(|l| !blank(l) && !comment(l))
                .map_or(level(line), level)
        } else {
            level(line)
        };
        let wanted = unit.repeat(target);
        if content[..leading] != wanted {
            let span = [line.start as u32, (line.start + leading) as u32];
            edits.push(json!({"range": range(&analysis.lines, span), "newText": wanted}));
        }
    }
    // The end of the file: exactly one line ending after the last content.
    if end_of_file && last + 1 >= lines.len() && content_lines > 0 {
        let end = lines[content_lines - 1].end;
        let ending = line_ending(text);
        if &text[end..] != ending {
            edits.push(json!({"range": range(&analysis.lines, [end as u32, text.len() as u32]), "newText": ending}));
        }
    }
    edits
}
