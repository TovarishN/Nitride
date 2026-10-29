//! WIT for a catalog's interfaces: per interface, a package whose world
//! exports it, so a WebAssembly component in any language can implement it.
use crate::interfaces::{Shape, closure, parameters, shape, snake, tail};
use crate::{array, strings, text};
use serde_json::Value;
use std::fmt::Write;

/// WIT keywords; an identifier that is one is written `%name`.
const KEYWORDS: &[&str] = &[
    "as",
    "async",
    "bool",
    "borrow",
    "char",
    "constructor",
    "enum",
    "error-context",
    "export",
    "f32",
    "f64",
    "flags",
    "from",
    "func",
    "future",
    "import",
    "include",
    "interface",
    "list",
    "own",
    "option",
    "package",
    "record",
    "resource",
    "result",
    "s16",
    "s32",
    "s64",
    "s8",
    "static",
    "stream",
    "string",
    "tuple",
    "type",
    "u16",
    "u32",
    "u64",
    "u8",
    "use",
    "variant",
    "with",
    "world",
];

/// `ParseDate` → `parse-date`; `date_2` → `date2`. Each word starts with a
/// letter, as WIT requires.
pub fn kebab(name: &str) -> String {
    let words: Vec<String> =
        snake(name)
            .split('_')
            .filter(|w| !w.is_empty())
            .fold(Vec::new(), |mut words, w| {
                match words.last_mut() {
                    Some(last) if w.starts_with(|c: char| c.is_ascii_digit()) => {
                        last.push_str(w);
                    }
                    _ => words.push(w.to_string()),
                }
                words
            });
    let kebab = words.join("-");
    if KEYWORDS.contains(&kebab.as_str()) {
        format!("%{kebab}")
    } else {
        kebab
    }
}

/// The WIT name of a catalog type: its own name, or `string` for source text.
fn name(ty: &str) -> String {
    if ty == crate::TEXT {
        "string".into()
    } else {
        kebab(tail(ty))
    }
}

/// The WIT declaration of a catalog type, if it needs one.
fn declaration(catalog: &Value, ty: &str) -> Result<Option<String>, String> {
    let alias = |of: &str| Some(format!("  type {} = {of};", name(ty)));
    Ok(match shape(catalog, ty)? {
        Shape::Source => None,
        Shape::F64 => alias("f64"),
        Shape::I64 => alias("s64"),
        Shape::Bool => alias("bool"),
        Shape::Text => alias("string"),
        Shape::List(element) => alias(&format!("list<{}>", name(element))),
        Shape::Record(fields) => Some(format!(
            "  record {} {{\n{}  }}",
            name(ty),
            fields
                .iter()
                .map(|(field, ty)| format!("    {}: {},\n", kebab(field), name(ty)))
                .collect::<String>()
        )),
    })
}

/// WIT for every interface in prepared language JSON, as `(interface id,
/// document)`, in catalog order.
pub fn wit(input: &str) -> Result<Vec<(String, String)>, String> {
    let data: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let catalog = &data["catalog"];
    let Some(interfaces) = catalog.get("interfaces").and_then(Value::as_object) else {
        return Ok(vec![]);
    };
    interfaces
        .iter()
        .map(|(id, interface)| Ok((id.clone(), document(catalog, id, interface, None)?)))
        .collect()
}

/// WIT for each of a prepared project's bindings, as `(component path,
/// document)`: the interface's world with the binding's export names, which
/// is what the component must export.
pub fn binding_wit(input: &str) -> Result<Vec<(String, String)>, String> {
    let data: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let catalog = &data["catalog"];
    let Some(bindings) = data.get("bindings").and_then(Value::as_array) else {
        return Ok(vec![]);
    };
    bindings
        .iter()
        .map(|binding| {
            let id = text(&binding["interface"])?;
            let interface = &catalog["interfaces"][id];
            let exports = binding["exports"].as_object().ok_or("binding exports")?;
            Ok((
                text(&binding["component"])?.to_string(),
                document(catalog, id, interface, Some(exports))?,
            ))
        })
        .collect()
}

/// One interface's WIT package, its operations exported under `exports`
/// (by operation id) or their own WIT names.
fn document(
    catalog: &Value,
    id: &str,
    interface: &Value,
    exports: Option<&serde_json::Map<String, Value>>,
) -> Result<String, String> {
    let operations = array(&catalog["operations"])?;
    {
        let module = id.rsplit_once('.').map_or("", |(m, _)| m);
        let interface_name = kebab(tail(id));
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
        let mut types = Vec::new();
        let mut failures: Vec<&str> = Vec::new();
        for op in &members {
            for ty in strings(&op["inputs"])?
                .into_iter()
                .chain([text(&op["result"])?])
            {
                closure(catalog, ty, &mut types)?;
            }
            for f in strings(&op["fails"])? {
                if !failures.contains(&f) {
                    failures.push(f);
                }
            }
        }
        let failure_enum = format!("{}-failure", kebab(tail(id)));
        let mut out = String::new();
        writeln!(
            out,
            "// Generated by nitride-codegen from the catalog. Do not edit.\n// Contract {id}@{}",
            text(&interface["hash"])?
        )
        .unwrap();
        writeln!(
            out,
            "package nitride:{}@{version}.0.0;\n\ninterface {interface_name} {{",
            kebab(&module.replace('.', "")),
        )
        .unwrap();
        for ty in &types {
            if let Some(declaration) = declaration(catalog, ty)? {
                writeln!(out, "{declaration}").unwrap();
            }
        }
        if !failures.is_empty() {
            writeln!(
                out,
                "  enum {failure_enum} {{\n{}  }}",
                failures
                    .iter()
                    .map(|f| format!("    {},\n", kebab(tail(f))))
                    .collect::<String>()
            )
            .unwrap();
        }
        for op in &members {
            let inputs = strings(&op["inputs"])?;
            let params = parameters(&inputs)
                .iter()
                .zip(&inputs)
                .map(|(p, ty)| format!("{}: {}", kebab(p), name(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            let result = name(text(&op["result"])?);
            let result = if failures.is_empty() {
                result
            } else {
                format!("result<{result}, {failure_enum}>")
            };
            let op_id = text(&op["id"])?;
            let export = match exports.and_then(|e| e.get(op_id)) {
                Some(name) => text(name)?.to_string(),
                None => kebab(tail(op_id)),
            };
            writeln!(out, "  {export}: func({params}) -> {result};").unwrap();
        }
        writeln!(
            out,
            "}}\n\nworld implementation {{\n  export {interface_name};\n}}"
        )
        .unwrap();
        Ok(out)
    }
}
