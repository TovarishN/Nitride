//! What each literal of a rule is for, from its place in the rule (the
//! TextMate palette), as Nitrogen's `LiteralRoles` classifies it.
use nitride_core::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// A word of a rule that lowers to an operation, outside its arguments.
    Operation,
    /// A word that can start a rule that declares a symbol.
    DeclarationKeyword,
    /// A word that can start any other rule.
    SectionKeyword,
    /// A word after an earlier element of its rule.
    ModifierWord,
    /// One of several bare word alternatives, not at the start of its rule.
    ConstantWord,
    BlockBracket,
    Parenthesis,
    SquareBracket,
    Terminator,
    Separator,
    Operator,
}

pub fn is_word(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && text.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Punctuation by its spelling; `None` for a word.
pub fn punctuation(text: &str) -> Option<Role> {
    (!is_word(text)).then_some(match text {
        "{" | "}" => Role::BlockBracket,
        "(" | ")" => Role::Parenthesis,
        "[" | "]" => Role::SquareBracket,
        ";" => Role::Terminator,
        "," => Role::Separator,
        _ => Role::Operator,
    })
}

/// Every `(rule, literal)` of the language's syntax rules and its role; the
/// earliest occurrence in a rule decides. `declares(rule name)` says whether
/// a rule has a `declares` clause.
pub fn roles(
    language: &Language<'_>,
    declares: &dyn Fn(&str) -> bool,
) -> HashMap<(u16, String), Role> {
    let mut roles = HashMap::new();
    for (id, rule) in language.rules.iter().enumerate() {
        if rule.lexical || rule.alias {
            continue;
        }
        let arguments = match rule.action {
            Action::Operation { fields, .. } | Action::Overload { fields, .. } => {
                Some(fields.iter().copied().collect::<HashSet<&str>>())
            }
            _ => None,
        };
        let body = match &rule.expr {
            Expr::Label(_, inner) => *inner,
            expr => expr,
        };
        let mut walker = Walker {
            language,
            rule: id as u16,
            body,
            declares: declares(rule.name),
            arguments,
            inlining: HashSet::new(),
            roles: &mut roles,
        };
        walker.walk(&rule.expr, true, false, false);
    }
    roles
}

struct Walker<'a, 'g> {
    language: &'a Language<'g>,
    rule: u16,
    /// The rule's body without a label: a word choice that is all of it is an enumeration.
    body: &'a Expr<'g>,
    declares: bool,
    arguments: Option<HashSet<&'g str>>,
    inlining: HashSet<u16>,
    roles: &'a mut HashMap<(u16, String), Role>,
}

impl<'g> Walker<'_, 'g> {
    /// Visits `expr`; returns whether it can match empty input.
    fn walk(
        &mut self,
        expr: &'g Expr<'g>,
        first: bool,
        in_argument: bool,
        word_choice: bool,
    ) -> bool {
        match expr {
            Expr::Literal(text) => {
                let role = self.classify(text, first, in_argument, word_choice);
                self.roles
                    .entry((self.rule, (*text).to_owned()))
                    .or_insert(role);
                text.is_empty()
            }
            Expr::Sequence(items) => {
                let mut nullable = true;
                for item in *items {
                    nullable &= self.walk(item, first && nullable, in_argument, false);
                }
                nullable
            }
            Expr::Choice(items) => {
                let words = items.len() >= 2
                    && items
                        .iter()
                        .all(|i| matches!(i, Expr::Literal(t) if is_word(t)));
                let whole = std::ptr::eq(expr, self.body);
                let mut nullable = false;
                for item in *items {
                    nullable |= self.walk(item, first && !(words && whole), in_argument, words);
                }
                nullable
            }
            Expr::Label(label, inner) => {
                let argument =
                    in_argument || self.arguments.as_ref().is_some_and(|a| a.contains(label));
                self.walk(inner, first, argument, word_choice)
            }
            Expr::Repeat { inner, min, .. } => {
                self.walk(inner, first, in_argument, false) || *min == 0
            }
            Expr::Predicate { .. } => true,
            Expr::Ref(id) => self.reference(*id, first, in_argument),
            _ => false,
        }
    }

    /// An alias has no node: its literals are leaves under this rule, so they are classified here.
    fn reference(&mut self, id: u16, first: bool, in_argument: bool) -> bool {
        let Some(rule) = self.language.rules.get(id as usize) else {
            return false;
        };
        if rule.lexical || !rule.alias || !self.inlining.insert(id) {
            return false;
        }
        let nullable = self.walk(&rule.expr, first, in_argument, false);
        self.inlining.remove(&id);
        nullable
    }

    fn classify(&self, text: &str, first: bool, in_argument: bool, word_choice: bool) -> Role {
        if let Some(role) = punctuation(text) {
            return role;
        }
        if self.arguments.is_some() && !in_argument {
            return Role::Operation;
        }
        if word_choice && !first {
            return Role::ConstantWord;
        }
        if first {
            return if self.declares {
                Role::DeclarationKeyword
            } else {
                Role::SectionKeyword
            };
        }
        Role::ModifierWord
    }
}
