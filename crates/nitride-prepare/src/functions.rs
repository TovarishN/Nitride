//! Grammar functions, evaluated when a grammar is prepared. This mirrors
//! Nitrogen's `GrammarFunctions` (registry, static checks, evaluation with
//! memoization and limits, token-choice factoring): every call in a rule body
//! is replaced by the ordinary expression it evaluates to, so later stages
//! never see a call. Produced rule references are module-qualified.
use crate::{
    PrepareError, Source,
    parser::{Function, Module},
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

const MAX_DEPTH: usize = 200;
const MAX_NODES: usize = 1_000_000;
const MAX_STEPS: i64 = 10_000_000;
const BUILTINS: [&str; 5] = ["char", "code", "upper", "lower", "length"];
const INT: &str = "Grammar.Int";
const CHAR: &str = "Grammar.Char";
const TEXT: &str = "Grammar.Text";
const CLASS: &str = "Grammar.Class";
const RULE: &str = "Grammar.Rule";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ty {
    Int,
    Bool,
    Char,
    Text,
    Class,
    Rule,
    RuleName,
}

fn describe(ty: Ty) -> &'static str {
    match ty {
        Ty::Int => INT,
        Ty::Bool => "a condition",
        Ty::Char => CHAR,
        Ty::Text => TEXT,
        Ty::Class => CLASS,
        Ty::Rule => RULE,
        Ty::RuleName => "a rule name",
    }
}

fn parameter_type(category: &str) -> Option<Ty> {
    match category {
        INT => Some(Ty::Int),
        CHAR => Some(Ty::Char),
        TEXT => Some(Ty::Text),
        CLASS => Some(Ty::Class),
        RULE => Some(Ty::Rule),
        _ => None,
    }
}

fn converts(from: Ty, to: Ty) -> bool {
    from == to
        || match to {
            Ty::Class => matches!(from, Ty::Char | Ty::RuleName),
            Ty::Rule => matches!(from, Ty::Char | Ty::Class | Ty::Text | Ty::RuleName),
            _ => false,
        }
}

/// UTF-16 code units as sorted, disjoint, non-adjacent inclusive ranges.
#[derive(Clone, PartialEq, Eq, Debug)]
struct CodeSet(Vec<(i64, i64)>);

impl CodeSet {
    fn of(ranges: impl IntoIterator<Item = (i64, i64)>) -> Self {
        let mut clipped: Vec<_> = ranges
            .into_iter()
            .map(|(a, b)| (a.max(0), b.min(0xFFFF)))
            .filter(|(a, b)| a <= b)
            .collect();
        clipped.sort_by_key(|r| r.0);
        let mut merged: Vec<(i64, i64)> = Vec::new();
        for (a, b) in clipped {
            match merged.last_mut() {
                Some(last) if a <= last.1 + 1 => last.1 = last.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        CodeSet(merged)
    }
    fn unit(c: u16) -> Self {
        Self::of([(c as i64, c as i64)])
    }
    fn of_class(expr: &Value) -> Self {
        let set = Self::of(
            expr["ranges"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| (r[0].as_i64().unwrap_or(0), r[1].as_i64().unwrap_or(0))),
        );
        if expr["negated"] == true {
            set.complement()
        } else {
            set
        }
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn union(&self, other: &Self) -> Self {
        Self::of(self.0.iter().chain(&other.0).copied())
    }
    fn complement(&self) -> Self {
        let mut result = Vec::new();
        let mut next = 0;
        for &(a, b) in &self.0 {
            if a > next {
                result.push((next, a - 1));
            }
            next = b + 1;
        }
        result.push((next, 0xFFFF));
        Self::of(result)
    }
    fn intersect(&self, other: &Self) -> Self {
        self.complement().union(&other.complement()).complement()
    }
    fn except(&self, other: &Self) -> Self {
        self.intersect(&other.complement())
    }
    fn shift(&self, offset: i64) -> Self {
        Self::of(self.0.iter().map(|(a, b)| (a + offset, b + offset)))
    }
    fn key(&self) -> String {
        self.0
            .iter()
            .map(|(a, b)| format!("{a:x}-{b:x},"))
            .collect()
    }
    fn to_class(&self) -> Value {
        json!({"kind":"class","negated":false,"ranges":self.0.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>()})
    }
}

#[derive(Clone, Debug)]
enum Val {
    Int(i64),
    Bool(bool),
    Char(u16),
    Text(Vec<u16>),
    Class(CodeSet),
    Rule(Value),
    RuleName(String),
}

impl Val {
    fn key(&self) -> String {
        match self {
            Val::Int(n) => format!("i{n}"),
            Val::Bool(b) => if *b { "t" } else { "f" }.into(),
            Val::Char(c) => format!("c{c}"),
            Val::Text(t) => format!("s{}:{}", t.len(), String::from_utf16_lossy(t)),
            Val::Class(set) => format!("k{}", set.key()),
            Val::Rule(rule) => format!("r{rule}"),
            Val::RuleName(name) => format!("n{name}"),
        }
    }
    fn same_kind(&self, other: &Val) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// A failed evaluation: an error to report, or one already reported.
enum Failure {
    Reported,
    Error(String),
}

type Outcome<T> = Result<T, Failure>;

struct Registry<'a> {
    modules: &'a [Module],
    functions: HashMap<String, HashMap<String, (usize, &'a Function)>>,
    rules: HashMap<String, (usize, usize)>,
    valid: HashSet<(usize, String)>,
}

impl<'a> Registry<'a> {
    /// A function by name from `from`: qualified, its own, or exactly one of its `using`s.
    fn resolve(&self, name: &str, from: &Module) -> Result<(usize, &'a Function), String> {
        if let Some(dot) = name.rfind('.') {
            return self
                .functions
                .get(&name[..dot])
                .and_then(|f| f.get(&name[dot + 1..]))
                .copied()
                .ok_or_else(|| format!("unknown function '{name}'"));
        }
        if let Some(found) = self.functions.get(&from.name).and_then(|f| f.get(name)) {
            return Ok(*found);
        }
        let mut seen = HashSet::new();
        let candidates: Vec<_> = from
            .imports
            .iter()
            .filter(|m| **m != from.name && seen.insert(m.as_str()))
            .filter_map(|m| self.functions.get(m).and_then(|f| f.get(name)))
            .collect();
        match candidates.as_slice() {
            [one] => Ok(**one),
            [first, second, ..] => Err(format!(
                "'{name}' is defined in both '{}' and '{}'",
                self.modules[first.0].name, self.modules[second.0].name
            )),
            [] => Err(format!("unknown function '{name}'")),
        }
    }
    /// A rule's qualified name, resolved like a reference written in `from`.
    fn resolve_rule(&self, name: &str, from: &Module) -> Option<String> {
        if name.contains('.') && self.rules.contains_key(name) {
            return Some(name.into());
        }
        let local = format!("{}.{}", from.name, name);
        if self.rules.contains_key(&local) {
            return Some(local);
        }
        let candidates: HashSet<_> = from
            .imports
            .iter()
            .filter(|m| **m != from.name)
            .map(|m| format!("{m}.{name}"))
            .filter(|n| self.rules.contains_key(n))
            .collect();
        (candidates.len() == 1).then(|| candidates.into_iter().next().unwrap())
    }
}

fn span(value: &Value) -> [usize; 2] {
    [
        value["_span"][0].as_u64().unwrap_or(0) as usize,
        value["_span"][1].as_u64().unwrap_or(0) as usize,
    ]
}

/// Expands every grammar function call in the modules' rule bodies.
pub fn expand(modules: &mut [Module], sources: &[Source]) -> Result<(), PrepareError> {
    let has_calls = modules.iter().any(|m| {
        !m.functions.is_empty()
            || m.rules.iter().any(|r| rule_has(r, "call"))
            || m.extensions
                .iter()
                .any(|(_, a)| a.iter().any(|r| rule_has(r, "call")))
    });
    if has_calls {
        let expansions = evaluate(modules, sources)?;
        for (mi, module) in modules.iter_mut().enumerate() {
            for (ri, rule) in module.rules.iter_mut().enumerate() {
                apply(rule, &expansions, (mi, Slot::Rule(ri)));
            }
            for (ei, (_, alternatives)) in module.extensions.iter_mut().enumerate() {
                for (ai, alternative) in alternatives.iter_mut().enumerate() {
                    if let Some(body) = expansions.get(&(mi, Slot::Extension(ei, ai))) {
                        alternative.body = body.clone();
                    }
                }
            }
        }
    }
    // Function-only forms outside a function body are not grammar.
    for module in modules.iter() {
        for rule in module
            .rules
            .iter()
            .chain(module.extensions.iter().flat_map(|(_, a)| a))
        {
            for kind in ["if", "comprehension", "valueclass", "call"] {
                if rule_has(rule, kind) {
                    return Err(sources[module.source].error(
                        rule.span,
                        "this expression can appear only in a grammar function",
                    ));
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Slot {
    Rule(usize),
    Alternative(usize, usize),
    Extension(usize, usize),
}

fn apply(
    rule: &mut crate::parser::Rule,
    expansions: &HashMap<(usize, Slot), Value>,
    key: (usize, Slot),
) {
    if let Some(body) = expansions.get(&key) {
        rule.body = body.clone();
    }
    if let (Slot::Rule(ri), Some(alternatives)) = (key.1, rule.alternatives.as_mut()) {
        for (ai, alternative) in alternatives.iter_mut().enumerate() {
            if let Some(body) = expansions.get(&(key.0, Slot::Alternative(ri, ai))) {
                alternative.body = body.clone();
            }
        }
    }
}

fn has(expr: &Value, kind: &str) -> bool {
    expr["kind"] == kind
        || expr["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| has(i, kind)))
        || expr.get("inner").is_some_and(|inner| has(inner, kind))
}

fn rule_has(rule: &crate::parser::Rule, kind: &str) -> bool {
    has(&rule.body, kind)
        || rule
            .alternatives
            .iter()
            .flatten()
            .any(|a| has(&a.body, kind))
}

/// Checks every function, then evaluates every call site. Returns the new
/// bodies of the rules that contained calls.
fn evaluate(
    modules: &[Module],
    sources: &[Source],
) -> Result<HashMap<(usize, Slot), Value>, PrepareError> {
    let mut registry = Registry {
        modules,
        functions: HashMap::new(),
        rules: HashMap::new(),
        valid: HashSet::new(),
    };
    for (mi, module) in modules.iter().enumerate() {
        for (ri, rule) in module.rules.iter().enumerate() {
            registry
                .rules
                .entry(format!("{}.{}", module.name, rule.name))
                .or_insert((mi, ri));
        }
    }
    for (mi, module) in modules.iter().enumerate() {
        if registry.functions.contains_key(&module.name) {
            continue;
        }
        let mut functions = HashMap::new();
        for function in &module.functions {
            let error = |message: String| sources[module.source].error(function.span, message);
            if functions.contains_key(&function.name)
                || module.helpers.iter().any(|h| h.name == function.name)
            {
                return Err(error(format!(
                    "'{}' is already defined in module '{}'",
                    function.name, module.name
                )));
            }
            if BUILTINS.contains(&function.name.as_str()) {
                return Err(error(format!("'{}' is a built-in function", function.name)));
            }
            functions.insert(function.name.clone(), (mi, function));
        }
        registry.functions.insert(module.name.clone(), functions);
    }
    // Every function body is checked once, used or not.
    let mut errors = Vec::new();
    let all: Vec<_> = registry
        .functions
        .values()
        .flat_map(|f| f.values().copied())
        .collect();
    for (mi, function) in &all {
        let before = errors.len();
        Checker {
            registry: &registry,
            module: &modules[*mi],
            errors: &mut errors,
        }
        .check_function(function);
        if errors.len() == before {
            registry.valid.insert((*mi, function.name.clone()));
        }
    }
    if let Some((mi, at, message)) = errors.first() {
        return Err(sources[modules[*mi].source].error(*at, message.clone()));
    }
    let mut evaluator = Evaluator {
        registry: &registry,
        memo: HashMap::new(),
        stack: Vec::new(),
        nodes: 0,
        steps: 0,
    };
    let mut result = HashMap::new();
    for (mi, module) in modules.iter().enumerate() {
        let mut inline = |body: &Value, lexical: bool| -> Result<Option<Value>, PrepareError> {
            if !has(body, "call") {
                return Ok(None);
            }
            inline(body, lexical, &mut evaluator, module, mi, sources).map(Some)
        };
        for (ri, rule) in module.rules.iter().enumerate() {
            if let Some(body) = inline(&rule.body, rule.lexical)? {
                result.insert((mi, Slot::Rule(ri)), body);
            }
            for (ai, alternative) in rule.alternatives.iter().flatten().enumerate() {
                if let Some(body) = inline(&alternative.body, false)? {
                    result.insert((mi, Slot::Alternative(ri, ai)), body);
                }
            }
        }
        for (ei, (_, alternatives)) in module.extensions.iter().enumerate() {
            for (ai, alternative) in alternatives.iter().enumerate() {
                if let Some(body) = inline(&alternative.body, false)? {
                    result.insert((mi, Slot::Extension(ei, ai)), body);
                }
            }
        }
    }
    Ok(result)
}

/// Replaces each call in a rule body by its expansion, bottom-up.
fn inline(
    expr: &Value,
    lexical: bool,
    evaluator: &mut Evaluator,
    module: &Module,
    mi: usize,
    sources: &[Source],
) -> Result<Value, PrepareError> {
    if expr["kind"] == "call" {
        let error = |message: String| sources[module.source].error(span(expr), message);
        let mut site_errors = Vec::new();
        Checker {
            registry: evaluator.registry,
            module,
            errors: &mut site_errors,
        }
        .check_call(expr, &mut Vec::new());
        if let Some((_, _, message)) = site_errors.first() {
            return Err(error(message.clone()));
        }
        evaluator.nodes = 0;
        evaluator.steps = 0;
        let built = evaluator.build(expr, &mut Vec::new(), mi);
        evaluator.stack.clear();
        return match built {
            Ok(result) => Ok(if lexical { factor(&result) } else { result }),
            Err(Failure::Error(message)) => Err(error(message)),
            Err(Failure::Reported) => Err(error("the called function is invalid".into())),
        };
    }
    let mut out = expr.clone();
    if let Some(items) = out.get_mut("items").and_then(Value::as_array_mut) {
        for item in items.iter_mut() {
            *item = inline(item, lexical, evaluator, module, mi, sources)?;
        }
    }
    if let Some(inner) = expr.get("inner") {
        out["inner"] = inline(inner, lexical, evaluator, module, mi, sources)?;
    }
    Ok(out)
}

type Scope<T> = Vec<(String, T)>;

fn find<'s, T>(scope: &'s Scope<T>, name: &str) -> Option<&'s T> {
    scope.iter().rev().find(|(n, _)| n == name).map(|(_, t)| t)
}

struct Checker<'r, 'a> {
    registry: &'r Registry<'a>,
    module: &'a Module,
    errors: &'r mut Vec<(usize, [usize; 2], String)>,
}

impl Checker<'_, '_> {
    fn report(&mut self, at: [usize; 2], message: String) {
        let mi = self
            .registry
            .modules
            .iter()
            .position(|m| std::ptr::eq(m, self.module))
            .unwrap_or(0);
        self.errors.push((mi, at, message));
    }

    fn unknown(name: &str) -> String {
        format!("'{name}' is not a parameter, a let, a variable or a rule")
    }

    fn check_function(&mut self, function: &Function) {
        let mut scope = Vec::new();
        let mut seen = HashSet::new();
        for parameter in &function.parameters {
            if !seen.insert(parameter.name.clone()) {
                self.report(
                    parameter.span,
                    format!("parameter '{}' is declared twice", parameter.name),
                );
            }
            match parameter_type(&parameter.category) {
                Some(ty) => scope.push((parameter.name.clone(), ty)),
                None => self.report(
                    parameter.span,
                    "a grammar function parameter is Grammar.Int, Grammar.Char, Grammar.Text, Grammar.Class or Grammar.Rule".into(),
                ),
            }
        }
        if function.result.0 != RULE {
            self.report(
                function.result.1,
                "a function with a grammar body must return Grammar.Rule".into(),
            );
        }
        for (name, value) in &function.lets {
            let ty = self.type_of(value, &mut scope).unwrap_or(Ty::Rule);
            scope.push((name.clone(), ty));
        }
        self.check_rule(&function.body, &mut scope);
    }

    fn lets(&mut self, lets: &Value, scope: &mut Scope<Ty>) {
        for pair in lets.as_array().into_iter().flatten() {
            let ty = self.type_of(&pair[1], scope).unwrap_or(Ty::Rule);
            scope.push((pair[0].as_str().unwrap_or("").to_owned(), ty));
        }
    }

    fn check_rule(&mut self, expr: &Value, scope: &mut Scope<Ty>) {
        match expr["kind"].as_str() {
            Some("ref") => {
                let name = expr["name"].as_str().unwrap_or("");
                match find(scope, name) {
                    Some(ty) if !converts(*ty, Ty::Rule) => {
                        let ty = *ty;
                        self.report(
                            span(expr),
                            format!("'{name}' is {}, not a rule", describe(ty)),
                        )
                    }
                    Some(_) => {}
                    None if self.registry.resolve_rule(name, self.module).is_none() => {
                        self.report(span(expr), Self::unknown(name))
                    }
                    None => {}
                }
            }
            Some("call") => {
                self.check_call(expr, scope);
            }
            Some("if") => {
                self.expect(&expr["cond"], Ty::Bool, scope);
                self.check_rule(&expr["then"], scope);
                self.check_rule(&expr["else"], scope);
            }
            Some("comprehension") => {
                let variable = if expr["to"].is_null() {
                    self.expect(&expr["from"], Ty::Text, scope);
                    Ty::Char
                } else {
                    self.expect(&expr["from"], Ty::Int, scope);
                    self.expect(&expr["to"], Ty::Int, scope);
                    Ty::Int
                };
                let depth = scope.len();
                scope.push((expr["var"].as_str().unwrap_or("").to_owned(), variable));
                self.check_block(&expr["block"], scope);
                scope.truncate(depth);
            }
            Some("valueclass") => self.check_items(&expr["items"], scope),
            _ => {
                for item in expr["items"].as_array().into_iter().flatten() {
                    self.check_rule(item, scope);
                }
                if let Some(inner) = expr.get("inner") {
                    self.check_rule(inner, scope);
                }
            }
        }
    }

    fn check_block(&mut self, block: &Value, scope: &mut Scope<Ty>) {
        let depth = scope.len();
        self.lets(&block["lets"], scope);
        if !block["where"].is_null() {
            self.expect(&block["where"], Ty::Bool, scope);
        }
        self.check_rule(&block["body"], scope);
        scope.truncate(depth);
    }

    fn check_items(&mut self, items: &Value, scope: &mut Scope<Ty>) {
        for item in items.as_array().into_iter().flatten() {
            let first = self.type_of(&item["first"], scope);
            if !item["last"].is_null() {
                self.check_bound(&item["first"], first);
                let last = self.type_of(&item["last"], scope);
                self.check_bound(&item["last"], last);
            } else if let Some(single) = first {
                if single != Ty::Int && !converts(single, Ty::Class) {
                    self.report(
                        span(&item["first"]),
                        format!(
                            "a class item is {CHAR}, {INT} or {CLASS}, found {}",
                            describe(single)
                        ),
                    );
                }
            }
        }
    }

    fn check_bound(&mut self, bound: &Value, ty: Option<Ty>) {
        if let Some(known) = ty.filter(|t| !matches!(t, Ty::Int | Ty::Char)) {
            self.report(
                span(bound),
                format!(
                    "a range bound is {CHAR} or {INT}, found {}",
                    describe(known)
                ),
            );
        }
    }

    fn expect(&mut self, value: &Value, expected: Ty, scope: &mut Scope<Ty>) {
        if let Some(actual) = self.type_of(value, scope) {
            if !converts(actual, expected) {
                self.report(
                    span(value),
                    format!(
                        "expected {}, found {}",
                        describe(expected),
                        describe(actual)
                    ),
                );
            }
        }
    }

    fn type_of(&mut self, value: &Value, scope: &mut Scope<Ty>) -> Option<Ty> {
        match value["v"].as_str()? {
            "int" => Some(Ty::Int),
            "char" => Some(Ty::Char),
            "text" => Some(Ty::Text),
            "name" => {
                let name = value["name"].as_str().unwrap_or("");
                if let Some(ty) = find(scope, name) {
                    return Some(*ty);
                }
                if self.registry.resolve_rule(name, self.module).is_some() {
                    return Some(Ty::RuleName);
                }
                self.report(span(value), Self::unknown(name));
                None
            }
            "call" => self.check_call(value, scope),
            "block" => {
                self.check_block(&value["block"], scope);
                Some(Ty::Rule)
            }
            "class" => {
                self.check_items(&value["items"], scope);
                Some(Ty::Class)
            }
            "if" => {
                self.expect(&value["cond"], Ty::Bool, scope);
                let then = self.type_of(&value["then"], scope);
                let otherwise = self.type_of(&value["else"], scope);
                let (a, b) = (then?, otherwise?);
                let unified = if a == b {
                    Some(a)
                } else if converts(a, Ty::Class) && converts(b, Ty::Class) {
                    Some(Ty::Class)
                } else if converts(a, Ty::Rule) && converts(b, Ty::Rule) {
                    Some(Ty::Rule)
                } else {
                    None
                };
                if unified.is_none() {
                    self.report(
                        span(value),
                        format!("the branches are {} and {}", describe(a), describe(b)),
                    );
                }
                unified
            }
            "unary" => {
                let expected = match value["op"].as_str() {
                    Some("-") => Ty::Int,
                    Some("~") => Ty::Class,
                    _ => Ty::Bool,
                };
                self.expect(&value["operand"], expected, scope);
                Some(expected)
            }
            "binary" => {
                let left = self.type_of(&value["left"], scope);
                let right = self.type_of(&value["right"], scope);
                let (l, r) = (left?, right?);
                let op = value["op"].as_str().unwrap_or("");
                let class = |t: Ty| converts(t, Ty::Class);
                let result = match op {
                    "or" | "and" => (l == Ty::Bool && r == Ty::Bool).then_some(Ty::Bool),
                    "==" | "!=" => (l == r && l != Ty::Rule && l != Ty::RuleName
                        || class(l) && class(r) && l != Ty::Char && r != Ty::Char)
                        .then_some(Ty::Bool),
                    "<" | "<=" | ">" | ">=" => {
                        (l == r && matches!(l, Ty::Int | Ty::Char)).then_some(Ty::Bool)
                    }
                    "|" | "&" => (class(l) && class(r)).then_some(Ty::Class),
                    "+" | "-" => match (l, r) {
                        (Ty::Int, Ty::Int) => Some(Ty::Int),
                        (Ty::Char, Ty::Int) => Some(Ty::Char),
                        _ if class(l) && r == Ty::Int => Some(Ty::Class),
                        _ if op == "-" && class(l) && class(r) => Some(Ty::Class),
                        _ => None,
                    },
                    _ => (l == Ty::Int && r == Ty::Int).then_some(Ty::Int),
                };
                if result.is_none() {
                    self.report(
                        span(value),
                        format!(
                            "'{op}' does not apply to {} and {}",
                            describe(l),
                            describe(r)
                        ),
                    );
                }
                result
            }
            _ => None,
        }
    }

    fn check_call(&mut self, call: &Value, scope: &mut Scope<Ty>) -> Option<Ty> {
        let name = call["name"].as_str().unwrap_or("");
        let arguments = call["args"].as_array().cloned().unwrap_or_default();
        let types: Vec<_> = arguments.iter().map(|a| self.type_of(a, scope)).collect();
        if BUILTINS.contains(&name) {
            let (parameter, result) = match name {
                "char" => (Ty::Int, Ty::Char),
                "code" => (Ty::Char, Ty::Int),
                "length" => (Ty::Text, Ty::Int),
                _ => (Ty::Char, Ty::Char),
            };
            if arguments.len() != 1 {
                self.report(
                    span(call),
                    format!("'{name}' takes 1 argument, found {}", arguments.len()),
                );
                return None;
            }
            if let Some(actual) = types[0].filter(|t| !converts(*t, parameter)) {
                self.report(
                    span(&arguments[0]),
                    format!(
                        "'{name}' expects {}, found {}",
                        describe(parameter),
                        describe(actual)
                    ),
                );
            }
            return Some(result);
        }
        let (_, target) = match self.registry.resolve(name, self.module) {
            Ok(found) => found,
            Err(message) => {
                self.report(span(call), message);
                return None;
            }
        };
        if target.parameters.len() != arguments.len() {
            let count = target.parameters.len();
            self.report(
                span(call),
                format!(
                    "'{name}' takes {count} argument{}, found {}",
                    if count == 1 { "" } else { "s" },
                    arguments.len()
                ),
            );
            return Some(Ty::Rule);
        }
        for (i, parameter) in target.parameters.iter().enumerate() {
            if let (Some(actual), Some(expected)) = (types[i], parameter_type(&parameter.category))
            {
                if !converts(actual, expected) {
                    self.report(
                        span(&arguments[i]),
                        format!(
                            "argument '{}' of '{name}' expects {}, found {}",
                            parameter.name,
                            describe(expected),
                            describe(actual)
                        ),
                    );
                }
            }
        }
        Some(Ty::Rule)
    }
}

struct Evaluator<'r, 'a> {
    registry: &'r Registry<'a>,
    memo: HashMap<String, Value>,
    stack: Vec<String>,
    nodes: usize,
    steps: i64,
}

fn strip(expr: &Value) -> Value {
    let mut out = expr.clone();
    if let Some(object) = out.as_object_mut() {
        object.remove("_span");
    }
    if let Some(items) = out.get_mut("items").and_then(Value::as_array_mut) {
        for item in items.iter_mut() {
            *item = strip(item);
        }
    }
    if let Some(inner) = expr.get("inner") {
        out["inner"] = strip(inner);
    }
    out
}

fn sequence(mut items: Vec<Value>) -> Value {
    items.retain(|i| {
        !(i["kind"] == "sequence" && i["items"].as_array().is_some_and(|a| a.is_empty()))
    });
    if items.len() == 1 {
        items.pop().unwrap()
    } else {
        json!({"kind":"sequence","items":items})
    }
}

impl<'a> Evaluator<'_, 'a> {
    fn error(&self, message: impl Into<String>) -> Failure {
        let message = message.into();
        Failure::Error(if self.stack.is_empty() {
            message
        } else {
            format!("{message} (in {})", self.stack.join(" → "))
        })
    }

    fn module(&self, mi: usize) -> &'a Module {
        &self.registry.modules[mi]
    }

    fn call(
        &mut self,
        mi: usize,
        function: &'a Function,
        mut arguments: Vec<Val>,
    ) -> Outcome<Value> {
        if !self.registry.valid.contains(&(mi, function.name.clone())) {
            return Err(Failure::Reported);
        }
        let mut scope = Vec::new();
        for (i, parameter) in function.parameters.iter().enumerate() {
            let value = match parameter_type(&parameter.category) {
                Some(Ty::Class) => Val::Class(self.as_class(&arguments[i])?),
                Some(Ty::Rule) => Val::Rule(self.as_rule(&arguments[i])?),
                _ => arguments[i].clone(),
            };
            arguments[i] = value.clone();
            scope.push((parameter.name.clone(), value));
        }
        let key = format!(
            "{}.{}({})",
            self.module(mi).name,
            function.name,
            arguments.iter().map(Val::key).collect::<Vec<_>>().join(",")
        );
        if let Some(known) = self.memo.get(&key) {
            return Ok(known.clone());
        }
        if self.stack.len() >= MAX_DEPTH {
            return Err(self.error(format!(
                "grammar function calls nest deeper than {MAX_DEPTH}"
            )));
        }
        self.stack.push(function.name.clone());
        for (name, value) in &function.lets {
            let evaluated = self.eval(value, &mut scope, mi)?;
            scope.push((name.clone(), evaluated));
        }
        let result = self.build(&function.body, &mut scope, mi)?;
        self.stack.pop();
        self.memo.insert(key, result.clone());
        Ok(result)
    }

    fn target(&self, name: &str, mi: usize) -> Outcome<(usize, &'a Function)> {
        self.registry
            .resolve(name, self.module(mi))
            .map_err(|e| self.error(e))
    }

    fn build(&mut self, expr: &Value, scope: &mut Scope<Val>, mi: usize) -> Outcome<Value> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(self.error(format!("the expansion exceeds {MAX_NODES} nodes")));
        }
        Ok(match expr["kind"].as_str() {
            Some("literal" | "class" | "any") => strip(expr),
            Some("ref") => {
                let name = expr["name"].as_str().unwrap_or("");
                if let Some(value) = find(scope, name).cloned() {
                    return self.as_rule(&value);
                }
                let qualified = self
                    .registry
                    .resolve_rule(name, self.module(mi))
                    .ok_or_else(|| self.error(format!("unknown rule '{name}'")))?;
                json!({"kind":"ref","name":qualified})
            }
            Some("label") => {
                json!({"kind":"label","label":expr["label"],"inner":self.build(&expr["inner"], scope, mi)?})
            }
            Some("repeat") => {
                json!({"kind":"repeat","min":expr["min"],"max":expr["max"],"inner":self.build(&expr["inner"], scope, mi)?})
            }
            Some("predicate") => {
                json!({"kind":"predicate","positive":expr["positive"],"inner":self.build(&expr["inner"], scope, mi)?})
            }
            Some("sequence") => {
                let mut items = Vec::new();
                for item in expr["items"].as_array().into_iter().flatten() {
                    items.push(self.build(item, scope, mi)?);
                }
                sequence(items)
            }
            Some("choice") => {
                let mut items = Vec::new();
                for item in expr["items"].as_array().into_iter().flatten() {
                    items.push(self.build(item, scope, mi)?);
                }
                self.choice(items)?
            }
            Some("call") => {
                let (target_module, function) =
                    self.target(expr["name"].as_str().unwrap_or(""), mi)?;
                let mut arguments = Vec::new();
                for argument in expr["args"].as_array().into_iter().flatten() {
                    arguments.push(self.eval(argument, scope, mi)?);
                }
                self.call(target_module, function, arguments)?
            }
            Some("if") => {
                let condition = self.eval(&expr["cond"], scope, mi)?;
                let branch = if self.as_bool(&condition)? {
                    &expr["then"]
                } else {
                    &expr["else"]
                };
                self.build(branch, scope, mi)?
            }
            Some("comprehension") => {
                let elements = self.elements(expr, scope, mi)?;
                let mut items = Vec::new();
                for element in elements {
                    self.steps += 1;
                    if self.steps > MAX_STEPS {
                        return Err(
                            self.error(format!("comprehensions take more than {MAX_STEPS} steps"))
                        );
                    }
                    let depth = scope.len();
                    scope.push((expr["var"].as_str().unwrap_or("").to_owned(), element));
                    let item = self.block(&expr["block"], scope, mi);
                    scope.truncate(depth);
                    if let Some(item) = item? {
                        items.push(item);
                    }
                }
                if expr["choice"] == true {
                    self.choice(items)?
                } else {
                    sequence(items)
                }
            }
            Some("valueclass") => {
                let set = self.items(&expr["items"], expr["negated"] == true, scope, mi)?;
                if set.is_empty() {
                    return Err(self.error("an empty class matches nothing"));
                }
                set.to_class()
            }
            _ => return Err(self.error("this expression cannot appear in a grammar function")),
        })
    }

    fn choice(&self, mut items: Vec<Value>) -> Outcome<Value> {
        match items.len() {
            0 => Err(self.error("'choice' produced no alternatives")),
            1 => Ok(items.pop().unwrap()),
            _ => Ok(json!({"kind":"choice","items":items})),
        }
    }

    fn elements(&mut self, expr: &Value, scope: &mut Scope<Val>, mi: usize) -> Outcome<Vec<Val>> {
        let from = self.eval(&expr["from"], scope, mi)?;
        if expr["to"].is_null() {
            return Ok(self.as_text(&from)?.into_iter().map(Val::Char).collect());
        }
        let first = self.as_int(&from)?;
        let to = self.eval(&expr["to"], scope, mi)?;
        let last = self.as_int(&to)?;
        if last.saturating_sub(first) >= MAX_STEPS {
            return Err(self.error(format!("comprehensions take more than {MAX_STEPS} steps")));
        }
        Ok((first..=last).map(Val::Int).collect())
    }

    /// The block's rule, or None when its `where` condition is false.
    fn block(
        &mut self,
        block: &Value,
        scope: &mut Scope<Val>,
        mi: usize,
    ) -> Outcome<Option<Value>> {
        let depth = scope.len();
        let result = (|| {
            for pair in block["lets"].as_array().into_iter().flatten() {
                let value = self.eval(&pair[1], scope, mi)?;
                scope.push((pair[0].as_str().unwrap_or("").to_owned(), value));
            }
            if !block["where"].is_null() {
                let condition = self.eval(&block["where"], scope, mi)?;
                if !self.as_bool(&condition)? {
                    return Ok(None);
                }
            }
            self.build(&block["body"], scope, mi).map(Some)
        })();
        scope.truncate(depth);
        result
    }

    fn eval(&mut self, value: &Value, scope: &mut Scope<Val>, mi: usize) -> Outcome<Val> {
        Ok(match value["v"].as_str() {
            Some("int") => Val::Int(value["n"].as_i64().unwrap_or(0)),
            Some("char") => Val::Char(value["c"].as_u64().unwrap_or(0) as u16),
            Some("text") => Val::Text(value["s"].as_str().unwrap_or("").encode_utf16().collect()),
            Some("name") => {
                let name = value["name"].as_str().unwrap_or("");
                if let Some(bound) = find(scope, name) {
                    return Ok(bound.clone());
                }
                Val::RuleName(
                    self.registry
                        .resolve_rule(name, self.module(mi))
                        .ok_or_else(|| self.error(format!("unknown name '{name}'")))?,
                )
            }
            Some("call") => {
                let mut arguments = Vec::new();
                for argument in value["args"].as_array().into_iter().flatten() {
                    arguments.push(self.eval(argument, scope, mi)?);
                }
                match value["name"].as_str().unwrap_or("") {
                    "char" => self.char_of(self.as_int(&arguments[0])?)?,
                    "code" => Val::Int(self.as_char(&arguments[0])? as i64),
                    "upper" => Val::Char(case(self.as_char(&arguments[0])?, true)),
                    "lower" => Val::Char(case(self.as_char(&arguments[0])?, false)),
                    "length" => Val::Int(self.as_text(&arguments[0])?.len() as i64),
                    name => {
                        let (target_module, function) = self.target(name, mi)?;
                        Val::Rule(self.call(target_module, function, arguments)?)
                    }
                }
            }
            Some("block") => Val::Rule(
                self.block(&value["block"], scope, mi)?
                    .unwrap_or_else(|| json!({"kind":"sequence","items":[]})),
            ),
            Some("class") => {
                Val::Class(self.items(&value["items"], value["negated"] == true, scope, mi)?)
            }
            Some("if") => {
                let condition = self.eval(&value["cond"], scope, mi)?;
                let branch = if self.as_bool(&condition)? {
                    &value["then"]
                } else {
                    &value["else"]
                };
                self.eval(branch, scope, mi)?
            }
            Some("unary") => {
                let operand = self.eval(&value["operand"], scope, mi)?;
                match value["op"].as_str() {
                    Some("-") => Val::Int(
                        self.as_int(&operand)?
                            .checked_neg()
                            .ok_or_else(|| self.error("integer overflow"))?,
                    ),
                    Some("~") => Val::Class(self.as_class(&operand)?.complement()),
                    _ => Val::Bool(!self.as_bool(&operand)?),
                }
            }
            Some("binary") => self.binary(value, scope, mi)?,
            _ => return Err(self.error("unsupported value")),
        })
    }

    fn binary(&mut self, value: &Value, scope: &mut Scope<Val>, mi: usize) -> Outcome<Val> {
        let op = value["op"].as_str().unwrap_or("");
        let left = self.eval(&value["left"], scope, mi)?;
        match op {
            "or" => {
                return if self.as_bool(&left)? {
                    Ok(left)
                } else {
                    let right = self.eval(&value["right"], scope, mi)?;
                    Ok(Val::Bool(self.as_bool(&right)?))
                };
            }
            "and" => {
                return if !self.as_bool(&left)? {
                    Ok(left)
                } else {
                    let right = self.eval(&value["right"], scope, mi)?;
                    Ok(Val::Bool(self.as_bool(&right)?))
                };
            }
            _ => {}
        }
        let right = self.eval(&value["right"], scope, mi)?;
        let overflow = || self.error("integer overflow");
        Ok(match op {
            "==" => Val::Bool(self.same(&left, &right)?),
            "!=" => Val::Bool(!self.same(&left, &right)?),
            "<" | "<=" | ">" | ">=" => {
                let number = |v: &Val| match v {
                    Val::Char(c) => Ok(*c as i64),
                    other => self.as_int(other),
                };
                let (l, r) = (number(&left)?, number(&right)?);
                Val::Bool(match op {
                    "<" => l < r,
                    "<=" => l <= r,
                    ">" => l > r,
                    _ => l >= r,
                })
            }
            "|" => Val::Class(self.as_class(&left)?.union(&self.as_class(&right)?)),
            "&" => Val::Class(self.as_class(&left)?.intersect(&self.as_class(&right)?)),
            "+" | "-" => {
                let sign = if op == "+" { 1 } else { -1 };
                match (&left, &right) {
                    (Val::Int(l), Val::Int(r)) => Val::Int(
                        r.checked_mul(sign)
                            .and_then(|r| l.checked_add(r))
                            .ok_or_else(overflow)?,
                    ),
                    (Val::Char(c), Val::Int(offset)) => {
                        let moved = (*c as i64)
                            .checked_add(sign * offset)
                            .ok_or_else(overflow)?;
                        if (0..=0xFFFF).contains(&moved) {
                            Val::Char(moved as u16)
                        } else {
                            return Err(self.error("the character is outside U+0000–U+FFFF"));
                        }
                    }
                    (_, Val::Int(shift)) => Val::Class(
                        self.as_class(&left)?
                            .shift(shift.checked_mul(sign).ok_or_else(overflow)?),
                    ),
                    _ => Val::Class(self.as_class(&left)?.except(&self.as_class(&right)?)),
                }
            }
            _ => {
                let (l, r) = (self.as_int(&left)?, self.as_int(&right)?);
                Val::Int(match op {
                    "*" => l.checked_mul(r).ok_or_else(overflow)?,
                    "/" if r == 0 => return Err(self.error("division by zero")),
                    "/" => l.checked_div(r).ok_or_else(overflow)?,
                    "%" if r == 0 => return Err(self.error("division by zero")),
                    "%" => l.checked_rem(r).ok_or_else(overflow)?,
                    _ => {
                        if r < 0 {
                            return Err(self.error("a negative exponent"));
                        }
                        let mut result: i64 = 1;
                        for _ in 0..r {
                            result = result.checked_mul(l).ok_or_else(overflow)?;
                            if result == 0 || result == 1 && l == 1 {
                                break;
                            }
                        }
                        result
                    }
                })
            }
        })
    }

    fn char_of(&self, code: i64) -> Outcome<Val> {
        if (0..=0xFFFF).contains(&code) {
            Ok(Val::Char(code as u16))
        } else {
            Err(self.error(format!("char({code}) is outside U+0000–U+FFFF")))
        }
    }

    fn same(&self, left: &Val, right: &Val) -> Outcome<bool> {
        if left.same_kind(right) {
            Ok(left.key() == right.key())
        } else {
            Ok(self.as_class(left)?.key() == self.as_class(right)?.key())
        }
    }

    fn items(
        &mut self,
        items: &Value,
        negated: bool,
        scope: &mut Scope<Val>,
        mi: usize,
    ) -> Outcome<CodeSet> {
        let mut set = CodeSet(Vec::new());
        for item in items.as_array().into_iter().flatten() {
            let first = self.eval(&item["first"], scope, mi)?;
            if item["last"].is_null() {
                set = set.union(&match first {
                    Val::Int(code) => CodeSet::of([(code, code)]),
                    other => self.as_class(&other)?,
                });
            } else {
                let last = self.eval(&item["last"], scope, mi)?;
                set = set.union(&CodeSet::of([(self.bound(&first)?, self.bound(&last)?)]));
            }
        }
        Ok(if negated { set.complement() } else { set })
    }

    fn bound(&self, value: &Val) -> Outcome<i64> {
        match value {
            Val::Char(c) => Ok(*c as i64),
            other => self.as_int(other),
        }
    }

    fn as_int(&self, value: &Val) -> Outcome<i64> {
        match value {
            Val::Int(n) => Ok(*n),
            _ => Err(self.error(format!("expected {INT}"))),
        }
    }

    fn as_bool(&self, value: &Val) -> Outcome<bool> {
        match value {
            Val::Bool(b) => Ok(*b),
            _ => Err(self.error("expected a condition")),
        }
    }

    fn as_char(&self, value: &Val) -> Outcome<u16> {
        match value {
            Val::Char(c) => Ok(*c),
            _ => Err(self.error(format!("expected {CHAR}"))),
        }
    }

    fn as_text(&self, value: &Val) -> Outcome<Vec<u16>> {
        match value {
            Val::Text(t) => Ok(t.clone()),
            _ => Err(self.error(format!("expected {TEXT}"))),
        }
    }

    fn as_class(&self, value: &Val) -> Outcome<CodeSet> {
        match value {
            Val::Class(set) => Ok(set.clone()),
            Val::Char(c) => Ok(CodeSet::unit(*c)),
            Val::RuleName(name) => self.class_of_rule(name, &mut HashSet::new()),
            _ => Err(self.error(format!("expected {CLASS}"))),
        }
    }

    fn class_of_rule(&self, name: &str, visiting: &mut HashSet<String>) -> Outcome<CodeSet> {
        let &(mi, ri) = self
            .registry
            .rules
            .get(name)
            .ok_or_else(|| self.error(format!("unknown rule '{name}'")))?;
        let rule = &self.registry.modules[mi].rules[ri];
        if !rule.lexical {
            return Err(self.error(format!(
                "'{}' is not a character-class token rule",
                rule.name
            )));
        }
        if !visiting.insert(name.to_owned()) {
            return Err(self.error(format!("token rule '{}' refers to itself", rule.name)));
        }
        let result = self.class_of_body(&rule.body, mi, visiting);
        visiting.remove(name);
        result?.ok_or_else(|| {
            self.error(format!(
                "token rule '{}' is not a character class",
                rule.name
            ))
        })
    }

    fn class_of_body(
        &self,
        body: &Value,
        mi: usize,
        visiting: &mut HashSet<String>,
    ) -> Outcome<Option<CodeSet>> {
        match body["kind"].as_str() {
            Some("class") => Ok(Some(CodeSet::of_class(body))),
            Some("choice") => {
                let mut set = CodeSet(Vec::new());
                for item in body["items"].as_array().into_iter().flatten() {
                    match self.class_of_body(item, mi, visiting)? {
                        Some(part) => set = set.union(&part),
                        None => return Ok(None),
                    }
                }
                Ok(Some(set))
            }
            Some("ref") => match self
                .registry
                .resolve_rule(body["name"].as_str().unwrap_or(""), self.module(mi))
            {
                Some(target) => self.class_of_rule(&target, visiting).map(Some),
                None => Ok(None),
            },
            _ => Ok(None),
        }
    }

    fn as_rule(&self, value: &Val) -> Outcome<Value> {
        match value {
            Val::Rule(rule) => Ok(rule.clone()),
            Val::RuleName(name) => Ok(json!({"kind":"ref","name":name})),
            Val::Char(c) => Ok(CodeSet::unit(*c).to_class()),
            Val::Class(set) if set.is_empty() => Err(self.error("an empty class matches nothing")),
            Val::Class(set) => Ok(set.to_class()),
            Val::Text(text) if text.is_empty() => Ok(json!({"kind":"sequence","items":[]})),
            Val::Text(text) => Ok(json!({"kind":"literal","text":String::from_utf16(text)
                .map_err(|_| self.error("unpaired UTF-16 surrogate in text"))?})),
            _ => Err(self.error(format!("expected {RULE}"))),
        }
    }
}

/// Simple case mapping of one UTF-16 unit, as .NET's invariant `ToUpper`/`ToLower` do.
fn case(c: u16, upper: bool) -> u16 {
    let Some(ch) = char::from_u32(c as u32) else {
        return c;
    };
    let mut mapped = if upper {
        ch.to_uppercase().collect::<Vec<_>>()
    } else {
        ch.to_lowercase().collect::<Vec<_>>()
    };
    match (mapped.pop(), mapped.is_empty()) {
        (Some(single), true) if (single as u32) <= 0xFFFF => single as u16,
        _ => c,
    }
}

/// Nitrogen's token-expansion factoring: `[a] S / [b] S` becomes `[ab] S` when
/// `S` is structurally equal and every alternative between them starts with
/// characters disjoint from `[b]`. Nested sequences and choices flatten first.
fn factor(expr: &Value) -> Value {
    match expr["kind"].as_str() {
        Some("sequence") => {
            let items = flat(
                expr["items"].as_array().unwrap().iter().map(factor),
                "sequence",
            );
            if items.len() == 1 {
                items.into_iter().next().unwrap()
            } else {
                json!({"kind":"sequence","items":items})
            }
        }
        Some("choice") => group(flat(
            expr["items"].as_array().unwrap().iter().map(factor),
            "choice",
        )),
        Some("label") => {
            json!({"kind":"label","label":expr["label"],"inner":factor(&expr["inner"])})
        }
        Some("repeat") => {
            json!({"kind":"repeat","min":expr["min"],"max":expr["max"],"inner":factor(&expr["inner"])})
        }
        Some("predicate") => {
            json!({"kind":"predicate","positive":expr["positive"],"inner":factor(&expr["inner"])})
        }
        _ => expr.clone(),
    }
}

fn flat(items: impl Iterator<Item = Value>, kind: &str) -> Vec<Value> {
    items
        .flat_map(|i| {
            if i["kind"] == kind {
                i["items"].as_array().unwrap().clone()
            } else {
                vec![i]
            }
        })
        .collect()
}

/// A structural identity ignoring how a class's ranges are written.
fn identity(expr: &Value) -> String {
    match expr["kind"].as_str() {
        Some("class") => format!(
            "C{}{}",
            if expr["negated"] == true { "^" } else { "" },
            CodeSet::of_class(&json!({"ranges":expr["ranges"]})).key()
        ),
        Some("literal") => format!("L{}", expr["text"]),
        Some("any") => "A".into(),
        Some("ref") => format!("R{}", expr["name"]),
        Some("sequence") => format!(
            "S({})",
            expr["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(identity)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Some("choice") => format!(
            "O({})",
            expr["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(identity)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Some("label") => format!("B{}:{}", expr["label"], identity(&expr["inner"])),
        Some("repeat") => format!(
            "P{},{}:{}",
            expr["min"],
            expr["max"],
            identity(&expr["inner"])
        ),
        Some("predicate") => format!("D{}:{}", expr["positive"], identity(&expr["inner"])),
        _ => format!("X{expr}"),
    }
}

fn split(alternative: &Value) -> Option<(CodeSet, Option<Value>)> {
    match alternative["kind"].as_str() {
        Some("class") if alternative["negated"] != true => {
            Some((CodeSet::of_class(alternative), None))
        }
        Some("literal") => {
            let units: Vec<u16> = alternative["text"].as_str()?.encode_utf16().collect();
            (units.len() == 1).then(|| (CodeSet::unit(units[0]), None))
        }
        Some("sequence") => {
            let items = alternative["items"].as_array()?;
            let (head, tail) = split(items.first()?)?;
            if tail.is_some() {
                return None;
            }
            let rest = if items.len() == 2 {
                items[1].clone()
            } else {
                json!({"kind":"sequence","items":items[1..].to_vec()})
            };
            Some((head, Some(rest)))
        }
        _ => None,
    }
}

fn group(alternatives: Vec<Value>) -> Value {
    struct Entry {
        head: Option<CodeSet>,
        tail_id: Option<String>,
        tail: Option<Value>,
        original: Value,
    }
    let mut entries: Vec<Entry> = Vec::new();
    for alternative in alternatives {
        if let Some((head, tail)) = split(&alternative) {
            let rest = tail.as_ref().map(identity);
            let mut merged = None;
            for k in (0..entries.len()).rev() {
                if entries[k].head.is_some() && entries[k].tail_id == rest {
                    merged = Some(k);
                    break;
                }
                match &entries[k].head {
                    Some(other) if other.intersect(&head).is_empty() => {}
                    _ => break,
                }
            }
            if let Some(k) = merged {
                let union = entries[k].head.as_ref().unwrap().union(&head);
                entries[k].head = Some(union);
                continue;
            }
            entries.push(Entry {
                head: Some(head),
                tail_id: rest,
                tail,
                original: alternative,
            });
        } else {
            entries.push(Entry {
                head: None,
                tail_id: None,
                tail: None,
                original: alternative,
            });
        }
    }
    let mut result: Vec<Value> = entries
        .into_iter()
        .map(|e| match e.head {
            None => e.original,
            Some(head) => match e.tail {
                None => head.to_class(),
                Some(rest) if rest["kind"] == "sequence" => {
                    let mut items = vec![head.to_class()];
                    items.extend(rest["items"].as_array().unwrap().iter().cloned());
                    json!({"kind":"sequence","items":items})
                }
                Some(rest) => json!({"kind":"sequence","items":[head.to_class(), rest]}),
            },
        })
        .collect();
    if result.len() == 1 {
        result.pop().unwrap()
    } else {
        json!({"kind":"choice","items":result})
    }
}
