//! Interface bindings implemented by WebAssembly components. A project's
//! `.nbind` names a component for an interface; this loads it with wasmtime,
//! finds the export of every interface operation, checks its parameter and
//! result counts, and turns each into a contract binding that converts
//! values between `Datum` and the component model.
use nitride_core::host::binding_name as wit;
use nitride_core::{Datum, Failure, HostBinding, Inputs, Language};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;
use wasmtime::component::{Component, Func, Linker, Val};
use wasmtime::{Engine, Store};

/// The engine components compile for: one that meters fuel, so a run can be
/// bounded, and one that does not, which runs faster.
fn engine(metered: bool) -> &'static Engine {
    static ENGINES: [OnceLock<Engine>; 2] = [OnceLock::new(), OnceLock::new()];
    ENGINES[usize::from(metered)].get_or_init(|| {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(metered);
        Engine::new(&config).expect("a wasmtime engine")
    })
}

/// The file's version: its modification time and length.
type Version = (Option<SystemTime>, u64);

/// Compiled components by path and whether their engine meters fuel.
type Cache = Mutex<HashMap<(PathBuf, bool), (Version, Component)>>;

/// `component` compiled for `engine`, once per version of the file: an editor
/// loads the same component on every run.
fn compiled(
    engine: &'static Engine,
    metered: bool,
    component: &Path,
) -> wasmtime::Result<Component> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let metadata = std::fs::metadata(component)?;
    let version = (metadata.modified().ok(), metadata.len());
    let key = (component.to_owned(), metered);
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some((cached, loaded)) = cache.lock().unwrap().get(&key)
        && *cached == version
    {
        return Ok(loaded.clone());
    }
    let loaded = Component::from_file(engine, component)?;
    cache.lock().unwrap().insert(key, (version, loaded.clone()));
    Ok(loaded)
}

/// How a catalog type's values cross into the component.
fn to_val(catalog: &Value, ty: &str, datum: &Datum) -> Result<Val, String> {
    let representation = &catalog["representations"][ty];
    let wrong = || format!("value does not match catalog type {ty}");
    Ok(match datum {
        Datum::Number(n)
            if ty != "Core.Text"
                && (representation.is_null() || representation == "f64")
                && n.is_finite() =>
        {
            Val::Float64(*n)
        }
        Datum::Integer(i) if representation == "i64" => Val::S64(*i),
        Datum::Boolean(b) if representation == "bool" => Val::Bool(*b),
        Datum::Text(t) if ty == "Core.Text" || representation == "text" => {
            Val::String(t.to_string())
        }
        Datum::List(items) => {
            let element = representation["list"].as_str().ok_or_else(wrong)?;
            Val::List(
                items
                    .iter()
                    .map(|item| to_val(catalog, element, item))
                    .collect::<Result<_, _>>()?,
            )
        }
        Datum::Record(values) => {
            let fields = representation["record"].as_array().ok_or_else(wrong)?;
            if fields.len() != values.len() {
                return Err(wrong());
            }
            Val::Record(
                fields
                    .iter()
                    .zip(values.iter())
                    .map(|(field, value)| {
                        let name = field.get(0).and_then(Value::as_str).ok_or_else(wrong)?;
                        let ty = field.get(1).and_then(Value::as_str).ok_or_else(wrong)?;
                        Ok((wit(name), to_val(catalog, ty, value)?))
                    })
                    .collect::<Result<_, String>>()?,
            )
        }
        _ => return Err(wrong()),
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
                        field
                            .get(0)
                            .and_then(Value::as_str)
                            .is_some_and(|name| self::wit(name) == wit.name)
                            && field
                                .get(1)
                                .and_then(Value::as_str)
                                .is_some_and(|ty| fits(catalog, ty, &wit.ty))
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
    bounded_bindings(language, prepared, binding, component, None)
}

/// [`bindings`] whose calls together may spend at most `fuel` (about one unit
/// per WebAssembly instruction); a call past it fails, so a component that
/// loops cannot hang its caller.
pub fn bounded_bindings<'g>(
    language: &Language<'g>,
    prepared: &Value,
    binding: &Value,
    component: &Path,
    fuel: Option<u64>,
) -> Result<Vec<HostBinding<'g>>, String> {
    let catalog = Arc::new(prepared["catalog"].clone());
    let read = |e: wasmtime::Error| format!("{}: {e:#}", component.display());
    let engine = engine(fuel.is_some());
    let loaded = compiled(engine, fuel.is_some(), component).map_err(read)?;
    let mut store = Store::new(engine, ());
    if let Some(fuel) = fuel {
        store.set_fuel(fuel).map_err(read)?;
    }
    let instance = Linker::new(engine)
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
        let name = name
            .as_str()
            .ok_or_else(|| format!("export name for {op} is not a string"))?
            .trim_start_matches('%');
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
        let result = ty.results().next();
        let result = match (failures.is_empty(), result) {
            (false, Some(wasmtime::component::Type::Result(result))) => result.ok(),
            (true, result) => result,
            _ => None,
        };
        if !ty
            .params()
            .zip(signature.inputs)
            .all(|((_, component), ty)| fits(&catalog, ty, &component))
            || !result.is_some_and(|result| fits(&catalog, signature.result, &result))
        {
            return Err(format!(
                "'{name}' of {export} has incompatible parameter or result types for {op}"
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
                Inputs::Text(text) if inputs == ["Core.Text"] => vec![Val::String(text.into())],
                Inputs::Values(values) if values.len() == inputs.len() => values
                    .iter()
                    .zip(&inputs)
                    .map(|(value, ty)| to_val(&catalog, ty, value))
                    .collect::<Result<_, _>>()
                    .map_err(host)?,
                _ => return Err(host("inputs do not match the signature".into())),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn catalog() -> Value {
        json!({"representations":{"Span":{"record":[["start_day","Day"],["dates","Dates"]]},"Day":"i64","Dates":{"list":"Day"},"Flag":"bool","Name":"text"}})
    }

    #[test]
    fn component_values_preserve_nested_shapes_and_wit_field_names() {
        let value = Datum::Record(
            vec![
                Datum::Integer(7),
                Datum::List(vec![Datum::Integer(8), Datum::Integer(9)].into()),
            ]
            .into(),
        );
        let converted = to_val(&catalog(), "Span", &value).unwrap();
        let Val::Record(fields) = converted else {
            panic!("not a record")
        };
        assert_eq!(fields[0].0, "start-day");
        assert!(matches!(fields[0].1, Val::S64(7)));
        assert_eq!(fields[1].0, "dates");
        let Val::List(items) = &fields[1].1 else {
            panic!("not a list")
        };
        assert!(matches!(items.as_slice(), [Val::S64(8), Val::S64(9)]));
        assert_eq!(
            from_val(Val::Record(vec![
                ("start-day".into(), Val::S64(7)),
                ("dates".into(), Val::List(vec![Val::S64(8), Val::S64(9)]))
            ]))
            .unwrap(),
            value
        );
        assert!(matches!(
            to_val(&catalog(), "Flag", &Datum::Boolean(true)).unwrap(),
            Val::Bool(true)
        ));
        assert!(
            matches!(to_val(&catalog(),"Name",&Datum::Text("é😀".into())).unwrap(),Val::String(text) if text=="é😀")
        );
        assert!(from_val(Val::U32(7)).is_err());
    }

    #[test]
    fn missing_record_fields_return_errors() {
        assert!(to_val(&catalog(), "Span", &Datum::Record(Vec::new().into())).is_err());
    }

    #[test]
    fn invalid_record_field_metadata_returns_an_error() {
        let malformed = json!({"representations":{"Bad":{"record":[[false,"Day"]]},"Day":"i64"}});
        assert!(
            to_val(
                &malformed,
                "Bad",
                &Datum::Record(vec![Datum::Integer(7)].into())
            )
            .is_err()
        );
    }
    #[test]
    fn scalar_representations_reject_wrong_kinds_and_nonfinite_numbers() {
        let catalog = catalog();
        assert!(matches!(
            to_val(&catalog, "Core.Scalar", &Datum::Number(2.5)).unwrap(),
            Val::Float64(2.5)
        ));
        for (ty, datum) in [
            ("Day", Datum::Number(7.0)),
            ("Core.Scalar", Datum::Boolean(true)),
            ("Core.Scalar", Datum::Number(f64::NAN)),
            ("Core.Scalar", Datum::Number(f64::INFINITY)),
            (
                "Span",
                Datum::Record(vec![Datum::Integer(7), Datum::Integer(8), Datum::Integer(9)].into()),
            ),
        ] {
            assert!(to_val(&catalog, ty, &datum).is_err(), "{ty}");
        }
        assert_eq!(from_val(Val::Float64(2.5)).unwrap(), Datum::Number(2.5));
        assert_eq!(from_val(Val::Bool(true)).unwrap(), Datum::Boolean(true));
        assert_eq!(
            from_val(Val::String("é😀".into())).unwrap(),
            Datum::Text("é😀".into())
        );
    }
    #[test]
    fn record_field_names_match_generated_wit_numeric_suffixes() {
        let catalog = json!({"representations":{"Numbered":{"record":[["date_2","Day"],["start__day","Day"],["Type","Day"]]},"Day":"i64"}});
        let value =
            Datum::Record(vec![Datum::Integer(1), Datum::Integer(2), Datum::Integer(3)].into());
        let Val::Record(fields) = to_val(&catalog, "Numbered", &value).unwrap() else {
            panic!("not a record")
        };
        assert_eq!(
            fields
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["date2", "start-day", "type"]
        );
    }
}
