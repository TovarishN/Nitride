//! Runtime languages: the tables `generate` emits, built in owned storage.
//!
//! The emitted Rust is read back as data, so a loaded language is the
//! generated static table value for value; no second lowering of prepared
//! JSON can drift from the first.
use nitride_core::*;
use std::any::Any;

/// A language prepared at runtime. Its tables live in storage it owns and are
/// freed when it is dropped.
pub struct LoadedLanguage {
    // Declared first, so it is dropped before the storage it points into.
    language: Language<'static>,
    _storage: Vec<Box<dyn Any + Send + Sync>>,
}

impl LoadedLanguage {
    /// Validates and lowers prepared language JSON (as `generate` does).
    pub fn load(prepared_json: &str) -> Result<LoadedLanguage, String> {
        let source = crate::generate(prepared_json)?;
        let start = source
            .find("Language {")
            .ok_or("generated language has no table")?;
        let value = Reader {
            text: source.as_bytes(),
            at: start,
        }
        .value()?;
        let mut builder = Builder {
            storage: Vec::new(),
        };
        let language = builder.language(&value)?;
        Ok(LoadedLanguage {
            language,
            _storage: builder.storage,
        })
    }

    /// The tables, borrowed for no longer than this value lives.
    pub fn language(&self) -> &Language<'_> {
        &self.language
    }
}

/// A value of the generated Rust subset: struct literals, calls, paths,
/// references, slices, tuples and scalar literals.
#[derive(Debug)]
enum Node {
    Struct(String, Vec<(String, Node)>),
    Call(String, Vec<Node>),
    Path(String),
    Ref(Box<Node>),
    Slice(Vec<Node>),
    Tuple(Vec<Node>),
    Str(String),
    Char(char),
    Int(u64),
    Bool(bool),
}

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }
    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(format!(
                "generated table: expected '{}' at {}",
                byte as char, self.at
            ))
        }
    }
    fn skip_spaces(&mut self) {
        while self.peek() == Some(b' ') {
            self.at += 1;
        }
    }
    /// An identifier, or a `::` path when `path` is set.
    fn word(&mut self, path: bool) -> String {
        let start = self.at;
        loop {
            match self.peek() {
                Some(b) if b.is_ascii_alphanumeric() || b == b'_' => self.at += 1,
                Some(b':') if path && self.text.get(self.at + 1) == Some(&b':') => self.at += 2,
                _ => break,
            }
        }
        String::from_utf8_lossy(&self.text[start..self.at]).into_owned()
    }
    fn list(&mut self, close: u8) -> Result<Vec<Node>, String> {
        let mut items = Vec::new();
        while self.peek() != Some(close) {
            items.push(self.value()?);
            if self.peek() == Some(b',') {
                self.at += 1;
            }
        }
        self.at += 1;
        Ok(items)
    }
    fn value(&mut self) -> Result<Node, String> {
        self.skip_spaces();
        match self.peek().ok_or("generated table: unexpected end")? {
            b'&' => {
                self.at += 1;
                if self.peek() == Some(b'[') {
                    self.at += 1;
                    Ok(Node::Slice(self.list(b']')?))
                } else {
                    Ok(Node::Ref(Box::new(self.value()?)))
                }
            }
            b'(' => {
                self.at += 1;
                Ok(Node::Tuple(self.list(b')')?))
            }
            b'"' => Ok(Node::Str(self.string()?)),
            b'\'' => {
                self.at += 1;
                let c = self.escaped(b'\'')?;
                self.expect(b'\'')?;
                Ok(Node::Char(c))
            }
            b'0'..=b'9' => {
                let digits = self.word(false);
                digits
                    .parse()
                    .map(Node::Int)
                    .map_err(|_| format!("generated table: bad number {digits}"))
            }
            _ => {
                let path = self.word(true);
                if path.is_empty() {
                    return Err(format!("generated table: unexpected input at {}", self.at));
                }
                self.skip_spaces();
                match (path.as_str(), self.peek()) {
                    ("true", _) => Ok(Node::Bool(true)),
                    ("false", _) => Ok(Node::Bool(false)),
                    (_, Some(b'(')) => {
                        self.at += 1;
                        Ok(Node::Call(path, self.list(b')')?))
                    }
                    (_, Some(b'{')) => {
                        self.at += 1;
                        let mut fields = Vec::new();
                        loop {
                            self.skip_spaces();
                            if self.peek() == Some(b'}') {
                                self.at += 1;
                                break;
                            }
                            let name = self.word(false);
                            self.expect(b':')?;
                            fields.push((name, self.value()?));
                            self.skip_spaces();
                            if self.peek() == Some(b',') {
                                self.at += 1;
                            }
                        }
                        Ok(Node::Struct(path, fields))
                    }
                    _ => Ok(Node::Path(path)),
                }
            }
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        while self.peek() != Some(b'"') {
            out.push(self.escaped(b'"')?);
        }
        self.at += 1;
        Ok(out)
    }
    /// One character of a Rust `{:?}` string or char literal.
    fn escaped(&mut self, quote: u8) -> Result<char, String> {
        let rest = std::str::from_utf8(&self.text[self.at..])
            .map_err(|_| "generated table: invalid UTF-8".to_owned())?;
        let mut chars = rest.chars();
        let c = chars
            .next()
            .ok_or("generated table: unterminated literal")?;
        if c as u32 == quote as u32 {
            return Err("generated table: empty literal".into());
        }
        if c != '\\' {
            self.at += c.len_utf8();
            return Ok(c);
        }
        let e = chars.next().ok_or("generated table: unterminated escape")?;
        self.at += 2;
        Ok(match e {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '0' => '\0',
            '\\' | '"' | '\'' => e,
            'u' => {
                self.expect(b'{')?;
                let digits = self.word(false);
                self.expect(b'}')?;
                u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or("generated table: bad unicode escape")?
            }
            _ => return Err(format!("generated table: unknown escape \\{e}")),
        })
    }
}

struct Builder {
    storage: Vec<Box<dyn Any + Send + Sync>>,
}

struct Fields<'n>(&'n [(String, Node)]);

impl<'n> Fields<'n> {
    fn get(&self, name: &str) -> Result<&'n Node, String> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
            .ok_or_else(|| format!("generated table: missing field {name}"))
    }
}

fn fields(node: &Node) -> Result<Fields<'_>, String> {
    match node {
        Node::Struct(_, fields) => Ok(Fields(fields)),
        _ => Err(format!("generated table: expected struct, found {node:?}")),
    }
}

fn tail(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

fn int<T: TryFrom<u64>>(node: &Node) -> Result<T, String> {
    match node {
        Node::Int(n) => T::try_from(*n).map_err(|_| "generated table: number out of range".into()),
        _ => Err(format!("generated table: expected number, found {node:?}")),
    }
}

fn boolean(node: &Node) -> Result<bool, String> {
    match node {
        Node::Bool(b) => Ok(*b),
        _ => Err(format!("generated table: expected bool, found {node:?}")),
    }
}

fn items(node: &Node) -> Result<&[Node], String> {
    match node {
        Node::Slice(items) => Ok(items),
        _ => Err(format!("generated table: expected slice, found {node:?}")),
    }
}

impl Builder {
    /// Moves `value` into storage owned by the language being built.
    fn keep<T: ?Sized + Send + Sync + 'static>(&mut self, value: Box<T>) -> &'static T
    where
        Box<T>: Send + Sync,
    {
        let pointer: *const T = &*value;
        self.storage.push(Box::new(value));
        // SAFETY: the box was just moved into `storage`, which keeps it alive
        // and never moves or mutates its contents until the `LoadedLanguage`
        // is dropped. The `'static` lifetime never escapes:
        // `LoadedLanguage::language` shortens it to the borrow of `self`
        // (sound because `Language` is covariant), and `language` is dropped
        // before `storage`.
        unsafe { &*pointer }
    }
    fn str(&mut self, node: &Node) -> Result<&'static str, String> {
        match node {
            Node::Str(s) => Ok(self.keep(s.clone().into_boxed_str())),
            _ => Err(format!("generated table: expected string, found {node:?}")),
        }
    }
    fn slice<T: Send + Sync + 'static>(
        &mut self,
        node: &Node,
        mut each: impl FnMut(&mut Self, &Node) -> Result<T, String>,
    ) -> Result<&'static [T], String> {
        let built = items(node)?
            .iter()
            .map(|item| each(self, item))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self.keep(built.into_boxed_slice()))
    }
    fn strs(&mut self, node: &Node) -> Result<&'static [&'static str], String> {
        self.slice(node, Self::str)
    }
    fn u16s(&mut self, node: &Node) -> Result<&'static [u16], String> {
        self.slice(node, |_, n| int(n))
    }
    fn reference<T: Send + Sync + 'static>(
        &mut self,
        node: &Node,
        build: impl FnOnce(&mut Self, &Node) -> Result<T, String>,
    ) -> Result<&'static T, String> {
        let Node::Ref(inner) = node else {
            return Err(format!(
                "generated table: expected reference, found {node:?}"
            ));
        };
        let value = build(self, inner)?;
        Ok(self.keep(Box::new(value)))
    }

    fn language(&mut self, node: &Node) -> Result<Language<'static>, String> {
        let f = fields(node)?;
        Ok(Language {
            profile: match f.get("profile")? {
                Node::Path(p) if tail(p) == "Typed" => Profile::Typed,
                Node::Path(p) if tail(p) == "SyntaxOnly" => Profile::SyntaxOnly,
                other => return Err(format!("generated table: bad profile {other:?}")),
            },
            rules: self.slice(f.get("rules")?, Self::rule)?,
            start: int(f.get("start")?)?,
            types: self.strs(f.get("types")?)?,
            operations: self.slice(f.get("operations")?, Self::signature)?,
            handlers: self.strs(f.get("handlers")?)?,
            checks: self.slice(f.get("checks")?, Self::check)?,
            helpers: self.slice(f.get("helpers")?, Self::helper)?,
            source_hash: self.str(f.get("source_hash")?)?,
        })
    }
    fn signature(&mut self, node: &Node) -> Result<Signature<'static>, String> {
        let f = fields(node)?;
        Ok(Signature {
            id: self.str(f.get("id")?)?,
            inputs: self.strs(f.get("inputs")?)?,
            result: self.str(f.get("result")?)?,
        })
    }
    fn check(&mut self, node: &Node) -> Result<Check<'static>, String> {
        let f = fields(node)?;
        Ok(Check {
            rule: int(f.get("rule")?)?,
            field: self.str(f.get("field")?)?,
            predicate: match f.get("predicate")? {
                Node::Path(p) if tail(p) == "IsoDate" => Predicate::IsoDate,
                other => return Err(format!("generated table: bad predicate {other:?}")),
            },
            code: self.str(f.get("code")?)?,
            message: self.str(f.get("message")?)?,
        })
    }
    fn hole(&mut self, node: &Node) -> Result<Hole<'static>, String> {
        let f = fields(node)?;
        Ok(Hole {
            name: self.str(f.get("name")?)?,
            category: int(f.get("category")?)?,
        })
    }
    fn arm(&mut self, node: &Node) -> Result<Arm<'static>, String> {
        let f = fields(node)?;
        Ok(Arm {
            captures: self.slice(f.get("captures")?, Self::hole)?,
            pattern: self.str(f.get("pattern")?)?,
            result: self.str(f.get("result")?)?,
        })
    }
    fn helper(&mut self, node: &Node) -> Result<Helper<'static>, String> {
        let f = fields(node)?;
        Ok(Helper {
            name: self.str(f.get("name")?)?,
            parameters: self.slice(f.get("parameters")?, Self::hole)?,
            result: int(f.get("result")?)?,
            body: self.str(f.get("body")?)?,
            selector: match f.get("selector")? {
                Node::Path(p) if p == "None" => None,
                Node::Call(p, args) if p == "Some" && args.len() == 1 => Some(int(&args[0])?),
                other => return Err(format!("generated table: bad selector {other:?}")),
            },
            arms: self.slice(f.get("arms")?, Self::arm)?,
        })
    }
    fn builtin(&mut self, node: &Node) -> Result<Builtin<'static>, String> {
        let f = fields(node)?;
        Ok(Builtin {
            kind: self.str(f.get("kind")?)?,
            name: self.str(f.get("name")?)?,
            candidates: self.u16s(f.get("candidates")?)?,
        })
    }
    fn builtin_value(&mut self, node: &Node) -> Result<BuiltinValue<'static>, String> {
        let f = fields(node)?;
        Ok(BuiltinValue {
            kind: self.str(f.get("kind")?)?,
            name: self.str(f.get("name")?)?,
            ty: self.str(f.get("ty")?)?,
        })
    }
    fn rule(&mut self, node: &Node) -> Result<Rule<'static>, String> {
        let f = fields(node)?;
        Ok(Rule {
            name: self.str(f.get("name")?)?,
            lexical: boolean(f.get("lexical")?)?,
            expr: self.expr(f.get("expr")?)?,
            except: self.strs(f.get("except")?)?,
            precedence: int(f.get("precedence")?)?,
            right_associative: boolean(f.get("right_associative")?)?,
            transparent: boolean(f.get("transparent")?)?,
            alias: boolean(f.get("alias")?)?,
            owner: match f.get("owner")? {
                Node::Path(p) if p == "None" => None,
                Node::Call(p, args) if p == "Some" && args.len() == 1 => Some(int(&args[0])?),
                other => return Err(format!("generated table: bad owner {other:?}")),
            },
            action: self.action(f.get("action")?)?,
        })
    }
    fn ranges(&mut self, node: &Node) -> Result<&'static [(char, char)], String> {
        self.slice(node, |_, pair| match pair {
            Node::Tuple(items) => match items.as_slice() {
                [Node::Char(a), Node::Char(b)] => Ok((*a, *b)),
                _ => Err("generated table: bad character range".into()),
            },
            _ => Err("generated table: bad character range".into()),
        })
    }
    fn expr(&mut self, node: &Node) -> Result<Expr<'static>, String> {
        Ok(match node {
            Node::Path(p) if tail(p) == "Any" => Expr::Any,
            Node::Call(p, args) => match (tail(p), args.as_slice()) {
                ("Literal", [s]) => Expr::Literal(self.str(s)?),
                ("Class", [r]) => Expr::Class(self.ranges(r)?),
                ("NotClass", [r]) => Expr::NotClass(self.ranges(r)?),
                ("Ref", [id]) => Expr::Ref(int(id)?),
                ("Sequence", [items]) => Expr::Sequence(self.slice(items, Self::expr)?),
                ("Choice", [items]) => Expr::Choice(self.slice(items, Self::expr)?),
                ("Label", [label, inner]) => {
                    Expr::Label(self.str(label)?, self.reference(inner, Self::expr)?)
                }
                _ => return Err(format!("generated table: bad expression {p}")),
            },
            Node::Struct(p, _) => {
                let f = fields(node)?;
                match tail(p) {
                    "Predicate" => Expr::Predicate {
                        positive: boolean(f.get("positive")?)?,
                        inner: self.reference(f.get("inner")?, Self::expr)?,
                    },
                    "Repeat" => Expr::Repeat {
                        inner: self.reference(f.get("inner")?, Self::expr)?,
                        min: int(f.get("min")?)?,
                        max: int(f.get("max")?)?,
                    },
                    "Pratt" => Expr::Pratt {
                        prefixes: self.u16s(f.get("prefixes")?)?,
                        infixes: self.u16s(f.get("infixes")?)?,
                        memoize: boolean(f.get("memoize")?)?,
                    },
                    _ => return Err(format!("generated table: bad expression {p}")),
                }
            }
            other => return Err(format!("generated table: bad expression {other:?}")),
        })
    }
    fn action(&mut self, node: &Node) -> Result<Action<'static>, String> {
        if let Node::Path(p) = node {
            return match tail(p) {
                "None" => Ok(Action::None),
                _ => Err(format!("generated table: bad action {p}")),
            };
        }
        let Node::Struct(p, _) = node else {
            return Err(format!("generated table: bad action {node:?}"));
        };
        let f = fields(node)?;
        Ok(match tail(p) {
            "Literal" => Action::Literal {
                ty: self.str(f.get("ty")?)?,
                field: self.str(f.get("field")?)?,
            },
            "Reference" => Action::Reference {
                kinds: self.strs(f.get("kinds")?)?,
                field: self.str(f.get("field")?)?,
                values: self.slice(f.get("values")?, Self::builtin_value)?,
            },
            "Operation" => Action::Operation {
                id: int(f.get("id")?)?,
                fields: self.strs(f.get("fields")?)?,
                texts: self.slice(f.get("texts")?, |_, n| boolean(n))?,
            },
            "Overload" => Action::Overload {
                candidates: self.u16s(f.get("candidates")?)?,
                fields: self.strs(f.get("fields")?)?,
            },
            "Call" => Action::Call {
                kinds: self.strs(f.get("kinds")?)?,
                field: self.str(f.get("field")?)?,
                fields: self.strs(f.get("fields")?)?,
                builtins: self.slice(f.get("builtins")?, Self::builtin)?,
            },
            "Template" => Action::Template {
                kind: self.str(f.get("kind")?)?,
                field: self.str(f.get("field")?)?,
                body: self.str(f.get("body")?)?,
                parameters: self.str(f.get("parameters")?)?,
            },
            "Parameter" => Action::Parameter {
                kind: self.str(f.get("kind")?)?,
                field: self.str(f.get("field")?)?,
                annotation: self.str(f.get("annotation")?)?,
            },
            "Alias" => Action::Alias {
                kind: self.str(f.get("kind")?)?,
                field: self.str(f.get("field")?)?,
            },
            "Expand" => Action::Expand {
                kinds: self.strs(f.get("kinds")?)?,
                field: self.str(f.get("field")?)?,
                arguments: self.str(f.get("arguments")?)?,
            },
            "Declare" => Action::Declare {
                kind: self.str(f.get("kind")?)?,
                field: self.str(f.get("field")?)?,
                annotation: match f.get("annotation")? {
                    Node::Path(p) if p == "None" => None,
                    Node::Call(p, args) if p == "Some" && args.len() == 1 => {
                        Some(self.str(&args[0])?)
                    }
                    other => return Err(format!("generated table: bad annotation {other:?}")),
                },
                value: self.str(f.get("value")?)?,
            },
            "Show" => Action::Show {
                field: self.str(f.get("field")?)?,
            },
            "Rewrite" => Action::Rewrite {
                helper: int(f.get("helper")?)?,
                fields: self.strs(f.get("fields")?)?,
                name: self.str(f.get("name")?)?,
            },
            other => return Err(format!("generated table: bad action {other}")),
        })
    }
}
