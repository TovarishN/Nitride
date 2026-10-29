//! Any project, prepared at run time: its program runs with the engine's
//! handlers and the WebAssembly components its bindings name, and its
//! interface examples run against those components.
use nitride_core::host::{Example, ExampleExpr, Expected};
use nitride_core::*;
use serde_json::{Value, json};
use std::path::Path;

/// A prepared project, its language and the bindings of its components.
struct Loaded {
    prepared: Value,
    language: nitride_codegen::LoadedLanguage,
}

fn load(manifest: &Path) -> Result<Loaded, String> {
    let prepared = nitride_prepare::prepare_project(manifest)
        .map_err(|e| e.to_string())?
        .data;
    let language = nitride_codegen::LoadedLanguage::load(&prepared.to_string())?;
    Ok(Loaded { prepared, language })
}

/// The engine's handlers and, per binding, its component's: `(interface,
/// component path, bindings)`.
fn bind<'g>(
    loaded: &Loaded,
    language: &'g Language<'g>,
    manifest: &Path,
) -> Result<Vec<(String, String, Vec<HostBinding<'g>>)>, String> {
    let folder = manifest.parent().unwrap_or(Path::new(""));
    let mut bound = Vec::new();
    for binding in loaded.prepared["bindings"].as_array().into_iter().flatten() {
        let component = binding["component"].as_str().unwrap_or_default();
        let bindings =
            nitride_wasm::bindings(language, &loaded.prepared, binding, &folder.join(component))?;
        bound.push((
            binding["interface"].as_str().unwrap_or_default().to_owned(),
            component.to_owned(),
            bindings,
        ));
    }
    Ok(bound)
}

/// Runs `source` in the project's language: the values of its show
/// statements, or the first diagnostic.
pub fn run_project(manifest: &Path, source: &str) -> Result<Value, String> {
    let loaded = load(manifest)?;
    let language = loaded.language.language();
    let mut bindings = host::bindings(language);
    for (_, _, component) in bind(&loaded, language, manifest)? {
        bindings.extend(component);
    }
    let error = |e: Diagnostic| {
        let mut observation =
            json!({"ok":false,"error":e.category,"span":e.span,"message":e.message});
        if let Some(code) = e.code {
            observation["code"] = json!(code);
        }
        observation
    };
    let tree = match parse(language, source, Limits::default()) {
        Ok(tree) => tree,
        Err(e) => return Ok(error(e)),
    };
    let program = match compile(language, &tree, Limits::default()) {
        Ok(program) => program,
        Err(e) => return Ok(error(e.into_iter().next().unwrap())),
    };
    let values = host::host_values(language, host::utc_today());
    Ok(
        match execute_with_values(&program, &bindings, &values, Limits::default()) {
            Ok(report) => json!({
                "ok": true,
                "values": report.values.iter().map(crate::datum).collect::<Vec<_>>(),
                "trace": report.trace,
            }),
            Err(failure) => {
                let mut observation = error(failure.diagnostic);
                observation["trace"] = json!(failure.trace);
                observation
            }
        },
    )
}

/// A catalog example expression, leaked: examples live for the run.
fn expr(value: &Value) -> ExampleExpr<'static> {
    if let Some(b) = value["boolean"].as_bool() {
        ExampleExpr::Boolean(b)
    } else if let Some(n) = value["number"].as_f64() {
        ExampleExpr::Number(n)
    } else if let Some(t) = value["text"].as_str() {
        ExampleExpr::Text(Box::leak(t.into()))
    } else {
        ExampleExpr::Call {
            operation: Box::leak(value["call"].as_str().unwrap_or_default().into()),
            args: Box::leak(
                value["args"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(expr)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
        }
    }
}

fn examples(interface: &Value) -> &'static [Example<'static>] {
    let examples: Vec<_> = interface["examples"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| Example {
            source: Box::leak(e["source"].as_str().unwrap_or_default().into()),
            value: expr(&e["value"]),
            expected: match e["fails"].as_str() {
                Some(id) => Expected::Fails(Box::leak(id.into())),
                None => Expected::Equals(expr(&e["equals"])),
            },
        })
        .collect();
    Box::leak(examples.into_boxed_slice())
}

/// Runs each bound interface's examples against its component, with the
/// engine's handlers for the language's other operations.
pub fn conform_project(manifest: &Path) -> Result<Value, String> {
    let loaded = load(manifest)?;
    let language = loaded.language.language();
    let mut reports = Vec::new();
    for (interface, component, bindings) in bind(&loaded, language, manifest)? {
        let mut all = host::bindings(language);
        all.extend(bindings);
        let examples = examples(&loaded.prepared["catalog"]["interfaces"][&interface]);
        let failed = host::conform(language, &all, examples);
        reports.push(json!({
            "interface": interface,
            "component": component,
            "examples": examples.len(),
            "failed": failed
                .iter()
                .map(|(example, why)| json!({"example": example, "why": why}))
                .collect::<Vec<_>>(),
        }));
    }
    Ok(Value::Array(reports))
}
