use crate::*;

pub type Handler = dyn Fn(&[f64]) -> Result<f64, String> + Send + Sync;
pub type TextHandler = dyn Fn(&str) -> Result<f64, String> + Send + Sync;
pub type ValueProvider = dyn Fn() -> Result<f64, String> + Send + Sync;
pub enum HostHandler {
    Numbers(Box<Handler>),
    /// Receives the exact source spelling of the operation's single `Core.Text` input.
    Text(Box<TextHandler>),
}
pub struct HostBinding<'g> {
    pub signature: Signature<'g>,
    pub handler: HostHandler,
}
impl<'g> HostBinding<'g> {
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
pub struct HostValue {
    pub kind: &'static str,
    pub name: &'static str,
    pub ty: &'static str,
    pub provider: Box<ValueProvider>,
}
impl HostValue {
    pub fn new(
        kind: &'static str,
        name: &'static str,
        ty: &'static str,
        provider: impl Fn() -> Result<f64, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            name,
            ty,
            provider: Box::new(provider),
        }
    }
}
#[derive(Debug)]
pub struct ExecutionReport<'g> {
    pub values: Vec<f64>,
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
                    if matches.len() != 1
                        || matches[0].signature != *signature
                        || text_input != matches!(matches[0].handler, HostHandler::Text(_))
                        || (text_input && signature.inputs.len() != 1)
                    {
                        errors.push(Diagnostic::new(
                            "host",
                            node.span,
                            "missing, duplicate or incompatible host binding",
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
    let mut values = Vec::with_capacity(program.nodes.len());
    let mut symbols = std::collections::HashMap::<u32, f64>::new();
    let mut builtins = Vec::<(&'static BuiltinValue, f64)>::new();
    let mut args_buffer = Vec::new();
    for statement in &program.statements {
        while values.len() <= statement.value as usize {
            let node = &program.nodes[values.len()];
            let result = match &node.kind {
                HirKind::Error => Err("damaged value cannot execute".into()),
                HirKind::Constant(n) => Ok(*n),
                HirKind::Reference(symbol) => Ok(symbols[symbol]),
                // Text is consumed only by its operation's text handler.
                HirKind::Text { .. } => Ok(0.0),
                HirKind::Builtin(builtin) => {
                    match builtins.iter().find(|(b, _)| std::ptr::eq(*b, *builtin)) {
                        Some((_, value)) => Ok(*value),
                        None => {
                            let read = (host_value(host_values, builtin).unwrap().provider)();
                            if let Ok(value) = read {
                                builtins.push((builtin, value));
                            }
                            read
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
                    match &binding.handler {
                        HostHandler::Numbers(handler) => {
                            args_buffer.clear();
                            args_buffer.extend(operands.iter().map(|id| values[*id as usize]));
                            handler(&args_buffer)
                        }
                        HostHandler::Text(handler) => {
                            let HirKind::Text { start, end } =
                                program.nodes[operands[0] as usize].kind
                            else {
                                unreachable!("preflight admits only text arguments here")
                            };
                            handler(&program.text[start as usize..end as usize])
                        }
                    }
                }
            };
            let number = match result {
                Ok(n) if n.is_finite() => n,
                other => {
                    return Err(ExecutionFailure {
                        diagnostic: Diagnostic::new(
                            "execution",
                            node.span,
                            other.err().unwrap_or_else(|| "nonfinite result".into()),
                        ),
                        trace: report.trace,
                    });
                }
            };
            values.push(number);
        }
        let value = values[statement.value as usize];
        if let Some(symbol) = statement.symbol {
            symbols.insert(symbol, value);
        } else {
            report.values.push(value);
        }
    }
    Ok(report)
}
