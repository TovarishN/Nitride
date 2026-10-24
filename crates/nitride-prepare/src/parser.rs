use crate::{
    PrepareError, Source,
    lexer::{Kind, Lexer, Token},
};
use serde_json::{Value, json};

pub struct Module {
    pub name: String,
    pub imports: Vec<String>,
    pub symbols: Vec<String>,
    pub builtins: Vec<(String, String)>,
    pub rules: Vec<Rule>,
    pub extensions: Vec<(String, Vec<Rule>)>,
    pub source: usize,
    pub span: [usize; 2],
}
#[derive(Clone)]
pub struct Rule {
    pub name: String,
    pub body: Value,
    pub lexical: bool,
    pub clauses: Vec<Value>,
    pub block: bool,
    pub except: Vec<String>,
    pub precedence: Option<u32>,
    pub associativity: Option<String>,
    pub alternatives: Option<Vec<Rule>>,
    pub span: [usize; 2],
}

pub fn parse(source: &Source, index: usize) -> Result<Vec<Module>, PrepareError> {
    let mut lexer = Lexer {
        source,
        position: 0,
    };
    let current = lexer.next()?;
    Parser {
        lexer,
        current,
        depth: 0,
    }
    .modules(index)
}
struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    depth: usize,
}
impl Parser<'_> {
    fn error(&self, message: impl Into<String>) -> PrepareError {
        self.lexer.source.error(self.current.span, message)
    }
    fn at(&self, text: &str) -> bool {
        self.current.text == text
            && self.current.kind != Kind::String
            && self.current.kind != Kind::Char
    }
    fn advance(&mut self) -> Result<Token, PrepareError> {
        let next = self.lexer.next()?;
        Ok(std::mem::replace(&mut self.current, next))
    }
    fn take(&mut self, text: &str) -> Result<bool, PrepareError> {
        if self.at(text) {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn expect(&mut self, text: &str) -> Result<(), PrepareError> {
        if self.take(text)? {
            Ok(())
        } else {
            Err(self.error(format!("expected '{text}'")))
        }
    }
    fn name(&mut self, qualified: bool) -> Result<String, PrepareError> {
        if self.current.kind != Kind::Name || (!qualified && self.current.text.contains('.')) {
            return Err(self.error("expected name"));
        }
        Ok(self.advance()?.text)
    }
    fn modules(&mut self, index: usize) -> Result<Vec<Module>, PrepareError> {
        let mut modules = Vec::new();
        while self.current.kind != Kind::End {
            let start = self.current.span[0];
            self.expect("syntax")?;
            self.expect("module")?;
            let name = self.name(true)?;
            self.expect("{")?;
            let mut module = Module {
                name,
                imports: Vec::new(),
                symbols: Vec::new(),
                builtins: Vec::new(),
                rules: Vec::new(),
                extensions: Vec::new(),
                source: index,
                span: [start, start],
            };
            while !self.at("}") {
                if self.take("using")? {
                    module.imports.push(self.name(true)?);
                    self.expect(";")?;
                } else if self.take("symbols")? {
                    self.expect("{")?;
                    let count = module.symbols.len();
                    while !self.at("}") {
                        module.symbols.push(self.name(false)?);
                    }
                    if count == module.symbols.len() {
                        return Err(self.error("expected symbol kind"));
                    }
                    self.expect("}")?;
                } else if self.take("builtin")? {
                    let kind = self.name(false)?;
                    if self.at("in") {
                        return Err(self.error("scoped builtins are not admitted"));
                    }
                    self.expect("{")?;
                    let count = module.builtins.len();
                    while !self.at("}") {
                        module.builtins.push((kind.clone(), self.name(true)?));
                    }
                    if count == module.builtins.len() {
                        return Err(self.error("expected builtin name"));
                    }
                    self.expect("}")?;
                } else if self.take("token")? {
                    let mut rule = self.rule(false, true)?;
                    if self.take("except")? {
                        if self.current.kind != Kind::String {
                            return Err(self.error("expected excluded literal"));
                        }
                        while self.current.kind == Kind::String {
                            let token = self.advance()?;
                            if token.text.is_empty() {
                                return Err(self.error("empty excluded literal"));
                            }
                            rule.except.push(token.text);
                        }
                    }
                    self.expect(";")?;
                    module.rules.push(rule);
                } else if self.take("syntax")? {
                    let rule = self.rule(false, false)?;
                    if !rule.block {
                        self.expect(";")?;
                    }
                    module.rules.push(rule);
                } else if self.take("extensible")? {
                    self.expect("syntax")?;
                    module.rules.push(self.rule(true, false)?);
                } else if self.take("extend")? {
                    self.expect("syntax")?;
                    let target = self.name(true)?;
                    let alternatives = self.alternatives()?;
                    module.extensions.push((target, alternatives));
                } else {
                    return Err(self.error("unsupported or malformed module declaration"));
                }
            }
            module.span[1] = self.current.span[1];
            self.advance()?;
            modules.push(module);
        }
        Ok(modules)
    }
    fn rule(&mut self, extensible: bool, lexical: bool) -> Result<Rule, PrepareError> {
        let start = self.current.span[0];
        let name = self.name(false)?;
        let mut rule = Rule {
            name,
            body: Value::Null,
            lexical,
            clauses: Vec::new(),
            block: false,
            except: Vec::new(),
            precedence: None,
            associativity: None,
            alternatives: None,
            span: [start, start],
        };
        if extensible {
            rule.alternatives = Some(self.alternatives()?);
        } else {
            self.expect("=")?;
            rule.body = self.expression()?;
            if !lexical {
                rule.clauses = self.clauses()?;
                rule.block = self.block()?;
            }
        }
        rule.span[1] = self.current.span[0];
        Ok(rule)
    }
    fn alternatives(&mut self) -> Result<Vec<Rule>, PrepareError> {
        self.expect("{")?;
        let mut rules = Vec::new();
        while self.take("|")? {
            let start = self.current.span[0];
            let mut peek = self.lexer.clone();
            let name = if self.current.kind == Kind::Name && peek.next()?.text == "=" {
                let n = self.name(false)?;
                self.expect("=")?;
                Some(n)
            } else {
                None
            };
            let body = self.expression()?;
            let name = match name {
                Some(n) => n,
                None if body["kind"] == "ref" => body["name"]
                    .as_str()
                    .unwrap()
                    .rsplit('.')
                    .next()
                    .unwrap()
                    .into(),
                _ => return Err(self.error("unnamed or transparent alternative is not admitted")),
            };
            let precedence = if self.take("precedence")? {
                if self.current.kind != Kind::Number {
                    return Err(self.error("expected precedence number"));
                }
                let value = self
                    .advance()?
                    .text
                    .parse::<u32>()
                    .map_err(|_| self.error("precedence too large"))?;
                if value > 255 {
                    return Err(self.error("precedence outside 0..255"));
                }
                Some(value)
            } else {
                None
            };
            let associativity = if self.take("left")? {
                Some("Left".into())
            } else if self.take("right")? {
                Some("Right".into())
            } else {
                None
            };
            if associativity.is_some() && precedence.is_none() {
                return Err(self.error("associativity requires precedence"));
            }
            let clauses = self.clauses()?;
            let block = self.block()?;
            rules.push(Rule {
                name,
                body,
                lexical: false,
                clauses,
                block,
                except: Vec::new(),
                precedence,
                associativity,
                alternatives: None,
                span: [start, self.current.span[0]],
            });
        }
        self.expect("}")?;
        Ok(rules)
    }
    fn block(&mut self) -> Result<bool, PrepareError> {
        if !self.at("{") {
            return Ok(false);
        }
        self.lexer.opaque_block(self.current.span[0])?;
        self.current = self.lexer.next()?;
        Ok(true)
    }
    fn element_start(&self) -> bool {
        match self.current.kind {
            Kind::String => true,
            Kind::Name => !matches!(
                self.current.text.as_str(),
                "precedence"
                    | "except"
                    | "then"
                    | "else"
                    | "declares"
                    | "references"
                    | "scope"
                    | "dynamic"
                    | "lowers"
                    | "rewrites"
            ),
            _ => matches!(self.current.text.as_str(), "[" | "(" | "." | "!" | "&"),
        }
    }
    fn expression(&mut self) -> Result<Value, PrepareError> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error("grammar nesting exceeds 128"));
        }
        let start = self.current.span[0];
        let first = self.sequence()?;
        let mut items = vec![first];
        while self.take("/")? {
            items.push(self.sequence()?);
        }
        self.depth -= 1;
        if items.len() == 1 {
            Ok(items.pop().unwrap())
        } else {
            Ok(json!({"kind":"choice","items":items,"_span":[start,self.current.span[0]]}))
        }
    }
    fn sequence(&mut self) -> Result<Value, PrepareError> {
        let start = self.current.span[0];
        let mut items = vec![self.element()?];
        while self.element_start() {
            items.push(self.element()?);
        }
        if items.len() == 1 {
            Ok(items.pop().unwrap())
        } else {
            Ok(json!({"kind":"sequence","items":items,"_span":[start,self.current.span[0]]}))
        }
    }
    fn element(&mut self) -> Result<Value, PrepareError> {
        let start = self.current.span[0];
        let mut peek = self.lexer.clone();
        if self.current.kind == Kind::Name
            && !self.current.text.contains('.')
            && peek.next()?.text == ":"
        {
            let label = self.name(false)?;
            self.expect(":")?;
            let inner = self.unary()?;
            return Ok(
                json!({"kind":"label","label":label,"inner":inner,"_span":[start,self.current.span[0]]}),
            );
        }
        self.unary()
    }
    fn unary(&mut self) -> Result<Value, PrepareError> {
        let original_depth = self.depth;
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error("grammar nesting exceeds 128"));
        }
        let token = self.current.clone();
        let result = if self.at("!") || self.at("&") {
            self.advance()?;
            let inner = self.unary()?;
            json!({"kind":"predicate","positive":token.text=="&","inner":inner,"_span":token.span})
        } else {
            let mut expr = self.primary()?;
            while self.at("?") || self.at("*") || self.at("+") {
                let op = self.advance()?.text;
                expr = json!({"kind":"repeat","min":if op=="+"{1}else{0},"max":if op=="?"{1}else{i32::MAX},"inner":expr,"_span":token.span});
                // Bound nested postfix forms too; they recurse during validation.
                self.depth += 1;
                if self.depth > 128 {
                    return Err(self.error("grammar nesting exceeds 128"));
                }
            }
            expr
        };
        // expression()/unary() depth tracks recursive calls; postfix wrappers are
        // separately bounded above, then the caller's level is restored.
        self.depth = original_depth;
        Ok(result)
    }
    fn primary(&mut self) -> Result<Value, PrepareError> {
        let token = self.current.clone();
        let mut result = match token.kind {
            Kind::String => {
                self.advance()?;
                if token.text.is_empty() {
                    return Err(self.error("empty literal is not admitted"));
                }
                json!({"kind":"literal","text":token.text})
            }
            Kind::Name if self.element_start() => {
                self.advance()?;
                if self.at("(") && self.current.span[0] == token.span[1] {
                    return Err(self.error("grammar function calls are not admitted"));
                }
                json!({"kind":"ref","name":token.text})
            }
            _ if self.take(".")? => json!({"kind":"any"}),
            _ if self.take("(")? => {
                let expr = self.expression()?;
                self.expect(")")?;
                return Ok(expr);
            }
            _ if self.take("[")? => {
                let negated = self.take("^")?;
                let mut ranges = Vec::new();
                while self.current.kind == Kind::Char {
                    let low = self.advance()?.text.chars().next().unwrap() as u32;
                    let high = if self.take("..")? {
                        if self.current.kind != Kind::Char {
                            return Err(self.error("expected character range bound"));
                        }
                        self.advance()?.text.chars().next().unwrap() as u32
                    } else {
                        low
                    };
                    if high < low {
                        return Err(self.error("empty character range"));
                    }
                    ranges.push([low, high]);
                }
                if ranges.is_empty() {
                    return Err(self.error("character class needs at least one character"));
                }
                self.expect("]")?;
                json!({"kind":"class","negated":negated,"ranges":ranges})
            }
            _ => return Err(self.error("expected grammar expression")),
        };
        result["_span"] = json!(token.span);
        Ok(result)
    }
    fn clauses(&mut self) -> Result<Vec<Value>, PrepareError> {
        let mut clauses = Vec::new();
        loop {
            let mut field = String::new();
            let mut target = None;
            let mut args = Vec::new();
            let mut kinds = Vec::new();
            let mut sequential = false;
            let kind = if self.take("declares")? {
                kinds.push(self.name(false)?);
                field = self.name(false)?;
                sequential = self.take("sequential")?;
                if !sequential {
                    return Err(self.error("only sequential declarations are admitted"));
                }
                if self.take("type")? {
                    target = Some(self.name(true)?);
                }
                "Declares"
            } else if self.take("references")? {
                if self.take("(")? {
                    kinds.push(self.name(false)?);
                    while self.take("|")? {
                        kinds.push(self.name(false)?);
                    }
                    self.expect(")")?;
                } else {
                    kinds.push(self.name(false)?);
                }
                field = self.name(false)?;
                "References"
            } else if self.take("lowers")? {
                if self.take("literal")? {
                    target = Some(self.name(true)?);
                    field = self.name(false)?;
                    "LowersLiteral"
                } else {
                    target = Some(self.name(true)?);
                    self.expect("(")?;
                    if !self.at(")") {
                        args.push(self.name(false)?);
                        while self.take(",")? {
                            args.push(self.name(false)?);
                        }
                    }
                    self.expect(")")?;
                    "Lowers"
                }
            } else {
                break;
            };
            clauses.push(json!({"kind":kind,"field":field,"target":target,"args":args,"symbol_kinds":kinds,"sequential":sequential}));
        }
        Ok(clauses)
    }
}
