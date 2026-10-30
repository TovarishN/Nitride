//! Rust for a catalog's interfaces: per interface, a trait an implementation
//! fills in, its declared failures, the records its signatures use, a binder
//! that turns an implementation into host bindings, and the interface's
//! examples as a conformance run.
use crate::{array, quoted, strings, text};
use serde_json::Value;
use std::fmt::Write;

/// `ParseDate` → `parse_date`.
pub(crate) fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The last segment of a qualified name.
pub(crate) fn tail(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// Parameter names from input types; a repeated type is numbered.
pub(crate) fn parameters(inputs: &[&str]) -> Vec<String> {
    inputs
        .iter()
        .enumerate()
        .map(|(i, ty)| {
            let base = snake(tail(ty));
            if inputs.iter().filter(|t| tail(t) == tail(ty)).count() > 1 {
                let n = inputs[..=i].iter().filter(|t| tail(t) == tail(ty)).count();
                format!("{base}_{n}")
            } else {
                base
            }
        })
        .collect()
}

/// How values of a type cross a binding.
pub(crate) enum Shape<'a> {
    /// `Core.Text`: the exact spelling a grammar passes.
    Source,
    F64,
    I64,
    Bool,
    Text,
    Record(Vec<(&'a str, &'a str)>),
    List(&'a str),
}

pub(crate) fn shape<'a>(catalog: &'a Value, ty: &str) -> Result<Shape<'a>, String> {
    if ty == crate::TEXT {
        return Ok(Shape::Source);
    }
    let representation = &catalog["representations"][ty];
    Ok(match representation {
        Value::Null => Shape::F64,
        Value::String(kind) => match kind.as_str() {
            "i64" => Shape::I64,
            "bool" => Shape::Bool,
            "text" => Shape::Text,
            _ => Shape::F64,
        },
        _ => match (representation.get("record"), representation.get("list")) {
            (Some(fields), _) => Shape::Record(
                array(fields)?
                    .iter()
                    .map(|f| Ok((text(&f[0])?, text(&f[1])?)))
                    .collect::<Result<_, String>>()?,
            ),
            (_, Some(element)) => Shape::List(text(element)?),
            _ => return Err(format!("invalid representation of {ty}")),
        },
    })
}

/// The suffix of a type's converters: `Calendar.Date` → `calendar_date`.
fn key(ty: &str) -> String {
    ty.split('.').map(snake).collect::<Vec<_>>().join("_")
}

/// The owned Rust type of a type's values.
fn rust(catalog: &Value, ty: &str) -> Result<String, String> {
    Ok(match shape(catalog, ty)? {
        Shape::Source | Shape::Text => "String".into(),
        Shape::F64 => "f64".into(),
        Shape::I64 => "i64".into(),
        Shape::Bool => "bool".into(),
        Shape::Record(_) => tail(ty).into(),
        Shape::List(element) => format!("Vec<{}>", rust(catalog, element)?),
    })
}

/// `ty` and every type inside it, each once, components first.
pub(crate) fn closure<'a>(
    catalog: &'a Value,
    ty: &'a str,
    into: &mut Vec<&'a str>,
) -> Result<(), String> {
    if into.contains(&ty) || ty == crate::TEXT {
        return Ok(());
    }
    match shape(catalog, ty)? {
        Shape::Record(fields) => {
            for (_, field) in fields {
                closure(catalog, field, into)?;
            }
        }
        Shape::List(element) => closure(catalog, element, into)?,
        _ => {}
    }
    into.push(ty);
    Ok(())
}

/// A record's struct, and `get_`/`put_` converters between a type's values
/// and `Datum`.
fn converters(catalog: &Value, ty: &str, out: &mut String) -> Result<(), String> {
    let k = key(ty);
    let t = rust(catalog, ty)?;
    let (get, put) = match shape(catalog, ty)? {
        Shape::Source => return Ok(()),
        Shape::F64 => (
            "match d { Datum::Number(v) => Some(*v), _ => None }".to_string(),
            "Datum::Number(v)".to_string(),
        ),
        Shape::I64 => (
            "match d { Datum::Integer(v) => Some(*v), _ => None }".into(),
            "Datum::Integer(v)".into(),
        ),
        Shape::Bool => (
            "match d { Datum::Boolean(v) => Some(*v), _ => None }".into(),
            "Datum::Boolean(v)".into(),
        ),
        Shape::Text => (
            "match d { Datum::Text(v) => Some(v.to_string()), _ => None }".into(),
            "Datum::Text(v.into())".into(),
        ),
        Shape::Record(fields) => {
            writeln!(
                out,
                "    /// `{ty}`.\n    #[derive(Debug, Clone, PartialEq)]\n    pub struct {t} {{"
            )
            .unwrap();
            for (name, field) in &fields {
                writeln!(
                    out,
                    "        pub {}: {},",
                    snake(name),
                    rust(catalog, field)?
                )
                .unwrap();
            }
            writeln!(out, "    }}\n").unwrap();
            let gets = fields
                .iter()
                .enumerate()
                .map(|(i, (name, field))| format!("{}: get_{}(&f[{i}])?", snake(name), key(field)))
                .collect::<Vec<_>>()
                .join(", ");
            let puts = fields
                .iter()
                .map(|(name, field)| format!("put_{}(v.{})", key(field), snake(name)))
                .collect::<Vec<_>>()
                .join(", ");
            (
                format!(
                    "match d {{ Datum::Record(f) if f.len() == {} => Some({t} {{ {gets} }}), _ => None }}",
                    fields.len()
                ),
                format!("Datum::Record(vec![{puts}].into())"),
            )
        }
        Shape::List(element) => (
            format!(
                "match d {{ Datum::List(items) => items.iter().map(get_{}).collect(), _ => None }}",
                key(element)
            ),
            format!(
                "Datum::List(v.into_iter().map(put_{}).collect())",
                key(element)
            ),
        ),
    };
    writeln!(
        out,
        "    #[allow(dead_code)]\n    fn get_{k}(d: &Datum) -> Option<{t}> {{\n        {get}\n    }}\n\n    #[allow(dead_code)]\n    fn put_{k}(v: {t}) -> Datum {{\n        {put}\n    }}\n"
    )
    .unwrap();
    Ok(())
}

fn expr(value: &Value) -> Result<String, String> {
    if let Some(b) = value.get("boolean") {
        let b = b.as_bool().ok_or("example boolean")?;
        return Ok(format!("nitride_core::host::ExampleExpr::Boolean({b})"));
    }
    if let Some(n) = value.get("number") {
        let n = n.as_f64().ok_or("example number")?;
        return Ok(format!("nitride_core::host::ExampleExpr::Number({n:?})"));
    }
    if let Some(t) = value.get("text") {
        return Ok(format!(
            "nitride_core::host::ExampleExpr::Text({})",
            quoted(text(t)?)
        ));
    }
    let args = array(&value["args"])?
        .iter()
        .map(expr)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!(
        "nitride_core::host::ExampleExpr::Call {{ operation: {}, args: &[{}] }}",
        quoted(text(&value["call"])?),
        args.join(", ")
    ))
}

/// Rust for every interface in prepared language JSON, as `generate` takes it.
/// Empty when the catalog declares none.
pub fn interfaces(input: &str) -> Result<String, String> {
    let data: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let catalog = &data["catalog"];
    let Some(interfaces) = catalog.get("interfaces").and_then(Value::as_object) else {
        return Ok(String::new());
    };
    let operations = array(&catalog["operations"])?;
    let failures = &catalog["failures"];
    let mut out = String::from(
        "// Generated by nitride-codegen from the catalog's interfaces. Do not edit.\n",
    );
    for (id, interface) in interfaces {
        let name = tail(id);
        let failure_enum = format!("{name}Failure");
        let hash = text(&interface["hash"])?;
        let version = interface["version"].as_u64().ok_or("interface version")?;
        let members = strings(&interface["operations"])?
            .into_iter()
            .map(|op| {
                operations
                    .iter()
                    .find(|o| o["id"] == op)
                    .ok_or_else(|| format!("unknown interface operation {op}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Every failure the interface's operations declare, in first-use order.
        let mut declared: Vec<&str> = Vec::new();
        for op in &members {
            for f in strings(&op["fails"])? {
                if !declared.contains(&f) {
                    declared.push(f);
                }
            }
        }
        // Every type the signatures use, with the types inside records and lists.
        let mut types = Vec::new();
        for op in &members {
            for ty in strings(&op["inputs"])?
                .into_iter()
                .chain([text(&op["result"])?])
            {
                closure(catalog, ty, &mut types)?;
            }
        }
        let failure = |f: &str| tail(f).to_string();

        writeln!(out, "\n/// `{id}` version {version}.").unwrap();
        writeln!(out, "pub mod {} {{", snake(name)).unwrap();
        writeln!(
            out,
            "    use nitride_core::{{Datum, Failure, HostBinding, Inputs, Language}};\n    use std::sync::Arc;\n"
        )
        .unwrap();
        writeln!(
            out,
            "    /// The contract a binding implements; preflight checks it against the language.\n    pub const CONTRACT: &str = {};\n",
            quoted(&format!("{id}@{hash}"))
        )
        .unwrap();

        writeln!(
            out,
            "    /// The failures `{name}`'s operations declare.\n    #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n    pub enum {failure_enum} {{"
        )
        .unwrap();
        for f in &declared {
            writeln!(out, "        {},", failure(f)).unwrap();
        }
        writeln!(out, "    }}\n\n    impl {failure_enum} {{").unwrap();
        writeln!(
            out,
            "        /// The catalog id.\n        pub fn id(self) -> &'static str {{\n            match self {{"
        )
        .unwrap();
        for f in &declared {
            writeln!(
                out,
                "                Self::{} => {},",
                failure(f),
                quoted(f)
            )
            .unwrap();
        }
        writeln!(
            out,
            "            }}\n        }}\n\n        /// The catalog message.\n        pub fn message(self) -> &'static str {{\n            match self {{"
        )
        .unwrap();
        for f in &declared {
            let message = text(&failures[*f]).map_err(|_| format!("undeclared failure {f}"))?;
            writeln!(
                out,
                "                Self::{} => {},",
                failure(f),
                quoted(message)
            )
            .unwrap();
        }
        writeln!(out, "            }}\n        }}\n    }}\n").unwrap();

        for ty in &types {
            converters(catalog, ty, &mut out)?;
        }

        writeln!(
            out,
            "    /// Implemented once per host; any conforming implementation can run the language.\n    pub trait {name}: Send + Sync + 'static {{"
        )
        .unwrap();
        for op in &members {
            let inputs = strings(&op["inputs"])?;
            let fails = strings(&op["fails"])?;
            let params = parameters(&inputs);
            let typed = params
                .iter()
                .zip(&inputs)
                .map(|(p, ty)| {
                    Ok(match shape(catalog, ty)? {
                        Shape::Source | Shape::Text => format!("{p}: &str"),
                        _ => format!("{p}: {}", rust(catalog, ty)?),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let returns = rust(catalog, text(&op["result"])?)?;
            let fails_doc = if fails.is_empty() {
                String::new()
            } else {
                format!(
                    "; may fail with {}",
                    fails.iter().map(|f| tail(f)).collect::<Vec<_>>().join(", ")
                )
            };
            writeln!(
                out,
                "        /// `{}({}) : {}`{fails_doc}.\n        fn {}(&self{}) -> Result<{returns}, {failure_enum}>;",
                tail(text(&op["id"])?),
                inputs.iter().map(|t| tail(t)).collect::<Vec<_>>().join(", "),
                tail(text(&op["result"])?),
                snake(tail(text(&op["id"])?)),
                typed.iter().map(|t| format!(", {t}")).collect::<String>()
            )
            .unwrap();
        }
        writeln!(out, "    }}\n").unwrap();

        writeln!(
            out,
            "    fn checked(result: Result<Datum, {failure_enum}>, declared: &[{failure_enum}]) -> Result<Datum, Failure> {{
        result.map_err(|f| {{
            if declared.contains(&f) {{
                Failure {{ id: f.id().into(), message: f.message().into() }}
            }} else {{
                Failure {{ id: \"host.undeclared\".into(), message: format!(\"undeclared failure {{}}\", f.id()).into() }}
            }}
        }})
    }}

    fn mismatch() -> Failure {{
        Failure {{ id: \"host.inputs\".into(), message: \"inputs do not match the signature\".into() }}
    }}
"
        )
        .unwrap();

        writeln!(
            out,
            "    /// A binding for each of `{name}`'s operations in `language`.
    pub fn bindings<'g>(language: &Language<'g>, implementation: Arc<dyn {name}>) -> Vec<HostBinding<'g>> {{
        let mut bindings = Vec::new();
        for signature in language.operations {{
            let i = implementation.clone();
            bindings.push(match signature.id {{"
        )
        .unwrap();
        for op in &members {
            let inputs = strings(&op["inputs"])?;
            let params = parameters(&inputs);
            let fails = strings(&op["fails"])?
                .iter()
                .map(|f| format!("{failure_enum}::{}", failure(f)))
                .collect::<Vec<_>>()
                .join(", ");
            let method = snake(tail(text(&op["id"])?));
            let put = format!("put_{}", key(text(&op["result"])?));
            let body = if inputs.first() == Some(&crate::TEXT) {
                format!(
                    "let Inputs::Text({p}) = inputs else {{ return Err(mismatch()) }};
                    checked(i.{method}({p}).map({put}), &[{fails}])",
                    p = params[0]
                )
            } else {
                let mut converted = String::new();
                if !params.is_empty() {
                    converted = format!(
                        "\n                    let ({},) = ({},) else {{ return Err(mismatch()) }};",
                        params
                            .iter()
                            .map(|p| format!("Some({p})"))
                            .collect::<Vec<_>>()
                            .join(", "),
                        params
                            .iter()
                            .zip(&inputs)
                            .map(|(p, ty)| format!("get_{}({p})", key(ty)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                let args = params
                    .iter()
                    .zip(&inputs)
                    .map(|(p, ty)| {
                        Ok(match shape(catalog, ty)? {
                            Shape::Text => format!("&{p}"),
                            _ => p.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                format!(
                    "let Inputs::Values([{}]) = inputs else {{ return Err(mismatch()) }};{converted}
                    checked(i.{method}({}).map({put}), &[{fails}])",
                    params.join(", "),
                    args.join(", ")
                )
            };
            writeln!(
                out,
                "                {} => HostBinding::contract(*signature, CONTRACT, move |inputs| {{
                    {body}
                }}),",
                quoted(text(&op["id"])?)
            )
            .unwrap();
        }
        writeln!(
            out,
            "                _ => continue,
            }});
        }}
        bindings
    }}
"
        )
        .unwrap();

        let effects = members
            .iter()
            .filter_map(|op| {
                let effects = strings(op.get("effects")?).ok()?;
                Some(format!(
                    "        ({}, &[{}]),",
                    quoted(op["id"].as_str()?),
                    effects
                        .iter()
                        .map(|e| quoted(e))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
            .collect::<Vec<_>>();
        let examples = array(&interface["examples"])?
            .iter()
            .map(|e| {
                let expected = if let Some(f) = e.get("fails") {
                    format!("nitride_core::host::Expected::Fails({})", quoted(text(f)?))
                } else {
                    format!(
                        "nitride_core::host::Expected::Equals({})",
                        expr(&e["equals"])?
                    )
                };
                Ok(format!(
                    "        nitride_core::host::Example {{ source: {}, value: {}, expected: {expected} }},",
                    quoted(text(&e["source"])?),
                    expr(&e["value"])?
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        writeln!(
            out,
            "    /// The catalog's examples for `{name}`.
    pub static EXAMPLES: &[nitride_core::host::Example<'static>] = &[
{}
    ];

    /// Each operation's effects, by operation id; the host grants them.
    pub static EFFECTS: &[(&str, &[&str])] = &[
{}
    ];

    /// The effects an operation needs.
    pub fn needs(operation: &str) -> Vec<String> {{
        EFFECTS
            .iter()
            .filter(|(op, _)| *op == operation)
            .flat_map(|(_, effects)| effects.iter().map(|e| e.to_string()))
            .collect()
    }}

    /// Runs the examples through `implementation`, with the engine's handlers
    /// for the language's other operations: `(example, why)` for each one it
    /// fails. No effects are granted, so examples that need one are skipped.
    pub fn conform(language: &Language<'_>, implementation: Arc<dyn {name}>) -> Vec<(&'static str, String)> {{
        conformance(language, implementation, &[]).failed
    }}

    /// Runs the examples with `granted` effects: what failed, and what was
    /// skipped for want of an effect.
    pub fn conformance(
        language: &Language<'_>,
        implementation: Arc<dyn {name}>,
        granted: &[String],
    ) -> nitride_core::host::Conformance<'static> {{
        let mut all = nitride_core::host::bindings(language);
        all.extend(nitride_core::host::grant(bindings(language, implementation), &needs, granted));
        nitride_core::host::conformance(language, &all, EXAMPLES)
    }}
}}",
            examples.join("\n"),
            effects.join("\n")
        )
        .unwrap();
    }
    Ok(out)
}
