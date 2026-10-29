//! Catalog files: parsed by the generated catalog frontend, resolved across
//! the project's modules and lowered to the catalog JSON codegen reads.
use crate::{PrepareError, Source};
use nitride_core::{Limits, SyntaxTree, host};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const CORE: &[&str] = &["Core.Scalar", "Core.Text"];
const REPRESENTATIONS: &[&str] = &["f64", "i64", "bool", "text"];
/// The type of a `true` or `false` literal in an example: any type
/// represented as bool.
const BOOLEAN_LITERAL: &str = "a boolean literal";

/// A name as written, where it was written.
struct Name {
    text: String,
    span: [usize; 2],
}

/// Who implements an operation.
enum Provider {
    /// A built-in host handler.
    Host(Name),
    /// Any binding of the module's interface; it may report these failures.
    Interface { interface: Name, fails: Vec<Name> },
    /// The engine reads this field of its record input.
    Field(Name),
    /// The engine's `record`, `list`, `length` or `item`.
    Structure(Name),
}

/// How a type's values are represented, when not f64.
enum Shape {
    Represented(Name),
    /// Field names and types, in order.
    Record(Vec<(Name, Name)>),
    List(Name),
}

enum ExampleExpr {
    Number(f64),
    Boolean(bool),
    Text(String),
    Call {
        operation: Name,
        args: Vec<ExampleExpr>,
    },
}

enum Outcome {
    Equals(ExampleExpr),
    Fails(Name),
}

struct Example {
    value: ExampleExpr,
    outcome: Outcome,
    span: [usize; 2],
}

enum Member {
    /// A type, and how its values are represented when not f64.
    Type(Name, Option<Shape>),
    Failure {
        name: Name,
        message: String,
    },
    /// Its operations follow it as `Operation` members.
    Interface {
        name: Name,
        version: u32,
        examples: Vec<Example>,
    },
    Operation {
        name: Name,
        inputs: Vec<Name>,
        result: Name,
        provider: Provider,
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
    /// The first node at or under `id` of one of `kinds`, through choices.
    fn choice(&self, mut id: u32, kinds: &[&str]) -> u32 {
        while !kinds.contains(&self.kind(id)) {
            id = self.tree.children(id)[0].node;
        }
        id
    }
    fn string(&self, id: u32, errors: &mut Vec<PrepareError>) -> String {
        crate::literal::decode(self.source, self.span(id), false).unwrap_or_else(|e| {
            errors.push(e);
            String::new()
        })
    }
    fn example_expr(&self, id: u32, errors: &mut Vec<PrepareError>) -> ExampleExpr {
        let id = self.choice(
            id,
            &[
                "ExampleCall",
                "ExampleNumber",
                "ExampleBoolean",
                "ExampleText",
            ],
        );
        match self.kind(id) {
            "ExampleCall" => ExampleExpr::Call {
                operation: self.named(id, "Operation"),
                args: self
                    .fields(id, "Args")
                    .into_iter()
                    .map(|a| self.example_expr(a, errors))
                    .collect(),
            },
            "ExampleNumber" => {
                let value = self.named(id, "Value");
                ExampleExpr::Number(value.text.parse().unwrap_or_else(|_| {
                    errors.push(self.source.error(value.span, "not a finite number"));
                    0.0
                }))
            }
            "ExampleBoolean" => ExampleExpr::Boolean(self.named(id, "Value").text == "true"),
            _ => ExampleExpr::Text(self.string(self.field(id, "Value"), errors)),
        }
    }
    /// An interface, then its operations, in source order.
    fn interface(&self, id: u32, errors: &mut Vec<PrepareError>) -> Vec<(Member, [usize; 2])> {
        let name = self.named(id, "Name");
        let version = self.named(id, "Version");
        let version = version
            .text
            .parse::<u32>()
            .ok()
            .filter(|v| *v > 0)
            .unwrap_or_else(|| {
                errors.push(
                    self.source
                        .error(version.span, "an interface version is a positive integer"),
                );
                1
            });
        let mut operations = Vec::new();
        let mut examples = Vec::new();
        for member in self.fields(id, "Members") {
            let m = self.choice(member, &["Contract", "Example"]);
            if self.kind(m) == "Contract" {
                let fails = self
                    .tree
                    .field(m, "Fails")
                    .map_or_else(Vec::new, |f| self.names(f, "Failures"));
                operations.push((
                    Member::Operation {
                        name: self.named(m, "Name"),
                        inputs: self.names(m, "Inputs"),
                        result: self.named(m, "Result"),
                        provider: Provider::Interface {
                            interface: Name {
                                text: name.text.clone(),
                                span: name.span,
                            },
                            fails,
                        },
                    },
                    self.span(m),
                ));
            } else {
                let outcome = self.choice(self.field(m, "Outcome"), &["Equals", "Raises"]);
                examples.push(Example {
                    value: self.example_expr(self.field(m, "Value"), errors),
                    outcome: if self.kind(outcome) == "Equals" {
                        Outcome::Equals(self.example_expr(self.field(outcome, "Expected"), errors))
                    } else {
                        Outcome::Fails(self.named(outcome, "Failure"))
                    },
                    span: self.span(m),
                });
            }
        }
        let interface = Member::Interface {
            name,
            version,
            examples,
        };
        std::iter::once((interface, self.span(id)))
            .chain(operations)
            .collect()
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
                "Type" => Member::Type(
                    self.named(m, "Name"),
                    self.tree.field(m, "Representation").map(|r| {
                        let shape =
                            self.choice(self.field(r, "Shape"), &["Represented", "Record", "List"]);
                        match self.kind(shape) {
                            "Represented" => Shape::Represented(self.named(shape, "Kind")),
                            "Record" => Shape::Record(
                                self.fields(shape, "Fields")
                                    .into_iter()
                                    .map(|f| (self.named(f, "Name"), self.named(f, "Type")))
                                    .collect(),
                            ),
                            _ => Shape::List(self.named(shape, "Element")),
                        }
                    }),
                ),
                "Failure" => Member::Failure {
                    name: self.named(m, "Name"),
                    message: self.string(self.field(m, "Message"), &mut module.errors),
                },
                "Interface" => {
                    for (member, span) in self.interface(m, &mut module.errors) {
                        module.members.push(member);
                        module.spans.push(span);
                    }
                    continue;
                }
                "Operation" => Member::Operation {
                    name: self.named(m, "Name"),
                    inputs: self.names(m, "Inputs"),
                    result: self.named(m, "Result"),
                    provider: {
                        let p = self.choice(
                            self.field(m, "Provider"),
                            &["Handler", "FieldOf", "Structure"],
                        );
                        match self.kind(p) {
                            "Handler" => Provider::Host(self.named(p, "Name")),
                            "FieldOf" => Provider::Field(self.named(p, "Name")),
                            _ => Provider::Structure(self.named(p, "Kind")),
                        }
                    },
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
    /// Qualified failure and interface names, and whether each is a failure.
    contracts: HashMap<String, (usize, [usize; 2], bool)>,
    /// Representations other than f64, by qualified type name: a kind such
    /// as `"i64"`, `{"record":[[field, type], …]}` or `{"list":type}`.
    representations: HashMap<String, Value>,
}

impl<'a> Scope<'a> {
    /// The kind of a resolved type's values: `f64`, `i64`, `bool`, `text`,
    /// `record` or `list`.
    fn representation(&self, ty: &str) -> &str {
        match self.representations.get(ty) {
            None => "f64",
            Some(Value::String(kind)) => kind,
            Some(Value::Object(shape)) => shape.keys().next().map_or("f64", String::as_str),
            Some(_) => "f64",
        }
    }
    /// A record type's fields, `(name, type)` in order.
    fn fields(&self, ty: &str) -> Option<Vec<(&str, &str)>> {
        let fields = self.representations.get(ty)?.get("record")?.as_array()?;
        Some(
            fields
                .iter()
                .map(|f| (f[0].as_str().unwrap(), f[1].as_str().unwrap()))
                .collect(),
        )
    }
    /// A list type's element type.
    fn element(&self, ty: &str) -> Option<&str> {
        self.representations.get(ty)?.get("list")?.as_str()
    }
    /// The representations of `ty` and of every type inside it, for a
    /// contract hash.
    fn shapes(&self, ty: &str, into: &mut Map<String, Value>) {
        if into.contains_key(ty) {
            return;
        }
        into.insert(
            ty.into(),
            self.representations
                .get(ty)
                .cloned()
                .unwrap_or_else(|| json!("f64")),
        );
        let inner: Vec<String> = match (self.fields(ty), self.element(ty)) {
            (Some(fields), _) => fields.iter().map(|f| f.1.to_string()).collect(),
            (_, Some(element)) => vec![element.into()],
            _ => vec![],
        };
        for ty in inner {
            self.shapes(&ty, into);
        }
    }
    fn error(&self, module: &Module, span: [usize; 2], message: impl Into<String>) -> PrepareError {
        self.sources[module.source].error(span, message)
    }
    /// `name` as written in `module`: qualified, its own, exactly one of its
    /// `using`s, else (for types) a core type.
    fn resolve(&self, module: &Module, name: &Name, what: &str) -> Result<String, PrepareError> {
        let declared = |q: &str| match what {
            "type" => self.types.contains_key(q),
            "failure" => self.contracts.get(q).is_some_and(|c| c.2),
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
    allowed: impl Iterator<Item = &'static str> + Clone,
    what: &str,
) -> Result<(), PrepareError> {
    if allowed.clone().any(|a| a == name.text) {
        Ok(())
    } else {
        Err(scope.error(
            module,
            name.span,
            format!(
                "unknown {what} '{}'; expected one of {}",
                name.text,
                allowed.collect::<Vec<_>>().join(", ")
            ),
        ))
    }
}

/// An example expression as JSON and its type. Collects the failures its
/// operations may report.
fn lower_expr(
    scope: &Scope,
    module: &Module,
    operations: &[Value],
    expr: &ExampleExpr,
    fails: &mut Vec<String>,
) -> Result<(Value, String), PrepareError> {
    match expr {
        ExampleExpr::Number(n) => Ok((json!({"number":n}), CORE[0].into())),
        ExampleExpr::Boolean(b) => Ok((json!({"boolean":b}), BOOLEAN_LITERAL.into())),
        ExampleExpr::Text(text) => Ok((json!({"text":text}), CORE[1].into())),
        ExampleExpr::Call { operation, args } => {
            let id = scope.resolve(module, operation, "operation")?;
            let op = operations
                .iter()
                .find(|op| op["id"] == id.as_str())
                .expect("resolved operations are lowered");
            let inputs = op["inputs"].as_array().unwrap();
            if inputs.len() != args.len() {
                return Err(scope.error(
                    module,
                    operation.span,
                    format!(
                        "'{}' takes {} inputs, not {}",
                        operation.text,
                        inputs.len(),
                        args.len()
                    ),
                ));
            }
            let mut lowered = Vec::new();
            for (position, (arg, input)) in args.iter().zip(inputs).enumerate() {
                let (value, ty) = lower_expr(scope, module, operations, arg, fails)?;
                if !fits(scope, input.as_str().unwrap(), &ty) {
                    return Err(scope.error(
                        module,
                        operation.span,
                        format!(
                            "input {} of '{}' is '{}', not '{ty}'",
                            position + 1,
                            operation.text,
                            input.as_str().unwrap()
                        ),
                    ));
                }
                lowered.push(value);
            }
            for fail in op["fails"].as_array().into_iter().flatten() {
                fails.push(fail.as_str().unwrap().into());
            }
            Ok((
                json!({"call":id,"args":lowered}),
                op["result"].as_str().unwrap().into(),
            ))
        }
    }
}

/// Checks an engine operation's signature against the types it works on:
/// `field` (when `field` is given), `record`, `list`, `length` or `item`.
fn structural(
    scope: &Scope,
    module: &Module,
    at: &Name,
    operation: &Value,
    field: Option<&Name>,
) -> Result<(), PrepareError> {
    let inputs: Vec<&str> = operation["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    let result = operation["result"].as_str().unwrap();
    let scalar = CORE[0];
    let (valid, message) = match field {
        Some(field) => {
            let fields = inputs.first().and_then(|ty| scope.fields(ty));
            match fields
                .as_ref()
                .and_then(|f| f.iter().find(|f| f.0 == field.text))
            {
                Some((_, ty)) => (
                    inputs.len() == 1 && *ty == result,
                    "a field operation takes one record and returns the field's type".into(),
                ),
                None if fields.is_some() => (
                    false,
                    format!("'{}' has no field '{}'", inputs[0], field.text),
                ),
                None => (
                    false,
                    "a field operation takes one record and returns the field's type".into(),
                ),
            }
        }
        None => match at.text.as_str() {
            "record" => (
                scope
                    .fields(result)
                    .is_some_and(|f| f.iter().map(|f| f.1).eq(inputs.iter().copied())),
                "a record operation takes its result's fields in order".into(),
            ),
            "list" => (
                scope
                    .element(result)
                    .is_some_and(|e| inputs.iter().all(|t| *t == e)),
                "a list operation takes elements of its result's type".into(),
            ),
            "length" => (
                inputs.len() == 1 && scope.element(inputs[0]).is_some() && result == scalar,
                "a length operation takes a list and returns Scalar".into(),
            ),
            _ => (
                inputs.len() == 2
                    && inputs[1] == scalar
                    && scope.element(inputs[0]) == Some(result),
                "an item operation takes a list and a Scalar and returns an element".into(),
            ),
        },
    };
    if valid {
        Ok(())
    } else {
        Err(scope.error(module, field.map_or(at.span, |f| f.span), message))
    }
}

/// Whether an example value of type `actual` may stand where `expected` is
/// required. Types are nominal; a boolean literal is any bool type, and a
/// string literal is source text or any text type.
fn fits(scope: &Scope, expected: &str, actual: &str) -> bool {
    expected == actual
        || (actual == BOOLEAN_LITERAL && scope.representation(expected) == "bool")
        || (actual == CORE[1] && scope.representation(expected) == "text")
}

/// An interface example as JSON, checked against the catalog's operations.
fn lower_example(
    scope: &Scope,
    module: &Module,
    operations: &[Value],
    example: &Example,
) -> Result<Value, PrepareError> {
    let error = |message: String| scope.error(module, example.span, message);
    let mut fails = Vec::new();
    let (value, ty) = lower_expr(scope, module, operations, &example.value, &mut fails)?;
    if ty == CORE[1] || ty == BOOLEAN_LITERAL {
        return Err(error(
            "an example's value cannot be a text or boolean literal".into(),
        ));
    }
    let source = &scope.sources[module.source].text[example.span[0]..example.span[1]];
    Ok(match &example.outcome {
        Outcome::Equals(expected) => {
            let (expected, other) =
                lower_expr(scope, module, operations, expected, &mut Vec::new())?;
            if !fits(scope, &ty, &other) {
                return Err(error(format!(
                    "the two sides of an example are '{ty}' and '{other}'"
                )));
            }
            json!({"source":source,"value":value,"equals":expected})
        }
        Outcome::Fails(failure) => {
            let failure_id = scope.resolve(module, failure, "failure")?;
            if !fails.contains(&failure_id) {
                return Err(scope.error(
                    module,
                    failure.span,
                    format!("no operation in this example reports '{}'", failure.text),
                ));
            }
            json!({"source":source,"value":value,"fails":failure_id})
        }
    })
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
        contracts: HashMap::new(),
        representations: HashMap::new(),
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
            if let Member::Type(name, Some(Shape::Represented(kind))) = member {
                vocabulary(
                    &scope,
                    module,
                    kind,
                    REPRESENTATIONS.iter().copied(),
                    "representation",
                )?;
                if kind.text != "f64" {
                    scope.representations.insert(
                        format!("{}.{}", module.name.text, name.text),
                        json!(kind.text),
                    );
                }
            }
            // The table a member's name goes in, and the flag it carries there.
            let (name, table, flag) = match member {
                Member::Type(name, _) => (name, 0, false),
                Member::Operation { name, .. } => (name, 1, true),
                Member::Overload { name, .. } => (name, 1, false),
                Member::Failure { name, .. } => (name, 2, true),
                Member::Interface { name, .. } => (name, 2, false),
                _ => continue,
            };
            let qualified = format!("{}.{}", module.name.text, name.text);
            let entry = (index, name.span, flag);
            let previous = match table {
                0 => scope
                    .types
                    .insert(qualified, (index, name.span))
                    .map(|p| p.1),
                1 => scope.callables.insert(qualified, entry).map(|p| p.1),
                _ => scope.contracts.insert(qualified, entry).map(|p| p.1),
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
    // Records and lists name other types, so they resolve once all are known.
    let mut composite = Vec::new();
    for module in &modules {
        for member in &module.members {
            let (name, shape) = match member {
                Member::Type(name, Some(shape @ (Shape::Record(_) | Shape::List(_)))) => {
                    (name, shape)
                }
                _ => continue,
            };
            let component = |ty: &Name| {
                let resolved = scope.resolve(module, ty, "type")?;
                if resolved == CORE[1] {
                    return Err(scope.error(module, ty.span, "a record or list cannot hold Text"));
                }
                Ok(resolved)
            };
            let value = match shape {
                Shape::Record(fields) => {
                    if fields.is_empty() {
                        return Err(scope.error(module, name.span, "a record has fields"));
                    }
                    let mut lowered = Vec::new();
                    for (i, (field, ty)) in fields.iter().enumerate() {
                        if fields[..i].iter().any(|f| f.0.text == field.text) {
                            return Err(scope.error(
                                module,
                                field.span,
                                format!("field '{}' is already declared", field.text),
                            ));
                        }
                        lowered.push(json!([field.text, component(ty)?]));
                    }
                    json!({"record": lowered})
                }
                Shape::List(element) => json!({"list": component(element)?}),
                Shape::Represented(_) => unreachable!(),
            };
            composite.push((format!("{}.{}", module.name.text, name.text), value));
        }
    }
    scope.representations.extend(composite);
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
    let mut failures = Map::new();
    // (module, qualified name, name span, version, examples), lowered once
    // every operation is known.
    let mut interfaces = Vec::new();
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
                Member::Type(name, _) => types.push(qualify(name)),
                Member::Failure { name, message } => {
                    failures.insert(qualify(name), json!(message));
                }
                Member::Interface {
                    name,
                    version,
                    examples,
                } => interfaces.push((index, qualify(name), name.span, *version, examples)),
                Member::Operation {
                    name,
                    inputs,
                    result,
                    provider,
                } => {
                    let inputs = inputs.iter().map(&mut ty).collect::<Result<Vec<_>, _>>()?;
                    let mut operation =
                        json!({"id":qualify(name),"inputs":inputs,"result":ty(result)?});
                    match provider {
                        Provider::Host(handler) => {
                            vocabulary(
                                &scope,
                                module,
                                handler,
                                host::OPERATIONS.iter().map(|h| h.name),
                                "operation handler",
                            )?;
                            operation["handler"] = json!(handler.text);
                        }
                        Provider::Interface { interface, fails } => {
                            let fails = fails
                                .iter()
                                .map(|f| scope.resolve(module, f, "failure"))
                                .collect::<Result<Vec<_>, _>>()?;
                            operation["interface"] = json!(qualify(interface));
                            operation["fails"] = json!(fails);
                        }
                        Provider::Field(field) => {
                            structural(&scope, module, name, &operation, Some(field))?;
                            operation["handler"] = json!("field");
                            operation["field"] = json!(field.text);
                        }
                        Provider::Structure(kind) => {
                            structural(&scope, module, kind, &operation, None)?;
                            operation["handler"] = json!(kind.text);
                        }
                    }
                    operations.push(operation);
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
                    vocabulary(
                        &scope,
                        module,
                        handler,
                        host::VALUES.iter().map(|h| h.name),
                        "value handler",
                    )?;
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
                    vocabulary(
                        &scope,
                        module,
                        predicate,
                        host::PREDICATES.iter().map(|h| h.name),
                        "check predicate",
                    )?;
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
    let mut lowered = Map::new();
    for (index, id, span, version, examples) in interfaces {
        let module = &modules[index];
        let members: Vec<&Value> = operations
            .iter()
            .filter(|op| op["interface"] == id.as_str())
            .collect();
        if members.is_empty() {
            return Err(scope.error(
                module,
                span,
                format!("interface '{id}' declares no operations"),
            ));
        }
        let mut used = Map::new();
        for op in &members {
            for ty in op["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .chain([&op["result"]])
            {
                scope.shapes(ty.as_str().unwrap(), &mut used);
            }
        }
        let contract = json!({"interface":id,"version":version,"representations":used,"operations":members.iter().map(|op| json!({
            "id":op["id"],"inputs":op["inputs"],"result":op["result"],"fails":op["fails"]})).collect::<Vec<_>>()});
        let hash = format!("{:x}", Sha256::digest(contract.to_string().as_bytes()));
        let examples = examples
            .iter()
            .map(|example| lower_example(&scope, module, &operations, example))
            .collect::<Result<Vec<_>, _>>()?;
        lowered.insert(
            id,
            json!({"version":version,"hash":hash,
                "operations":members.iter().map(|op| &op["id"]).collect::<Vec<_>>(),
                "examples":examples}),
        );
    }
    let mut catalog = json!({"types":types,"operations":operations,"statements":statements});
    for (key, section) in [
        ("overloads", overloads),
        ("builtins", builtins),
        ("values", values),
        ("calls", calls),
        ("checks", checks),
        ("failures", failures),
        ("interfaces", lowered),
        (
            "representations",
            scope
                .representations
                .iter()
                .map(|(ty, shape)| (ty.clone(), shape.clone()))
                .collect(),
        ),
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
                    Member::Type(name, _) => Some(item("type", name, span)),
                    Member::Failure { name, .. } => Some(item("failure", name, span)),
                    Member::Interface { name, .. } => Some(item("interface", name, span)),
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

/// The host handlers a catalog file names.
pub fn handlers(source: &Source) -> Result<Vec<crate::HostHandler>, PrepareError> {
    let mut found = Vec::new();
    for module in editor_modules(source)? {
        for member in &module.members {
            let (kind, handler) = match member {
                Member::Operation {
                    provider: Provider::Host(handler),
                    ..
                } => ("operation", handler),
                Member::Value { handler, .. } => ("value", handler),
                _ => continue,
            };
            found.push(crate::HostHandler {
                kind,
                name: handler.text.clone(),
                span: handler.span,
            });
        }
    }
    Ok(found)
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
                Member::Type(_, Some(Shape::Record(fields))) => {
                    members.extend(fields.iter().map(|f| &f.1))
                }
                Member::Type(_, Some(Shape::List(element))) => members.push(element),
                Member::Type(..) | Member::Failure { .. } | Member::Interface { .. } => {}
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
