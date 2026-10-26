//! Semantic tokens, classified as Nitrogen's `SemanticTokenBuilder` does:
//! declared names, then literals by their role in the rule that matched them
//! (the TextMate palette), number- and string-like tokens, and comments.
//! A client that asks for palette tokens gets the palette types; any other
//! gets each entry's standard fallback, or no token.
use crate::analysis::Analysis;
use crate::palette::{self, Entry};
use crate::roles::{Role, punctuation};
use nitride_prepare::GrammarItem;
use std::collections::HashMap;

/// The standard types, in legend order; palette types follow them.
pub const TYPES: [&str; 12] = [
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
    "enumMember",
];
pub const MODIFIERS: [&str; 2] = ["declaration", "defaultLibrary"];

const NAMESPACE: u32 = 0;
const TYPE: u32 = 1;
const FUNCTION: u32 = 2;
const VARIABLE: u32 = 3;
const NUMBER: u32 = 8;
const STRING: u32 = 9;
const COMMENT: u32 = 10;
const DECLARATION: u32 = 1;

/// The legend: the standard types, then, for a palette client, every palette entry.
pub fn legend(palette: bool) -> Vec<&'static str> {
    let mut types = TYPES.to_vec();
    if palette {
        types.extend(palette::ENTRIES.iter().map(|e| e.name));
    }
    types
}

struct Token {
    start: u32,
    end: u32,
    /// A standard type, used when `palette` is `None`.
    kind: u32,
    modifiers: u32,
    palette: Option<&'static Entry>,
}

impl Token {
    fn standard(start: u32, end: u32, kind: u32, modifiers: u32) -> Self {
        Self {
            start,
            end,
            kind,
            modifiers,
            palette: None,
        }
    }
    fn palette(start: u32, end: u32, entry: &'static Entry) -> Self {
        Self {
            start,
            end,
            kind: 0,
            modifiers: 0,
            palette: Some(entry),
        }
    }
}

/// The innermost syntax (non-lexical) node at increasing positions.
struct Owner {
    /// (start, end, rule), by start, outer nodes first.
    nodes: Vec<(u32, u32, u16)>,
    next: usize,
    open: Vec<(u32, u16)>,
}

impl Owner {
    fn new(analysis: &Analysis) -> Self {
        let mut nodes: Vec<(u32, u32, u16, usize)> = analysis
            .tree
            .iter()
            .flat_map(|tree| {
                tree.nodes.iter().enumerate().filter_map(move |(id, n)| {
                    (n.span[0] < n.span[1] && !tree.rule(id as u32).lexical)
                        .then_some((n.span[0], n.span[1], n.rule, id))
                })
            })
            .collect();
        // Outer nodes first; of equal spans the parent, which comes later in postorder.
        nodes.sort_by_key(|&(start, end, _, id)| {
            (start, std::cmp::Reverse(end), std::cmp::Reverse(id))
        });
        let nodes = nodes
            .into_iter()
            .map(|(start, end, rule, _)| (start, end, rule))
            .collect();
        Self {
            nodes,
            next: 0,
            open: Vec::new(),
        }
    }

    /// The rule of the innermost node containing `at`; positions must not decrease.
    fn at(&mut self, at: u32) -> Option<u16> {
        while let Some(&(start, end, rule)) = self.nodes.get(self.next) {
            if start > at {
                break;
            }
            while self.open.last().is_some_and(|&(e, _)| e <= start) {
                self.open.pop();
            }
            self.open.push((end, rule));
            self.next += 1;
        }
        while self.open.last().is_some_and(|&(e, _)| e <= at) {
            self.open.pop();
        }
        self.open.last().map(|&(_, rule)| rule)
    }
}

/// Literals by rule, longest first, with their roles.
fn literals_by_rule(roles: &HashMap<(u16, String), Role>) -> HashMap<u16, Vec<(&str, Role)>> {
    let mut by_rule: HashMap<u16, Vec<(&str, Role)>> = HashMap::new();
    for ((rule, literal), role) in roles {
        if !literal.is_empty() {
            by_rule
                .entry(*rule)
                .or_default()
                .push((literal.as_str(), *role));
        }
    }
    for literals in by_rule.values_mut() {
        literals.sort_by_key(|(l, _)| std::cmp::Reverse(l.len()));
    }
    by_rule
}

struct Classifier<'a> {
    text: &'a str,
    owner: Owner,
    literals: HashMap<u16, Vec<(&'a str, Role)>>,
}

impl Classifier<'_> {
    /// Literal text in `[start, end)`: the longest literal of the enclosing
    /// rule at each position, else words and punctuation by spelling.
    fn literal(&mut self, start: u32, end: u32, tokens: &mut Vec<Token>) {
        let text = self.text;
        let mut at = start as usize;
        let end = end as usize;
        while at < end {
            let Some(c) = text[at..].chars().next() else {
                break;
            };
            if c.is_whitespace() {
                at += c.len_utf8();
                continue;
            }
            let rule = self.owner.at(at as u32);
            let known = rule.and_then(|r| self.literals.get(&r)).and_then(|ls| {
                ls.iter()
                    .find(|(l, _)| at + l.len() <= end && text[at..].starts_with(l))
            });
            let (length, role) = match known {
                Some((l, role)) => (l.len(), *role),
                None if c.is_alphabetic() || c == '_' => {
                    let length = text[at..end]
                        .find(|d: char| !(d.is_alphanumeric() || d == '_'))
                        .unwrap_or(end - at);
                    (length, Role::SectionKeyword)
                }
                None => (
                    c.len_utf8(),
                    punctuation(&c.to_string()).unwrap_or(Role::Operator),
                ),
            };
            tokens.push(Token::palette(
                at as u32,
                (at + length) as u32,
                palette::of(role),
            ));
            at += length;
        }
    }

    /// Text between leaves: comments, and the literals of the enclosing rules.
    fn gap(&mut self, start: u32, end: u32, tokens: &mut Vec<Token>) {
        let text = self.text;
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
                self.literal(run as u32, i as u32, tokens);
                tokens.push(Token::standard(i as u32, comment_end as u32, COMMENT, 0));
                i = comment_end;
                run = i;
            } else {
                i += text[i..].chars().next().map_or(1, char::len_utf8);
            }
        }
        self.literal(run as u32, end as u32, tokens);
    }
}

/// The LSP relative encoding of a document's tokens.
pub fn encode(analysis: &Analysis, palette: bool) -> Vec<u32> {
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
            Some(u32::MAX) // Literal text of a syntax rule.
        };
        leaves.push((node.span[0], node.span[1], class));
    }
    leaves.sort_unstable();

    let mut classifier = Classifier {
        text,
        owner: Owner::new(analysis),
        literals: literals_by_rule(&analysis.roles),
    };
    let mut at = 0u32;
    for (start, end, class) in leaves {
        if start < at {
            continue;
        }
        classifier.gap(at, start, &mut tokens);
        if let Some(&(name_end, kind, modifiers)) = names.get(&start) {
            tokens.push(Token::standard(start, name_end, kind, modifiers));
            at = name_end.max(end);
            continue;
        }
        match class {
            Some(u32::MAX) => classifier.literal(start, end, &mut tokens),
            Some(kind) => tokens.push(Token::standard(start, end, kind, 0)),
            None => {}
        }
        at = end;
    }
    classifier.gap(at, text.len() as u32, &mut tokens);
    relative(analysis, &tokens, palette)
}

/// A token's type in the client's legend, or `None` when it has none there.
fn type_index(token: &Token, palette: bool) -> Option<u32> {
    let Some(entry) = token.palette else {
        return Some(token.kind);
    };
    if palette {
        let index = palette::ENTRIES
            .iter()
            .position(|e| std::ptr::eq(e, entry))?;
        return Some((TYPES.len() + index) as u32);
    }
    let fallback = entry.fallback?;
    TYPES.iter().position(|t| *t == fallback).map(|i| i as u32)
}

/// Splits tokens per line and encodes them relative to the previous one.
fn relative(analysis: &Analysis, tokens: &[Token], palette: bool) -> Vec<u32> {
    let lines = &analysis.lines;
    let text = lines.text();
    let mut data = Vec::with_capacity(tokens.len() * 5);
    let (mut previous_line, mut previous_column) = (0, 0);
    for token in tokens {
        let Some(kind) = type_index(token, palette) else {
            continue;
        };
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
                data.extend([delta_line, delta_column, length, kind, token.modifiers]);
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
