//! Semantic tokens, classified as Nitrogen's `SemanticTokenBuilder` does
//! lexically: declared names, then literal words (keywords) and other
//! literals (operators), number- and string-like tokens, and comments.
use crate::analysis::Analysis;
use nitride_prepare::GrammarItem;
use std::collections::HashMap;

pub const TYPES: [&str; 11] = [
    "namespace",
    "type",
    "function",
    "variable",
    "parameter",
    "property",
    "keyword",
    "operator",
    "number",
    "string",
    "comment",
];
pub const MODIFIERS: [&str; 2] = ["declaration", "defaultLibrary"];

const NAMESPACE: u32 = 0;
const TYPE: u32 = 1;
const FUNCTION: u32 = 2;
const VARIABLE: u32 = 3;
const KEYWORD: u32 = 6;
const OPERATOR: u32 = 7;
const NUMBER: u32 = 8;
const STRING: u32 = 9;
const COMMENT: u32 = 10;
const DECLARATION: u32 = 1;

struct Token {
    start: u32,
    end: u32,
    kind: u32,
    modifiers: u32,
}

/// The LSP relative encoding of a document's tokens.
pub fn encode(analysis: &Analysis) -> Vec<u32> {
    let text = analysis.lines.text();
    let mut tokens = Vec::new();
    let Some(tree) = &analysis.tree else {
        return vec![];
    };
    // Names first: a declared name colours its whole span.
    let mut names = HashMap::new();
    for name in &analysis.declarations {
        let kind = if name.kind == "function" {
            FUNCTION
        } else {
            VARIABLE
        };
        names.insert(name.name_span[0], (name.name_span[1], kind, DECLARATION));
    }
    fn outline(items: &[GrammarItem], names: &mut HashMap<u32, (u32, u32, u32)>) {
        for item in items {
            let kind = match item.kind {
                "module" => NAMESPACE,
                "function" | "helper" => FUNCTION,
                _ => TYPE,
            };
            let span = item.name_span.map(|b| b as u32);
            names.insert(span[0], (span[1], kind, DECLARATION));
            outline(&item.children, names);
        }
    }
    outline(&analysis.outline, &mut names);

    // Leaves in text order: token nodes, and childless syntax nodes (literals).
    let mut leaves: Vec<(u32, u32, Option<u32>)> = Vec::new();
    for (id, node) in tree.nodes.iter().enumerate() {
        if node.child_count != 0 || node.span[0] >= node.span[1] {
            continue;
        }
        let rule = tree.rule(id as u32);
        let class = if rule.lexical {
            let name = rule.name.rsplit('.').next().unwrap_or_default();
            if ["Number", "Digit", "Integer"]
                .iter()
                .any(|n| name.contains(n))
            {
                Some(NUMBER)
            } else if ["String", "Char"].iter().any(|n| name.contains(n)) {
                Some(STRING)
            } else {
                None
            }
        } else {
            Some(OPERATOR) // Literal text; split into words and operators below.
        };
        leaves.push((node.span[0], node.span[1], class));
    }
    leaves.sort_unstable();

    let mut at = 0u32;
    for (start, end, class) in leaves {
        if start < at {
            continue;
        }
        gap(text, at, start, &mut tokens);
        if let Some(&(name_end, kind, modifiers)) = names.get(&start) {
            tokens.push(Token {
                start,
                end: name_end,
                kind,
                modifiers,
            });
            at = name_end.max(end);
            continue;
        }
        match class {
            Some(OPERATOR) => literal(text, start, end, &mut tokens),
            Some(kind) => tokens.push(Token {
                start,
                end,
                kind,
                modifiers: 0,
            }),
            None => {}
        }
        at = end;
    }
    gap(text, at, text.len() as u32, &mut tokens);
    relative(analysis, &tokens)
}

/// Text between leaves: comments, and the literals of the enclosing rules.
fn gap(text: &str, start: u32, end: u32, tokens: &mut Vec<Token>) {
    let bytes = text.as_bytes();
    let mut i = start as usize;
    let end = end as usize;
    let mut run = i;
    while i < end {
        let comment_end = if bytes[i..end].starts_with(b"//") {
            Some(text[i..end].find('\n').map_or(end, |n| i + n))
        } else if bytes[i..end].starts_with(b"/*") {
            Some(text[i + 2..end].find("*/").map_or(end, |n| i + 2 + n + 2))
        } else {
            None
        };
        if let Some(comment_end) = comment_end {
            literal(text, run as u32, i as u32, tokens);
            tokens.push(Token {
                start: i as u32,
                end: comment_end as u32,
                kind: COMMENT,
                modifiers: 0,
            });
            i = comment_end;
            run = i;
        } else {
            i += text[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    literal(text, run as u32, end as u32, tokens);
}

/// Words become keywords; every other visible character an operator.
fn literal(text: &str, start: u32, end: u32, tokens: &mut Vec<Token>) {
    let slice = &text[start as usize..end as usize];
    let mut chars = slice.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let at = start + i as u32;
        if c.is_whitespace() {
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let mut stop = at + 1;
            while let Some(&(j, d)) = chars.peek() {
                if !(d.is_ascii_alphanumeric() || d == '_') {
                    break;
                }
                stop = start + j as u32 + 1;
                chars.next();
            }
            tokens.push(Token {
                start: at,
                end: stop,
                kind: KEYWORD,
                modifiers: 0,
            });
        } else {
            tokens.push(Token {
                start: at,
                end: at + c.len_utf8() as u32,
                kind: OPERATOR,
                modifiers: 0,
            });
        }
    }
}

/// Splits tokens per line and encodes them relative to the previous one.
fn relative(analysis: &Analysis, tokens: &[Token]) -> Vec<u32> {
    let lines = &analysis.lines;
    let text = lines.text();
    let mut data = Vec::with_capacity(tokens.len() * 5);
    let (mut previous_line, mut previous_column) = (0, 0);
    for token in tokens {
        let mut start = token.start;
        while start < token.end {
            let (line, column) = lines.position(start);
            let line_end = lines.offset(line, u32::MAX).min(token.end).max(start);
            let length = text[start as usize..line_end as usize]
                .encode_utf16()
                .count() as u32;
            if length > 0 {
                let delta_line = line - previous_line;
                let delta_column = if delta_line == 0 {
                    column - previous_column
                } else {
                    column
                };
                data.extend([
                    delta_line,
                    delta_column,
                    length,
                    token.kind,
                    token.modifiers,
                ]);
                (previous_line, previous_column) = (line, column);
            }
            if line + 1 >= lines.line_count() {
                break;
            }
            start = lines.offset(line + 1, 0).max(line_end + 1);
        }
    }
    data
}
