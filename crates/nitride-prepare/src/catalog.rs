//! Catalog files: parsed by the generated catalog frontend, resolved across
//! the project's modules and lowered to the catalog JSON codegen reads.
use crate::{PrepareError, Source};
use nitride_core::{Limits, SyntaxTree};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

const OPERATION_HANDLERS: &[&str] = &[
    "zero", "identity", "neg", "add", "sub", "mul", "div", "pow", "iso-date",
];
const VALUE_HANDLERS: &[&str] = &["pi", "e", "tau", "today"];
const PREDICATES: &[&str] = &["iso-date"];
const CORE: &[&str] = &["Core.Scalar", "Core.Text"];

/// A name as written, where it was written.
struct Name {
    text: String,
    span: [usize; 2],
}

enum Member {
    Type(Name),
    Operation {
        name: Name,
        inputs: Vec<Name>,
        result: Name,
        handler: Name,
    },
    Overload {
        name: Name,
        candidates: Vec<Name>,
    },
    Callable {
        kind: Name,
        name: Name,
        candidates: Vec<Name>,
    },
    Value {
        kind: Name,
        name: Name,
        ty: Name,
        handler: Name,
    },
    Statement {
        show: bool,
        rule: Name,
        field: Name,
        infer: bool,
    },
    Call {
        rule: Name,
        args: Vec<Name>,
    },
    Check {
        rule: Name,
        field: Name,
        predicate: Name,
        code: Name,
        message: String,
    },
}

struct Module {
    name: Name,
    span: [usize; 2],
    source: usize,
    usings: Vec<Name>,
    members: Vec<Member>,
    /// The span of each member, in `members` order.
    spans: Vec<[usize; 2]>,
    /// Problems decoding found; lowering reports the first. Editors still
    /// read the module's structure.
    errors: Vec<PrepareError>,
}

/// The catalog frontend's tree of one source.
fn syntax(source: &Source) -> Result<SyntaxTree<'static>, PrepareError> {
    nitride_core::parse_with_options(
        &crate::catalog_seed::LANGUAGE,
        &source.text,
        Limits::default(),
        nitride_core::ParseOptions {
            strict_comments: true,
            token_children: false,
            alias_nodes: true,
        },
    )
    .map_err(|e| source.error(e.span.map(|v| v as usize), e.message))
}

struct Decoder<'a> {
    source: &'a Source,
    tree: &'a SyntaxTree<'static>,
}
impl Decoder<'_> {
    fn kind(&self, id: u32) -> &str {
        crate::catalog_seed::LANGUAGE.rules[self.tree.nodes[id as usize].rule as usize]
            .name
            .rsplit('.')
            .next()
            .unwrap()
    }
    fn span(&self, id: u32) -> [usize; 2] {
        self.tree.nodes[id as usize].span.map(|v| v as usize)
    }
    fn field(&self, id: u32, label: &str) -> u32 {
        self.tree.field(id, label).expect("generated catalog field")
    }
    fn fields(&self, id: u32, label: &str) -> Vec<u32> {
        self.tree
            .children(id)
            .iter()
            .filter(|e| self.tree.label(e) == label)
            .map(|e| e.node)
            .collect()
    }
    fn name(&self, id: u32) -> Name {
        Name {
            text: self.tree.text(id).unwrap().into(),
            span: self.span(id),
        }
    }
    fn named(&self, id: u32, label: &str) -> Name {
        self.name(self.field(id, label))
    }
    fn names(&self, id: u32, label: &str) -> Vec<Name> {
        self.fields(id, label)
            .into_iter()
            .map(|n| self.name(n))
            .collect()
    }
    fn handler(&self, id: u32) -> Name {
        self.named(self.field(id, "Handler"), "Name")
    }
    fn module(&self, id: u32, source: usize) -> Module {
        let mut module = Module {
            name: self.named(id, "Name"),
            span: self.span(id),
            source,
            usings: vec![],
            members: vec![],
            spans: vec![],
            errors: vec![],
        };
        for member in self.fields(id, "Members") {
            let m = self.tree.children(member)[0].node;
            let decoded = match self.kind(m) {
                "Using" => {
                    if !module.members.is_empty() {
                        module.errors.push(
                            self.source
                                .error(self.span(m), "'using' must precede the module's members"),
                        );
                    }
                    module.usings.push(self.named(m, "Name"));
                    continue;
                }
                "Type" => Member::Type(self.named(m, "Name")),
                "Operation" => Member::Operation {
                    name: self.named(m, "Name"),
                    inputs: self.names(m, "Inputs"),
                    result: self.named(m, "Result"),
                    handler: self.handler(m),
                },
                "Overload" => Member::Overload {
                    name: self.named(m, "Name"),
                    candidates: self.names(m, "Candidates"),
                },
                "Builtin" => {
                    let mut body = self.field(m, "Body");
                    while !matches!(self.kind(body), "BuiltinValue" | "BuiltinCall") {
                        body = self.tree.children(body)[0].node;
                    }
                    let (kind, name) = (self.named(m, "Kind"), self.named(m, "Name"));
                    if self.kind(body) == "BuiltinValue" {
                        Member::Value {
                            kind,
                            name,
                            ty: self.named(body, "Type"),
                            handler: self.handler(body),
                        }
                    } else {
                        Member::Callable {
                            kind,
                            name,
                            candidates: self.names(body, "Candidates"),
                        }
                    }
                }
                kind @ ("Let" | "Show") => Member::Statement {
                    show: kind == "Show",
                    rule: self.named(m, "Rule"),
                    field: self.named(m, "Field"),
                    infer: self.tree.field(m, "Infer").is_some(),
                },
                "Call" => Member::Call {
                    rule: self.named(m, "Rule"),
                    args: self.names(m, "Args"),
                },
                "Check" => {
                    let target = self.named(m, "Target");
                    let Some(dot) = target.text.rfind('.') else {
                        module.errors.push(
                            self.source
                                .error(target.span, "a check names its field: 'Rule.Field'"),
                        );
                        continue;
                    };
                    let message = self.span(self.field(m, "Message"));
                    let message = crate::literal::decode(self.source, message, false)
                        .unwrap_or_else(|e| {
                            module.errors.push(e);
                            String::new()
                        });
                    Member::Check {
                        rule: Name {
                            text: target.text[..dot].into(),
                            span: [target.span[0], target.span[0] + dot],
                        },
                        field: Name {
                            text: target.text[dot + 1..].into(),
                            span: [target.span[0] + dot + 1, target.span[1]],
                        },
                        predicate: self.named(m, "Predicate"),
                        code: self.named(m, "Code"),
                        message,
                    }
                }
                other => unreachable!("catalog member {other}"),
            };
            module.members.push(decoded);
            module.spans.push(self.span(m));
        }
        module
    }
}

/// Parses every catalog source, one or more modules each.
fn parse(sources: &[Source]) -> Result<Vec<Module>, PrepareError> {
    let mut modules = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let tree = syntax(source)?;
        let decoder = Decoder {
            source,
            tree: &tree,
        };
        for id in decoder.fields(tree.root, "Modules") {
            modules.push(decoder.module(id, index));
        }
    }
    Ok(modules)
}

/// Members declared by every module, by qualified name.
struct Scope<'a> {
    sources: &'a [Source],
    modules: &'a [Module],
    /// Qualified type names.
    types: HashMap<String, (usize, [usize; 2])>,
    /// Qualified operation and overload names, and whether each is an operation.
    callables: HashMap<String, (usize, [usize; 2], bool)>,
}

impl<'a> Scope<'a> {
    fn error(&self, module: &Module, span: [usize; 2], message: impl Into<String>) -> PrepareError {
        self.sources[module.source].error(span, message)
    }
    /// `name` as written in `module`: qualified, its own, exactly one of its
    /// `using`s, else (for types) a core type.
    fn resolve(&self, module: &Module, name: &Name, what: &str) -> Result<String, PrepareError> {
        let declared = |q: &str| match what {
            "type" => self.types.contains_key(q),
            _ => self.callables.get(q).is_some_and(|c| c.2),
        };
        let core = |q: &str| what == "type" && CORE.contains(&q);
        let unknown = || self.error(module, name.span, format!("unknown {what} '{}'", name.text));
        if name.text.contains('.') {
            return if declared(&name.text) || core(&name.text) {
                Ok(name.text.clone())
            } else {
                Err(unknown())
            };
        }
        let own = format!("{}.{}", module.name.text, name.text);
        if declared(&own) {
            return Ok(own);
        }
        let mut found: Vec<(&str, String)> = Vec::new();
        for using in &module.usings {
            let q = format!("{}.{}", using.text, name.text);
            if declared(&q) && !found.iter().any(|(m, _)| *m == using.text) {
                found.push((&using.text, q));
            }
        }
        match found.as_slice() {
            [(_, one)] => Ok(one.clone()),
            [(first, _), (second, _), ..] => Err(self.error(
                module,
                name.span,
                format!(
                    "'{}' is defined in both '{first}' and '{second}'",
                    name.text
                ),
            )),
            [] if core(&format!("Core.{}", name.text)) => Ok(format!("Core.{}", name.text)),
            [] => Err(unknown()),
        }
    }
    /// A grammar rule: bare names are the grammar module named like the catalog.
    fn rule(&self, module: &Module, rule: &Name) -> String {
        if rule.text.contains('.') {
            rule.text.clone()
        } else {
            format!("{}.{}", module.name.text, rule.text)
        }
    }
    fn location(&self, module: usize, span: [usize; 2]) -> String {
        let source = &self.sources[self.modules[module].source];
        let line = source.text[..span[0]]
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
            + 1;
        format!("{}:{line}", source.path)
    }
}

fn vocabulary(
    scope: &Scope,
    module: &Module,
    name: &Name,
    allowed: &[&str],
    what: &str,
) -> Result<(), PrepareError> {
    if allowed.contains(&name.text.as_str()) {
        Ok(())
    } else {
        Err(scope.error(
            module,
            name.span,
            format!(
                "unknown {what} '{}'; expected one of {}",
                name.text,
                allowed.join(", ")
            ),
        ))
    }
}

/// A grammar rule, or one of its fields, named by a catalog member.
pub struct Use {
    pub rule: String,
    pub field: Option<String>,
    pub source: usize,
    pub span: [usize; 2],
}

/// The catalog JSON of a project's catalog sources, merged in source order,
/// and the grammar rules and fields it names. No sources give the empty
/// catalog of a syntax-only profile.
pub fn lower(sources: &[Source]) -> Result<(Value, Vec<Use>), PrepareError> {
    let modules = parse(sources)?;
    if let Some(error) = modules.iter().flat_map(|m| &m.errors).next() {
        return Err(error.clone());
    }
    let mut scope = Scope {
        sources,
        modules: &modules,
        types: HashMap::new(),
        callables: HashMap::new(),
    };
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (index, module) in modules.iter().enumerate() {
        if let Some(&other) = seen.get(module.name.text.as_str()) {
            return Err(scope.error(
                module,
                module.name.span,
                format!(
                    "catalog module '{}' is already declared at {}",
                    module.name.text,
                    scope.location(other, modules[other].name.span)
                ),
            ));
        }
        seen.insert(&module.name.text, index);
        for member in &module.members {
            let (name, operation, table) = match member {
                Member::Type(name) => (name, false, true),
                Member::Operation { name, .. } => (name, true, false),
                Member::Overload { name, .. } => (name, false, false),
                _ => continue,
            };
            let qualified = format!("{}.{}", module.name.text, name.text);
            let previous = if table {
                scope
                    .types
                    .insert(qualified, (index, name.span))
                    .map(|p| p.1)
            } else {
                scope
                    .callables
                    .insert(qualified, (index, name.span, operation))
                    .map(|p| p.1)
            };
            if let Some(span) = previous {
                return Err(scope.error(
                    module,
                    name.span,
                    format!(
                        "'{}' is already declared at {}",
                        name.text,
                        scope.location(index, span)
                    ),
                ));
            }
        }
    }
    for module in &modules {
        for using in &module.usings {
            if !seen.contains_key(using.text.as_str()) {
                return Err(scope.error(
                    module,
                    using.span,
                    format!("catalog module '{}' is not in this project", using.text),
                ));
            }
        }
    }
    let scope = scope;

    let mut types = Vec::new();
    let mut text = false;
    let mut operations = Vec::new();
    let mut overloads = Map::new();
    let mut builtins = Map::new();
    let mut values = Map::new();
    let mut statements = Map::new();
    let mut calls = Map::new();
    let mut checks = Map::new();
    // The first declaration of each keyed entry, for conflict messages.
    let mut keyed: HashMap<String, (usize, [usize; 2])> = HashMap::new();
    let mut uses = Vec::new();
    let mut claim = |key: String, index: usize, at: &Name, what: &str| {
        if let Some(&(other, span)) = keyed.get(&key) {
            return Err(scope.error(
                &modules[index],
                at.span,
                format!(
                    "{what} is already declared at {}",
                    scope.location(other, span)
                ),
            ));
        }
        keyed.insert(key, (index, at.span));
        Ok(())
    };
    for (index, module) in modules.iter().enumerate() {
        let mut ty = |name: &Name| {
            let resolved = scope.resolve(module, name, "type")?;
            text |= resolved == "Core.Text";
            Ok::<_, PrepareError>(resolved)
        };
        let qualify = |name: &Name| format!("{}.{}", module.name.text, name.text);
        let mut used = |rule: &str, field: Option<&Name>, at: &Name| {
            uses.push(Use {
                rule: rule.into(),
                field: field.map(|f| f.text.clone()),
                source: module.source,
                span: at.span,
            })
        };
        for member in &module.members {
            match member {
                Member::Type(name) => types.push(qualify(name)),
                Member::Operation {
                    name,
                    inputs,
                    result,
                    handler,
                } => {
                    vocabulary(
                        &scope,
                        module,
                        handler,
                        OPERATION_HANDLERS,
                        "operation handler",
                    )?;
                    let inputs = inputs.iter().map(&mut ty).collect::<Result<Vec<_>, _>>()?;
                    operations.push(json!({"id":qualify(name),"inputs":inputs,"result":ty(result)?,"handler":handler.text}));
                }
                Member::Overload { name, candidates } => {
                    let candidates = candidates
                        .iter()
                        .map(|c| scope.resolve(module, c, "operation"))
                        .collect::<Result<Vec<_>, _>>()?;
                    overloads.insert(qualify(name), json!(candidates));
                }
                Member::Callable {
                    kind,
                    name,
                    candidates,
                } => {
                    claim(
                        format!("builtin {} {}", kind.text, name.text),
                        index,
                        name,
                        &format!("builtin {} '{}'", kind.text, name.text),
                    )?;
                    let candidates = candidates
                        .iter()
                        .map(|c| scope.resolve(module, c, "operation"))
                        .collect::<Result<Vec<_>, _>>()?;
                    builtins
                        .entry(kind.text.clone())
                        .or_insert_with(|| json!({}))[&name.text] = json!(candidates);
                }
                Member::Value {
                    kind,
                    name,
                    ty: value_type,
                    handler,
                } => {
                    claim(
                        format!("builtin {} {}", kind.text, name.text),
                        index,
                        name,
                        &format!("builtin {} '{}'", kind.text, name.text),
                    )?;
                    vocabulary(&scope, module, handler, VALUE_HANDLERS, "value handler")?;
                    values.entry(kind.text.clone()).or_insert_with(|| json!({}))[&name.text] =
                        json!({"type":ty(value_type)?,"handler":handler.text});
                }
                Member::Statement {
                    show,
                    rule,
                    field,
                    infer,
                } => {
                    let rule_name = scope.rule(module, rule);
                    claim(
                        format!("statement {rule_name}"),
                        index,
                        rule,
                        &format!("a statement policy for '{rule_name}'"),
                    )?;
                    used(&rule_name, None, rule);
                    used(&rule_name, Some(field), field);
                    let mut policy =
                        json!({"kind":if *show {"show"} else {"let"},"value":field.text});
                    if *infer {
                        policy["infer"] = json!(true);
                    }
                    statements.insert(rule_name, policy);
                }
                Member::Call { rule, args } => {
                    let rule_name = scope.rule(module, rule);
                    claim(
                        format!("call {rule_name}"),
                        index,
                        rule,
                        &format!("a call for '{rule_name}'"),
                    )?;
                    used(&rule_name, None, rule);
                    for arg in args {
                        used(&rule_name, Some(arg), arg);
                    }
                    calls.insert(
                        rule_name,
                        json!({"args":args.iter().map(|a| &a.text).collect::<Vec<_>>()}),
                    );
                }
                Member::Check {
                    rule,
                    field,
                    predicate,
                    code,
                    message,
                } => {
                    let rule_name = scope.rule(module, rule);
                    claim(
                        format!("check {rule_name}.{}", field.text),
                        index,
                        field,
                        &format!("a check on '{rule_name}.{}'", field.text),
                    )?;
                    vocabulary(&scope, module, predicate, PREDICATES, "check predicate")?;
                    used(&rule_name, None, rule);
                    used(&rule_name, Some(field), field);
                    checks
                        .entry(rule_name)
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"code":code.text,"field":field.text,"predicate":predicate.text,"message":message}));
                }
            }
        }
    }
    if !modules.is_empty() {
        let core = if text { CORE } else { &CORE[..1] };
        types.splice(0..0, core.iter().map(|t| t.to_string()));
    }
    let mut catalog = json!({"types":types,"operations":operations,"statements":statements});
    for (key, section) in [
        ("overloads", overloads),
        ("builtins", builtins),
        ("values", values),
        ("calls", calls),
        ("checks", checks),
    ] {
        if !section.is_empty() {
            catalog[key] = Value::Object(section);
        }
    }
    Ok((catalog, uses))
}

/// The modules of one catalog source for editors, from its undamaged lines.
fn editor_modules(source: &Source) -> Result<Vec<Module>, PrepareError> {
    let source = crate::parser::tolerant_with(source, syntax)?;
    parse(std::slice::from_ref(&source))
}

/// Whether a catalog file parses as written.
pub fn parses(source: &Source) -> bool {
    syntax(source).is_ok()
}

/// Catalog modules and the types, operations, overloads and built-ins they declare.
pub fn outline(source: &Source) -> Result<Vec<crate::GrammarItem>, PrepareError> {
    let item = |kind: &'static str, name: &Name, span: [usize; 2]| crate::GrammarItem {
        kind,
        name: name.text.clone(),
        span,
        name_span: name.span,
        children: vec![],
    };
    Ok(editor_modules(source)?
        .iter()
        .map(|module| crate::GrammarItem {
            children: module
                .members
                .iter()
                .zip(&module.spans)
                .filter_map(|(member, &span)| match member {
                    Member::Type(name) => Some(item("type", name, span)),
                    Member::Operation { name, .. } => Some(item("operation", name, span)),
                    Member::Overload { name, .. } => Some(item("overload", name, span)),
                    Member::Callable { name, .. } | Member::Value { name, .. } => {
                        Some(item("builtin", name, span))
                    }
                    _ => None,
                })
                .collect(),
            ..item("catalog", &module.name, module.span)
        })
        .collect())
}

/// Names a catalog file uses: catalog modules (`using`, qualifiers), catalog
/// members ("member": types, operations and overloads), grammar rules and
/// modules, and rule fields (named `Rule.Field` as written).
pub fn references(source: &Source) -> Result<Vec<crate::GrammarReference>, PrepareError> {
    type Used = Vec<(&'static str, String, [usize; 2])>;
    // A qualified name's qualifier is a module of the name's own kind.
    fn qualified(used: &mut Used, kind: &'static str, qualifier: &'static str, name: &Name) {
        if let Some(dot) = name.text.rfind('.') {
            let span = [name.span[0], name.span[0] + dot];
            used.push((qualifier, name.text[..dot].into(), span));
        }
        used.push((kind, name.text.clone(), name.span));
    }
    fn rule(used: &mut Used, rule: &Name, fields: &[&Name]) {
        qualified(used, "rule", "module", rule);
        for field in fields {
            used.push(("field", format!("{}.{}", rule.text, field.text), field.span));
        }
    }
    let mut found = Vec::new();
    for module in editor_modules(source)? {
        let mut used = Used::new();
        for using in &module.usings {
            qualified(&mut used, "catalog", "catalog", using);
        }
        let mut members: Vec<&Name> = Vec::new();
        for member in &module.members {
            match member {
                Member::Type(_) => {}
                Member::Operation { inputs, result, .. } => {
                    members.extend(inputs);
                    members.push(result);
                }
                Member::Overload { candidates, .. } | Member::Callable { candidates, .. } => {
                    members.extend(candidates)
                }
                Member::Value { ty, .. } => members.push(ty),
                Member::Statement { rule: r, field, .. } | Member::Check { rule: r, field, .. } => {
                    rule(&mut used, r, &[field])
                }
                Member::Call { rule: r, args } => {
                    rule(&mut used, r, &args.iter().collect::<Vec<_>>())
                }
            }
        }
        for name in members {
            qualified(&mut used, "member", "catalog", name);
        }
        found.extend(
            used.into_iter()
                .map(|(kind, name, span)| crate::GrammarReference {
                    kind,
                    name,
                    span,
                    module: module.name.text.clone(),
                }),
        );
    }
    found.sort_by_key(|r| r.span[0]);
    Ok(found)
}
