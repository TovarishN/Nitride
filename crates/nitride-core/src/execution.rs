use crate::*;

pub type Handler = dyn Fn(&[f64]) -> Result<f64, String> + Send + Sync;
pub struct HostBinding {
    pub signature: Signature,
    pub handler: Box<Handler>,
}
impl HostBinding {
    pub fn new(
        signature: Signature,
        handler: impl Fn(&[f64]) -> Result<f64, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            signature,
            handler: Box::new(handler),
        }
    }
}
#[derive(Debug)]
pub struct ExecutionReport {
    pub values: Vec<f64>,
    pub trace: Vec<&'static str>,
}
#[derive(Debug)]
pub struct ExecutionFailure {
    pub diagnostic: Diagnostic,
    pub trace: Vec<&'static str>,
}
pub fn preflight(program: &Program, bindings: &[HostBinding]) -> Result<(), Vec<Diagnostic>> {
    let mut errors = Vec::new();
    let mut previous_root = None;
    let mut visible = std::collections::HashMap::new();
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
                    if matches.len() != 1 || matches[0].signature != *signature {
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
                                    *id as usize >= index || program.nodes[*id as usize].ty != *ty
                                })
                        })
                    {
                        errors.push(Diagnostic::new(
                            "host",
                            node.span,
                            "invalid typed HIR arguments",
                        ));
                    }
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
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn execute(
    program: &Program,
    bindings: &[HostBinding],
    limits: Limits,
) -> Result<ExecutionReport, ExecutionFailure> {
    if let Err(errors) = preflight(program, bindings) {
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
    let mut args_buffer = Vec::new();
    for statement in &program.statements {
        while values.len() <= statement.value as usize {
            let node = &program.nodes[values.len()];
            let result = match &node.kind {
                HirKind::Constant(n) => Ok(*n),
                HirKind::Reference(symbol) => Ok(symbols[symbol]),
                HirKind::Operation { operation, args } => {
                    let signature = program.language.operations[*operation as usize];
                    let binding = bindings
                        .iter()
                        .find(|b| b.signature.id == signature.id)
                        .unwrap();
                    args_buffer.clear();
                    args_buffer.extend(
                        program.args[args.start as usize..args.end as usize]
                            .iter()
                            .map(|id| values[*id as usize]),
                    );
                    report.trace.push(signature.id);
                    (binding.handler)(&args_buffer)
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
