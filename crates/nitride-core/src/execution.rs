use crate::*;
use std::borrow::Cow;

pub type Handler = dyn Fn(&[f64]) -> Result<f64, String> + Send + Sync;
pub type TextHandler = dyn Fn(&str) -> Result<f64, String> + Send + Sync;
pub type ValueProvider = dyn Fn() -> Result<f64, String> + Send + Sync;
pub type ContractHandler = dyn Fn(Inputs<'_>) -> Result<Datum, Failure> + Send + Sync;
pub type ValuesHandler = dyn Fn(&[Datum]) -> Result<Datum, String> + Send + Sync;

/// A runtime value. A type's catalog representation decides which kind its
/// values are; a type without one is `Number`.
#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Number(f64),
    Integer(i64),
    Boolean(bool),
    Text(Box<str>),
    /// Field values in declaration order.
    Record(std::sync::Arc<[Datum]>),
    List(std::sync::Arc<[Datum]>),
}

impl Datum {
    pub fn number(&self) -> Option<f64> {
        match self {
            Datum::Number(n) => Some(*n),
            _ => None,
        }
    }
}

impl std::fmt::Display for Datum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Datum::Number(n) => write!(f, "{n}"),
            Datum::Integer(i) => write!(f, "{i}"),
            Datum::Boolean(b) => write!(f, "{b}"),
            Datum::Text(t) => write!(f, "{t:?}"),
            Datum::Record(fields) => {
                write!(f, "(")?;
                for (i, field) in fields.iter().enumerate() {
                    write!(f, "{}{field}", if i > 0 { ", " } else { "" })?;
                }
                write!(f, ")")
            }
            Datum::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    write!(f, "{}{item}", if i > 0 { ", " } else { "" })?;
                }
                write!(f, "]")
            }
        }
    }
}

impl PartialEq<f64> for Datum {
    fn eq(&self, other: &f64) -> bool {
        self.number() == Some(*other)
    }
}

/// An interface operation's inputs: values, or the exact source spelling of
/// its single `Core.Text` input.
#[derive(Debug, Clone, Copy)]
pub enum Inputs<'a> {
    Values(&'a [Datum]),
    Text(&'a str),
}

/// A failure an interface operation declares, by catalog id (`Calendar.InvalidDate`).
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub id: Cow<'static, str>,
    pub message: Cow<'static, str>,
}

pub enum HostHandler {
    Numbers(Box<Handler>),
    /// Receives the exact source spelling of the operation's single `Core.Text` input.
    Text(Box<TextHandler>),
    /// Takes and returns values of any kind: the engine's record and list
    /// operations.
    Values(Box<ValuesHandler>),
    /// Implements an interface operation of the contract `M.I@hash`, which
    /// must be the one the language's catalog declares.
    Contract {
        contract: Cow<'static, str>,
        run: Box<ContractHandler>,
    },
}
pub struct HostBinding<'g> {
    pub signature: Signature<'g>,
    pub handler: HostHandler,
}

/// Why a host call returned no value.
#[derive(Debug, Clone, PartialEq)]
pub struct CallError {
    pub message: String,
    /// The declared failure, for interface operations.
    pub failure: Option<Box<str>>,
}

impl<'g> HostBinding<'g> {
    pub fn contract(
        signature: Signature<'g>,
        contract: impl Into<Cow<'static, str>>,
        run: impl Fn(Inputs<'_>) -> Result<Datum, Failure> + Send + Sync + 'static,
    ) -> Self {
        Self {
            signature,
            handler: HostHandler::Contract {
                contract: contract.into(),
                run: Box::new(run),
            },
        }
    }

    /// Calls the handler. A text handler or text contract receives `text`;
    /// any other receives `inputs`. A built-in handler takes numbers only, and
    /// `numbers` is its scratch space.
    pub fn call(
        &self,
        inputs: &[Datum],
        text: Option<&str>,
        numbers: &mut Vec<f64>,
    ) -> Result<Datum, CallError> {
        let plain = |message: String| CallError {
            message,
            failure: None,
        };
        match (&self.handler, text) {
            (HostHandler::Numbers(handler), _) => {
                numbers.clear();
                for input in inputs {
                    numbers.push(
                        input
                            .number()
                            .ok_or_else(|| plain("a built-in handler takes numbers".into()))?,
                    );
                }
                handler(numbers).map(Datum::Number).map_err(plain)
            }
            (HostHandler::Text(handler), Some(text)) => {
                handler(text).map(Datum::Number).map_err(plain)
            }
            (HostHandler::Text(_), None) => Err(plain("text handler without text".into())),
            (HostHandler::Values(handler), _) => handler(inputs).map_err(plain),
            (HostHandler::Contract { run, .. }, _) => {
                let inputs = text.map_or(Inputs::Values(inputs), Inputs::Text);
                run(inputs).map_err(|failure| CallError {
                    message: failure.message.into_owned(),
                    failure: Some(failure.id.into()),
                })
            }
        }
    }
    pub fn new(
        signature: Signature<'g>,
        handler: impl Fn(&[f64]) -> Result<f64, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            signature,
            handler: HostHandler::Numbers(Box::new(handler)),
        }
    }
    pub fn text(
        signature: Signature<'g>,
        handler: impl Fn(&str) -> Result<f64, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            signature,
            handler: HostHandler::Text(Box::new(handler)),
        }
    }
}
/// A built-in value supplied by the host for an exact (kind, name, type).
/// Names are owned when they come from a language loaded at run time.
pub struct HostValue {
    pub kind: Cow<'static, str>,
    pub name: Cow<'static, str>,
    pub ty: Cow<'static, str>,
    pub provider: Box<ValueProvider>,
}
impl HostValue {
    pub fn new(
        kind: impl Into<Cow<'static, str>>,
        name: impl Into<Cow<'static, str>>,
        ty: impl Into<Cow<'static, str>>,
        provider: impl Fn() -> Result<f64, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind: kind.into(),
            name: name.into(),
            ty: ty.into(),
            provider: Box::new(provider),
        }
    }
}
#[derive(Debug)]
pub struct ExecutionReport<'g> {
    pub values: Vec<Datum>,
    pub trace: Vec<&'g str>,
}
#[derive(Debug)]
pub struct ExecutionFailure<'g> {
    pub diagnostic: Diagnostic,
    pub trace: Vec<&'g str>,
}
pub fn preflight(program: &Program, bindings: &[HostBinding]) -> Result<(), Vec<Diagnostic>> {
    preflight_with_values(program, bindings, &[])
}

fn host_value<'a>(values: &'a [HostValue], builtin: &BuiltinValue) -> Option<&'a HostValue> {
    let mut matches = values
        .iter()
        .filter(|v| v.kind == builtin.kind && v.name == builtin.name);
    let found = matches.next();
    found.filter(|v| matches.next().is_none() && v.ty == builtin.ty)
}

pub fn preflight_with_values(
    program: &Program,
    bindings: &[HostBinding],
    values: &[HostValue],
) -> Result<(), Vec<Diagnostic>> {
    if !program.diagnostics().is_empty() {
        return Err(program.diagnostics().to_vec());
    }
    let mut errors = Vec::new();
    let mut previous_root = None;
    let mut visible = std::collections::HashMap::new();
    let (mut texts, mut consumed_texts) = (0usize, 0usize);
    for statement in &program.statements {
        if statement.value as usize >= program.nodes.len()
            || previous_root.is_some_and(|root| statement.value <= root)
        {
            errors.push(Diagnostic::new(
                "host",
                [0, 0],
                "invalid statement root order",
            ));
            continue;
        }
        for (index, node) in program
            .nodes
            .iter()
            .enumerate()
            .take(statement.value as usize + 1)
            .skip(previous_root.map_or(0, |r| r as usize + 1))
        {
            match &node.kind {
                HirKind::Error => errors.push(Diagnostic::new(
                    "host",
                    node.span,
                    "damaged value cannot execute",
                )),
                HirKind::Constant(number) if !number.is_finite() => {
                    errors.push(Diagnostic::new("host", node.span, "nonfinite constant"))
                }
                HirKind::Reference(symbol) => {
                    if visible.get(symbol) != Some(&node.ty) {
                        errors.push(Diagnostic::new(
                            "host",
                            node.span,
                            "missing or mistyped symbol input",
                        ));
                    }
                }
                HirKind::Builtin(builtin) => {
                    if node.ty != builtin.ty || host_value(values, builtin).is_none() {
                        errors.push(Diagnostic::new(
                            "host",
                            node.span,
                            "missing, duplicate or mistyped host value",
                        ));
                    }
                }
                HirKind::Text { start, end } => {
                    texts += 1;
                    if node.ty != TEXT
                        || index == statement.value as usize
                        || program.text.get(*start as usize..*end as usize).is_none()
                    {
                        errors.push(Diagnostic::new("host", node.span, "invalid text HIR"));
                    }
                }
                HirKind::Operation { operation, args } => {
                    let Some(signature) = program.language.operations.get(*operation as usize)
                    else {
                        errors.push(Diagnostic::new("host", node.span, "unknown operation"));
                        continue;
                    };
                    let matches: Vec<_> = bindings
                        .iter()
                        .filter(|b| b.signature.id == signature.id)
                        .collect();
                    let text_input = signature.inputs.contains(&TEXT);
                    // `M.I@hash` for an interface operation, else a host handler name.
                    let declared = program
                        .language
                        .handlers
                        .get(*operation as usize)
                        .filter(|h| h.contains('@'));
                    let compatible = |handler: &HostHandler| match (handler, declared) {
                        (HostHandler::Contract { .. }, declared) => declared.is_some(),
                        (_, Some(_)) => false,
                        (HostHandler::Text(_), None) => text_input,
                        (HostHandler::Numbers(_) | HostHandler::Values(_), None) => !text_input,
                    };
                    if matches.len() != 1
                        || matches[0].signature != *signature
                        || !compatible(&matches[0].handler)
                        || (text_input && signature.inputs.len() != 1)
                    {
                        errors.push(Diagnostic::new(
                            "host",
                            node.span,
                            "missing, duplicate or incompatible host binding",
                        ));
                    } else if let (HostHandler::Contract { contract, .. }, Some(declared)) =
                        (&matches[0].handler, declared)
                        && contract != declared
                    {
                        errors.push(Diagnostic::new(
                            "host",
                            node.span,
                            format!(
                                "binding for '{}' implements contract {contract}, the catalog declares {declared}",
                                signature.id
                            ),
                        ));
                    }
                    let operands = program.args.get(args.start as usize..args.end as usize);
                    if node.ty != signature.result
                        || operands.is_none_or(|a| {
                            a.len() != signature.inputs.len()
                                || a.iter().zip(signature.inputs).any(|(id, ty)| {
                                    *id as usize >= index
                                        || program.nodes[*id as usize].ty != *ty
                                        || (*ty == TEXT)
                                            != matches!(
                                                program.nodes[*id as usize].kind,
                                                HirKind::Text { .. }
                                            )
                                })
                        })
                    {
                        errors.push(Diagnostic::new(
                            "host",
                            node.span,
                            "invalid typed HIR arguments",
                        ));
                    }
                    consumed_texts += operands.map_or(0, |a| {
                        a.iter()
                            .filter(|id| {
                                program
                                    .nodes
                                    .get(**id as usize)
                                    .is_some_and(|n| matches!(n.kind, HirKind::Text { .. }))
                            })
                            .count()
                    });
                }
                _ => {}
            }
        }
        if let Some(symbol) = statement.symbol {
            visible.insert(symbol, program.nodes[statement.value as usize].ty);
        }
        previous_root = Some(statement.value);
    }
    if program
        .statements
        .last()
        .map(|s| s.value as usize + 1)
        .unwrap_or(0)
        != program.nodes.len()
    {
        errors.push(Diagnostic::new("host", [0, 0], "unowned HIR nodes"));
    }
    if texts != consumed_texts {
        errors.push(Diagnostic::new(
            "host",
            [0, 0],
            "unconsumed or shared text HIR",
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn execute<'g>(
    program: &Program<'g>,
    bindings: &[HostBinding],
    limits: Limits,
) -> Result<ExecutionReport<'g>, ExecutionFailure<'g>> {
    execute_with_values(program, bindings, &[], limits)
}

/// Executes after complete preflight. Each built-in value is read at most once.
pub fn execute_with_values<'g>(
    program: &Program<'g>,
    bindings: &[HostBinding],
    host_values: &[HostValue],
    limits: Limits,
) -> Result<ExecutionReport<'g>, ExecutionFailure<'g>> {
    if let Err(errors) = preflight_with_values(program, bindings, host_values) {
        return Err(ExecutionFailure {
            diagnostic: errors.into_iter().next().unwrap(),
            trace: vec![],
        });
    }
    if program.nodes.len() as u64 > limits.work {
        return Err(ExecutionFailure {
            diagnostic: Diagnostic::new("limit", [0, 0], "execution work limit exceeded"),
            trace: vec![],
        });
    }
    let mut report = ExecutionReport {
        values: Vec::new(),
        trace: Vec::new(),
    };
    let mut values: Vec<Datum> = Vec::with_capacity(program.nodes.len());
    let mut symbols = std::collections::HashMap::<u32, Datum>::new();
    let mut builtins = Vec::<(&'static BuiltinValue, f64)>::new();
    let mut args_buffer = Vec::new();
    let mut numbers = Vec::new();
    for statement in &program.statements {
        while values.len() <= statement.value as usize {
            let node = &program.nodes[values.len()];
            let result = match &node.kind {
                HirKind::Error => Err(("damaged value cannot execute".into(), None)),
                HirKind::Constant(n) => Ok(Datum::Number(*n)),
                HirKind::Reference(symbol) => Ok(symbols[symbol].clone()),
                // Text is consumed only by its operation's text handler.
                HirKind::Text { .. } => Ok(Datum::Number(0.0)),
                HirKind::Builtin(builtin) => {
                    match builtins.iter().find(|(b, _)| std::ptr::eq(*b, *builtin)) {
                        Some((_, value)) => Ok(Datum::Number(*value)),
                        None => {
                            let read = (host_value(host_values, builtin).unwrap().provider)();
                            if let Ok(value) = read {
                                builtins.push((builtin, value));
                            }
                            read.map(Datum::Number).map_err(|e| (e, None))
                        }
                    }
                }
                HirKind::Operation { operation, args } => {
                    let signature = program.language.operations[*operation as usize];
                    let binding = bindings
                        .iter()
                        .find(|b| b.signature.id == signature.id)
                        .unwrap();
                    let operands = &program.args[args.start as usize..args.end as usize];
                    report.trace.push(signature.id);
                    // Preflight admits a text argument only as an operation's single input.
                    let text = match operands.first().map(|id| &program.nodes[*id as usize].kind) {
                        Some(HirKind::Text { start, end }) => {
                            Some(&program.text[*start as usize..*end as usize])
                        }
                        _ => None,
                    };
                    args_buffer.clear();
                    if text.is_none() {
                        args_buffer.extend(operands.iter().map(|id| values[*id as usize].clone()));
                    }
                    binding
                        .call(&args_buffer, text, &mut numbers)
                        .map_err(|e| (e.message, e.failure))
                }
            };
            let value = match result {
                Ok(value) if value.number().is_none_or(f64::is_finite) => value,
                other => {
                    let (message, failure) = other
                        .err()
                        .unwrap_or_else(|| ("nonfinite result".into(), None));
                    let mut diagnostic = Diagnostic::new("execution", node.span, message);
                    diagnostic.code = failure;
                    return Err(ExecutionFailure {
                        diagnostic,
                        trace: report.trace,
                    });
                }
            };
            values.push(value);
        }
        let value = values[statement.value as usize].clone();
        if let Some(symbol) = statement.symbol {
            symbols.insert(symbol, value);
        } else {
            report.values.push(value);
        }
    }
    Ok(report)
}
