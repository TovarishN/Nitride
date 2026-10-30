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

#[cfg(feature = "components")]
fn component_bindings<'g>(
    language: &'g Language<'g>,
    loaded: &Loaded,
    binding: &Value,
    component: &Path,
) -> Result<Vec<HostBinding<'g>>, String> {
    nitride_wasm::bindings(language, &loaded.prepared, binding, component)
}

/// Without the `components` feature a component binding cannot be loaded.
#[cfg(not(feature = "components"))]
fn component_bindings<'g>(
    _: &'g Language<'g>,
    _: &Loaded,
    binding: &Value,
    component: &Path,
) -> Result<Vec<HostBinding<'g>>, String> {
    Err(format!(
        "{} binds {} to a WebAssembly component; this nitride was built without component support (cargo feature `components`)",
        component.display(),
        binding["interface"].as_str().unwrap_or_default()
    ))
}

/// Per binding, its implementation's bindings: `(interface, what implements
/// it, bindings)`, where the implementation is a component path or a program.
fn bind<'g>(
    loaded: &Loaded,
    language: &'g Language<'g>,
    manifest: &Path,
    granted: &[String],
) -> Result<Vec<(String, String, Vec<HostBinding<'g>>)>, String> {
    let folder = manifest.parent().unwrap_or(Path::new(""));
    let operations = &loaded.prepared["catalog"]["operations"];
    let needs = |op: &str| -> Vec<String> {
        operations
            .as_array()
            .into_iter()
            .flatten()
            .filter(|o| o["id"] == op)
            .flat_map(|o| o["effects"].as_array().cloned().unwrap_or_default())
            .filter_map(|e| e.as_str().map(str::to_owned))
            .collect()
    };
    let mut bound = Vec::new();
    for binding in loaded.prepared["bindings"].as_array().into_iter().flatten() {
        let (implementation, bindings) = match binding["target"].as_str() {
            Some("process") => {
                let directory = folder.join(binding["directory"].as_str().unwrap_or_default());
                let program = binding["program"].as_str().unwrap_or_default();
                let bindings =
                    nitride_process::bindings(language, &loaded.prepared, binding, &directory)?;
                (program, bindings)
            }
            _ => {
                let component = binding["component"].as_str().unwrap_or_default();
                (
                    component,
                    component_bindings(language, loaded, binding, &folder.join(component))?,
                )
            }
        };
        bound.push((
            binding["interface"].as_str().unwrap_or_default().to_owned(),
            implementation.to_owned(),
            host::grant(bindings, &needs, granted),
        ));
    }
    Ok(bound)
}

/// Runs `source` in the project's language: the values of its show
/// statements, or the first diagnostic. Components' operations that need
/// effects outside `granted` are denied.
pub fn run_project(manifest: &Path, source: &str, granted: &[String]) -> Result<Value, String> {
    let loaded = load(manifest)?;
    let language = loaded.language.language();
    let mut bindings = host::bindings(language);
    for (_, _, component) in bind(&loaded, language, manifest, granted)? {
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
/// engine's handlers for the language's other operations. Examples that
/// need effects outside `granted` are skipped.
pub fn conform_project(manifest: &Path, granted: &[String]) -> Result<Value, String> {
    let loaded = load(manifest)?;
    let language = loaded.language.language();
    let mut reports = Vec::new();
    for (interface, component, bindings) in bind(&loaded, language, manifest, granted)? {
        let mut all = host::bindings(language);
        all.extend(bindings);
        let examples = examples(&loaded.prepared["catalog"]["interfaces"][&interface]);
        let outcome = host::conformance(language, &all, examples);
        reports.push(json!({
            "interface": interface,
            "implementation": component,
            "examples": examples.len(),
            "skipped": outcome.skipped,
            "failed": outcome.failed
                .iter()
                .map(|(example, why)| json!({"example": example, "why": why}))
                .collect::<Vec<_>>(),
        }));
    }
    Ok(Value::Array(reports))
}
