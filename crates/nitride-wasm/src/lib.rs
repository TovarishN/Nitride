//! Interface bindings implemented by WebAssembly components. A project's
//! `.nbind` names a component for an interface; this loads it with wasmtime,
//! finds the export of every interface operation, checks its parameter and
//! result counts, and turns each into a contract binding that converts
//! values between `Datum` and the component model.
use nitride_core::{Datum, Failure, HostBinding, Inputs, Language};
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex};
use wasmtime::component::{Component, Func, Linker, Val};
use wasmtime::{Engine, Store};

/// How a catalog type's values cross into the component.
fn to_val(catalog: &Value, ty: &str, datum: &Datum) -> Result<Val, String> {
    let representation = &catalog["representations"][ty];
    Ok(match datum {
        Datum::Number(n) => Val::Float64(*n),
        Datum::Integer(i) => Val::S64(*i),
        Datum::Boolean(b) => Val::Bool(*b),
        Datum::Text(t) => Val::String(t.to_string()),
        Datum::List(items) => {
            let element = representation["list"].as_str().ok_or("not a list type")?;
            Val::List(
                items
                    .iter()
                    .map(|item| to_val(catalog, element, item))
                    .collect::<Result<_, _>>()?,
            )
        }
        Datum::Record(values) => {
            let fields = representation["record"]
                .as_array()
                .ok_or("not a record type")?;
            Val::Record(
                fields
                    .iter()
                    .zip(values.iter())
                    .map(|(field, value)| {
                        let name = field[0].as_str().unwrap();
                        let ty = field[1].as_str().unwrap();
                        Ok((wit(name), to_val(catalog, ty, value)?))
                    })
                    .collect::<Result<_, String>>()?,
            )
        }
    })
}

/// A component's value as a `Datum`.
fn from_val(val: Val) -> Result<Datum, String> {
    Ok(match val {
        Val::Float64(n) => Datum::Number(n),
        Val::S64(i) => Datum::Integer(i),
        Val::Bool(b) => Datum::Boolean(b),
        Val::String(t) => Datum::Text(t.into()),
        Val::List(items) => Datum::List(
            items
                .into_iter()
                .map(from_val)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        ),
        Val::Record(fields) => Datum::Record(
            fields
                .into_iter()
                .map(|(_, v)| from_val(v))
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        ),
        other => return Err(format!("unexpected component value {other:?}")),
    })
}

/// A catalog name as WIT writes it, without a keyword's `%`.
fn wit(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else if c == '_' {
            out.push('-');
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether a component type can carry values of catalog type `ty`.
fn fits(catalog: &Value, ty: &str, wit: &wasmtime::component::Type) -> bool {
    use wasmtime::component::Type;
    if ty == "Core.Text" {
        return matches!(wit, Type::String);
    }
    let representation = &catalog["representations"][ty];
    match (representation.as_str(), wit) {
        (Some("i64"), Type::S64) | (Some("bool"), Type::Bool) | (Some("text"), Type::String) => {
            true
        }
        (Some(_), _) => false,
        (None, Type::Float64) => representation.is_null(),
        (None, Type::List(list)) => representation["list"]
            .as_str()
            .is_some_and(|e| fits(catalog, e, &list.ty())),
        (None, Type::Record(record)) => {
            let fields = representation["record"].as_array();
            fields.is_some_and(|fields| {
                fields.len() == record.fields().len()
                    && fields.iter().zip(record.fields()).all(|(field, wit)| {
                        fits(catalog, field[1].as_str().unwrap_or_default(), &wit.ty)
                    })
            })
        }
        _ => false,
    }
}

/// Drafts a `.nbind` binding `interface` (from prepared data) to
/// `component`, written as `path` in the draft: each operation goes to the
/// export of its WIT name, else to the one export of the same shape; what
/// stays unmatched is left as a comment to resolve by hand.
pub fn draft(
    prepared: &Value,
    interface: &str,
    component: &Path,
    path: &str,
) -> Result<String, String> {
    use wasmtime::component::types::ComponentItem;
    let catalog = &prepared["catalog"];
    let declared = &catalog["interfaces"][interface];
    if declared.is_null() {
        return Err(format!("unknown interface '{interface}'"));
    }
    let engine = Engine::default();
    let loaded = Component::from_file(&engine, component)
        .map_err(|e| format!("{}: {e:#}", component.display()))?;
    let ty = loaded.component_type();
    let module = interface.rsplit_once('.').map_or("", |(m, _)| m);
    let version = &declared["version"];
    let nitride = format!(
        "nitride:{}/{}@{version}.0.0",
        wit(&module.replace('.', "")),
        wit(interface.rsplit('.').next().unwrap_or(interface))
    );
    let instances: Vec<_> = ty
        .exports(&engine)
        .filter_map(|(name, item)| match item.ty {
            ComponentItem::ComponentInstance(instance) => Some((name, instance)),
            _ => None,
        })
        .collect();
    let (instance_name, instance) = match instances.iter().find(|(name, _)| *name == nitride) {
        Some(found) => found,
        None => match instances.as_slice() {
            [only] => only,
            _ => {
                return Err(format!(
                    "{} exports instances {}; none is {nitride}",
                    component.display(),
                    instances
                        .iter()
                        .map(|(n, _)| *n)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        },
    };
    let functions: Vec<_> = instance
        .exports(&engine)
        .filter_map(|(name, item)| match item.ty {
            ComponentItem::ComponentFunc(func) => Some((name, func)),
            _ => None,
        })
        .collect();
    let failures = catalog["operations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["interface"] == interface)
        .any(|o| o["fails"].as_array().is_some_and(|f| !f.is_empty()));
    let mut taken: Vec<&str> = Vec::new();
    let mut lines = Vec::new();
    for op in declared["operations"].as_array().into_iter().flatten() {
        let id = op.as_str().unwrap_or_default();
        let operation = catalog["operations"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|o| o["id"] == id)
            .ok_or_else(|| format!("unknown operation {id}"))?;
        let inputs: Vec<&str> = operation["inputs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let result = operation["result"].as_str().unwrap_or_default();
        let shaped = |func: &wasmtime::component::types::ComponentFunc| {
            let returns = func.results().next();
            let returns = match (failures, returns) {
                (true, Some(wasmtime::component::Type::Result(r))) => r.ok(),
                (false, returns) => returns,
                _ => None,
            };
            func.params().len() == inputs.len()
                && func
                    .params()
                    .zip(&inputs)
                    .all(|((_, wit), ty)| fits(catalog, ty, &wit))
                && returns.is_some_and(|r| fits(catalog, result, &r))
        };
        let tail = id.rsplit('.').next().unwrap_or(id);
        let own = wit(tail);
        if functions
            .iter()
            .any(|(name, func)| *name == own && shaped(func))
        {
            taken.push(functions.iter().find(|(n, _)| *n == own).unwrap().0);
            continue;
        }
        let candidates: Vec<&str> = functions
            .iter()
            .filter(|(name, func)| !taken.contains(name) && shaped(func))
            .map(|(name, _)| *name)
            .collect();
        match candidates.as_slice() {
            [one] => {
                taken.push(one);
                lines.push(format!("  {tail} = {one};"));
            }
            [] => lines.push(format!("  // {tail}: no export of its shape")),
            several => lines.push(format!(
                "  // {tail}: one of {} (write `{tail} = <export>;`)",
                several.join(", ")
            )),
        }
    }
    let mut draft = format!(
        "// Drafted by `nitride binding draft` from {}; review before use.\nbinding {interface} version {version} for wasm component {path:?}",
        component.display()
    );
    if *instance_name != nitride {
        draft += &format!(" export {instance_name:?}");
    }
    if lines.is_empty() {
        draft += ";\n";
    } else {
        draft += &format!("\n{{\n{}\n}}\n", lines.join("\n"));
    }
    Ok(draft)
}

/// The store and the functions of one instantiated component; calls take
/// turns on the store.
struct Instance {
    store: Mutex<Store<()>>,
}

/// Bindings for every operation `binding` (one entry of prepared data's
/// `bindings`) maps to `component`. `prepared` is the project's prepared
/// data; its catalog gives types, failures and messages.
pub fn bindings<'g>(
    language: &Language<'g>,
    prepared: &Value,
    binding: &Value,
    component: &Path,
) -> Result<Vec<HostBinding<'g>>, String> {
    let catalog = Arc::new(prepared["catalog"].clone());
    let read = |e: wasmtime::Error| format!("{}: {e:#}", component.display());
    let engine = Engine::default();
    let loaded = Component::from_file(&engine, component).map_err(read)?;
    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &loaded)
        .map_err(read)?;
    let export = binding["export"].as_str().ok_or("binding has no export")?;
    let interface_index = instance
        .get_export_index(&mut store, None, export)
        .ok_or_else(|| format!("{} does not export {export}", component.display()))?;
    let contract: Arc<str> = binding["contract"]
        .as_str()
        .ok_or("binding has no contract")?
        .into();
    let interface = &catalog["interfaces"][binding["interface"].as_str().unwrap_or("")];
    // Failure enum cases, as WIT names them, to catalog ids.
    let failures: Arc<Vec<(String, String)>> = Arc::new(
        catalog["operations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|op| op["interface"] == binding["interface"])
            .flat_map(|op| op["fails"].as_array().cloned().unwrap_or_default())
            .filter_map(|f| f.as_str().map(str::to_owned))
            .map(|id| (wit(id.rsplit('.').next().unwrap()), id))
            .collect(),
    );
    let mut found: Vec<(&str, Func)> = Vec::new();
    for (op, name) in binding["exports"]
        .as_object()
        .ok_or("binding has no exports")?
    {
        let name = name.as_str().unwrap().trim_start_matches('%');
        let index = instance
            .get_export_index(&mut store, Some(&interface_index), name)
            .ok_or_else(|| {
                format!(
                    "{} does not export '{name}' of {export}",
                    component.display()
                )
            })?;
        let func = instance
            .get_func(&mut store, index)
            .ok_or_else(|| format!("'{name}' of {export} is not a function"))?;
        let signature = language
            .operations
            .iter()
            .find(|s| s.id == op)
            .ok_or_else(|| format!("the language has no operation {op}"))?;
        let ty = func.ty(&store);
        if ty.params().len() != signature.inputs.len() || ty.results().len() != 1 {
            return Err(format!(
                "'{name}' of {export} takes {} inputs and returns {} values; {op} takes {} and returns one",
                ty.params().len(),
                ty.results().len(),
                signature.inputs.len()
            ));
        }
        found.push((signature.id, func));
    }
    if interface.is_null() {
        return Err("binding names no catalog interface".into());
    }
    let shared = Arc::new(Instance {
        store: Mutex::new(store),
    });
    let mut bindings = Vec::new();
    for (id, func) in found {
        let signature = *language.operations.iter().find(|s| s.id == id).unwrap();
        let (shared, catalog, failures) = (shared.clone(), catalog.clone(), failures.clone());
        let inputs: Vec<String> = signature.inputs.iter().map(|t| t.to_string()).collect();
        let call = move |values: Inputs<'_>| -> Result<Datum, Failure> {
            let host = |message: String| Failure {
                id: "host.component".into(),
                message: message.into(),
            };
            let params = match values {
                Inputs::Text(text) => vec![Val::String(text.into())],
                Inputs::Values(values) => values
                    .iter()
                    .zip(&inputs)
                    .map(|(value, ty)| to_val(&catalog, ty, value))
                    .collect::<Result<_, _>>()
                    .map_err(host)?,
            };
            let mut results = [Val::Bool(false)];
            let mut store = shared
                .store
                .lock()
                .map_err(|_| host("component poisoned".into()))?;
            func.call(&mut *store, &params, &mut results)
                .map_err(|e| host(format!("{e:#}")))?;
            let [result] = results;
            let value = match result {
                Val::Result(Ok(value)) => *value.ok_or_else(|| host("empty result".into()))?,
                Val::Result(Err(Some(failure))) => {
                    let Val::Enum(case) = *failure else {
                        return Err(host("a failure is not an enum case".into()));
                    };
                    let id = failures
                        .iter()
                        .find(|(name, _)| *name == case)
                        .map(|(_, id)| id.clone())
                        .ok_or_else(|| host(format!("unknown failure '{case}'")))?;
                    let message = catalog["failures"][&id]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned();
                    return Err(Failure {
                        id: id.into(),
                        message: message.into(),
                    });
                }
                Val::Result(Err(None)) => return Err(host("failed without a reason".into())),
                plain => plain,
            };
            from_val(value).map_err(host)
        };
        bindings.push(HostBinding::contract(signature, contract.to_string(), call));
    }
    Ok(bindings)
}
