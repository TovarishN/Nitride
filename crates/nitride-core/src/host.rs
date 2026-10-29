//! The host handlers a catalog names with `= host h`, declared once: the
//! built-in `Core.Host` interface and its Rust binding. Catalog preparation,
//! codegen, the fixture host and the editor all read these tables.
use crate::*;

/// How an operation handler takes its inputs.
#[derive(Debug, Clone, Copy)]
pub enum Run {
    /// `arity` scalar inputs.
    Numbers {
        arity: usize,
        eval: fn(&[f64]) -> f64,
    },
    /// The exact source spelling of the operation's single `Core.Text` input.
    Text(fn(&str) -> Result<f64, String>),
}

#[derive(Debug)]
pub struct OperationHandler {
    pub name: &'static str,
    pub run: Run,
    /// What the handler computes, for hovers.
    pub description: &'static str,
}

impl OperationHandler {
    pub fn arity(&self) -> usize {
        match self.run {
            Run::Numbers { arity, .. } => arity,
            Run::Text(_) => 1,
        }
    }

    pub fn takes_text(&self) -> bool {
        matches!(self.run, Run::Text(_))
    }

    /// This handler bound to an operation of that signature.
    pub fn binding<'g>(&self, signature: Signature<'g>) -> HostBinding<'g> {
        match self.run {
            Run::Numbers { arity, eval } => HostBinding::new(signature, move |a| {
                if a.len() == arity {
                    Ok(eval(a))
                } else {
                    Err("missing argument".into())
                }
            }),
            Run::Text(run) => HostBinding::text(signature, run),
        }
    }
}

/// Where a built-in value handler reads its value.
#[derive(Debug, Clone, Copy)]
pub enum Read {
    Constant(f64),
    /// The run's day number for today, supplied by the host.
    Today,
}

#[derive(Debug)]
pub struct ValueHandler {
    pub name: &'static str,
    pub read: Read,
    pub description: &'static str,
}

#[derive(Debug)]
pub struct PredicateHandler {
    pub name: &'static str,
    pub predicate: Predicate,
    pub description: &'static str,
}

const ISO_DATE: &str =
    "the day number (days since 0001-01-01) of an ISO `yyyy-mm-dd` date as written";

fn iso_date(text: &str) -> Result<f64, String> {
    iso_date_day_number(text)
        .map(|day| day as f64)
        .ok_or_else(|| "not a calendar date".into())
}

/// Operation handlers, in the order diagnostics list them.
pub const OPERATIONS: &[OperationHandler] = &[
    OperationHandler {
        name: "zero",
        run: Run::Numbers {
            arity: 0,
            eval: |_| 0.0,
        },
        description: "zero",
    },
    OperationHandler {
        name: "identity",
        run: Run::Numbers {
            arity: 1,
            eval: |a| a[0],
        },
        description: "its input, unchanged",
    },
    OperationHandler {
        name: "neg",
        run: Run::Numbers {
            arity: 1,
            eval: |a| -a[0],
        },
        description: "its input, negated",
    },
    OperationHandler {
        name: "add",
        run: Run::Numbers {
            arity: 2,
            eval: |a| a[0] + a[1],
        },
        description: "the sum of its two inputs",
    },
    OperationHandler {
        name: "sub",
        run: Run::Numbers {
            arity: 2,
            eval: |a| a[0] - a[1],
        },
        description: "the first input minus the second",
    },
    OperationHandler {
        name: "mul",
        run: Run::Numbers {
            arity: 2,
            eval: |a| a[0] * a[1],
        },
        description: "the product of its two inputs",
    },
    OperationHandler {
        name: "div",
        run: Run::Numbers {
            arity: 2,
            eval: |a| a[0] / a[1],
        },
        description: "the first input divided by the second",
    },
    OperationHandler {
        name: "pow",
        run: Run::Numbers {
            arity: 2,
            eval: |a| a[0].powf(a[1]),
        },
        description: "the first input raised to the power of the second",
    },
    OperationHandler {
        name: "iso-date",
        run: Run::Text(iso_date),
        description: ISO_DATE,
    },
];

/// Built-in value handlers, in the order diagnostics list them.
pub const VALUES: &[ValueHandler] = &[
    ValueHandler {
        name: "pi",
        read: Read::Constant(std::f64::consts::PI),
        description: "π",
    },
    ValueHandler {
        name: "e",
        read: Read::Constant(std::f64::consts::E),
        description: "Euler's number *e*",
    },
    ValueHandler {
        name: "tau",
        read: Read::Constant(std::f64::consts::TAU),
        description: "τ (2π)",
    },
    ValueHandler {
        name: "today",
        read: Read::Today,
        description: "today's date as a day number, in UTC",
    },
];

/// Check predicates, in the order diagnostics list them.
pub const PREDICATES: &[PredicateHandler] = &[PredicateHandler {
    name: "iso-date",
    predicate: Predicate::IsoDate,
    description: ISO_DATE,
}];

pub fn operation(name: &str) -> Option<&'static OperationHandler> {
    OPERATIONS.iter().find(|h| h.name == name)
}

pub fn value(name: &str) -> Option<&'static ValueHandler> {
    VALUES.iter().find(|h| h.name == name)
}

pub fn predicate(name: &str) -> Option<&'static PredicateHandler> {
    PREDICATES.iter().find(|h| h.name == name)
}

/// What a handler of any kind computes, for hovers; `None` for an unknown name.
pub fn describe(name: &str) -> Option<&'static str> {
    operation(name)
        .map(|h| h.description)
        .or_else(|| value(name).map(|h| h.description))
        .or_else(|| predicate(name).map(|h| h.description))
}

/// A binding for every catalog operation whose handler is known.
pub fn bindings<'g>(language: &'g Language<'g>) -> Vec<HostBinding<'g>> {
    language
        .operations
        .iter()
        .zip(language.handlers)
        .filter_map(|(signature, name)| Some(operation(name)?.binding(*signature)))
        .collect()
}

/// Host values for every built-in value the language declares, with `today`
/// as the day number `Read::Today` reads.
pub fn host_values(language: &Language<'_>, today: f64) -> Vec<HostValue> {
    let mut values: Vec<HostValue> = Vec::new();
    let declared = language.rules.iter().flat_map(|r| match r.action {
        Action::Reference { values, .. } => values,
        _ => &[],
    });
    for declared in declared {
        if values
            .iter()
            .any(|v| v.kind == declared.kind && v.name == declared.name)
        {
            continue;
        }
        let Some(handler) = value(declared.handler) else {
            continue;
        };
        let number = match handler.read {
            Read::Constant(number) => number,
            Read::Today => today,
        };
        values.push(HostValue::new(
            declared.kind.to_owned(),
            declared.name.to_owned(),
            declared.ty.to_owned(),
            move || Ok(number),
        ));
    }
    values
}

/// An expression in an interface example.
#[derive(Debug)]
pub enum ExampleExpr<'g> {
    Number(f64),
    /// Only as the single input of a text operation.
    Text(&'g str),
    Call {
        operation: &'g str,
        args: &'g [ExampleExpr<'g>],
    },
}

#[derive(Debug)]
pub enum Expected<'g> {
    Equals(ExampleExpr<'g>),
    /// The declared failure, by catalog id.
    Fails(&'g str),
}

/// A conformance case of an interface, as the catalog writes it.
#[derive(Debug)]
pub struct Example<'g> {
    pub source: &'g str,
    pub value: ExampleExpr<'g>,
    pub expected: Expected<'g>,
}

/// Whether two results agree: exactly, or within a relative 1e-9.
fn agree(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

fn evaluate(
    language: &Language<'_>,
    bindings: &[HostBinding<'_>],
    expr: &ExampleExpr<'_>,
) -> Result<f64, CallError> {
    let plain = |message: String| CallError {
        message,
        failure: None,
    };
    let (operation, args) = match expr {
        ExampleExpr::Number(n) => return Ok(*n),
        ExampleExpr::Text(_) => return Err(plain("text outside a text operation".into())),
        ExampleExpr::Call { operation, args } => (*operation, *args),
    };
    let index = language
        .operations
        .iter()
        .position(|s| s.id == operation)
        .ok_or_else(|| plain(format!("unknown operation '{operation}'")))?;
    let binding = bindings
        .iter()
        .find(|b| b.signature.id == operation)
        .ok_or_else(|| plain(format!("no binding for '{operation}'")))?;
    let declared = language.handlers.get(index).filter(|h| h.contains('@'));
    match (&binding.handler, declared) {
        (HostHandler::Contract { contract, .. }, Some(declared)) if contract != declared => {
            return Err(plain(format!(
                "binding for '{operation}' implements contract {contract}, the catalog declares {declared}"
            )));
        }
        _ => {}
    }
    let result = match args {
        [ExampleExpr::Text(text)] => binding.call(&[], Some(text)),
        _ => {
            let numbers = args
                .iter()
                .map(|a| evaluate(language, bindings, a))
                .collect::<Result<Vec<_>, _>>()?;
            binding.call(&numbers, None)
        }
    }?;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(plain("nonfinite result".into()))
    }
}

/// Runs interface examples through `bindings`: `(example, why)` for each one
/// they fail. Empty when the bindings conform.
pub fn conform<'e>(
    language: &Language<'_>,
    bindings: &[HostBinding<'_>],
    examples: &'e [Example<'e>],
) -> Vec<(&'e str, String)> {
    let mut failed = Vec::new();
    for example in examples {
        let value = evaluate(language, bindings, &example.value);
        let why = match (&example.expected, value) {
            (Expected::Equals(expected), Ok(value)) => {
                match evaluate(language, bindings, expected) {
                    Ok(expected) if agree(value, expected) => None,
                    Ok(expected) => Some(format!("got {value}, expected {expected}")),
                    Err(e) => Some(format!("expected side failed: {}", e.message)),
                }
            }
            (Expected::Fails(id), Err(e)) if e.failure.as_deref() == Some(*id) => None,
            (Expected::Fails(id), Ok(value)) => Some(format!("got {value}, expected {id}")),
            (_, Err(e)) => Some(match e.failure {
                Some(failure) => format!("failed with {failure} ({})", e.message),
                None => e.message,
            }),
        };
        if let Some(why) = why {
            failed.push((example.source, why));
        }
    }
    failed
}

/// Today as a day number (days since 0001-01-01), in UTC.
pub fn utc_today() -> f64 {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (719_162 + seconds / 86_400) as f64
}
