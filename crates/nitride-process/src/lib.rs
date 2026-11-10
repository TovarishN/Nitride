//! Interface bindings implemented by another process. A `.nbind` names a
//! program for an interface; this starts it and speaks JSON-RPC 2.0 to it,
//! one JSON object per line on its stdin and stdout:
//!
//! ```text
//! → {"jsonrpc":"2.0","id":1,"method":"parse-date","params":["2024-01-01"]}
//! ← {"jsonrpc":"2.0","id":1,"result":738885}
//! ← {"jsonrpc":"2.0","id":2,"error":{"code":1,"message":"…","data":{"failure":"invalid-date"}}}
//! ```
//!
//! Methods are the binding's export names. Values are JSON by the catalog's
//! representations: numbers, integers, booleans, strings, records as objects
//! keyed by field name, lists as arrays. A declared failure is an error whose
//! `data.failure` is the failure's WIT name.
use nitride_core::host::binding_name as kebab;
use nitride_core::{Datum, Failure, HostBinding, Inputs, Language};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

/// A value of catalog type `ty` as protocol JSON.
fn to_json(catalog: &Value, ty: &str, datum: &Datum) -> Result<Value, String> {
    let representation = &catalog["representations"][ty];
    let wrong = || format!("value does not match catalog type {ty}");
    Ok(match datum {
        Datum::Number(n)
            if ty != "Core.Text"
                && (representation.is_null() || representation == "f64")
                && n.is_finite() =>
        {
            json!(n)
        }
        Datum::Integer(i) if representation == "i64" => json!(i),
        Datum::Boolean(b) if representation == "bool" => json!(b),
        Datum::Text(t) if ty == "Core.Text" || representation == "text" => json!(t),
        Datum::List(items) => {
            let element = representation["list"].as_str().ok_or_else(wrong)?;
            Value::Array(
                items
                    .iter()
                    .map(|item| to_json(catalog, element, item))
                    .collect::<Result<_, _>>()?,
            )
        }
        Datum::Record(values) => {
            let fields = representation["record"].as_array().ok_or_else(wrong)?;
            if fields.len() != values.len() {
                return Err(wrong());
            }
            Value::Object(
                fields
                    .iter()
                    .zip(values.iter())
                    .map(|(field, value)| {
                        let name = field.get(0).and_then(Value::as_str).ok_or_else(wrong)?;
                        let ty = field.get(1).and_then(Value::as_str).ok_or_else(wrong)?;
                        Ok((name.to_owned(), to_json(catalog, ty, value)?))
                    })
                    .collect::<Result<_, String>>()?,
            )
        }
        _ => return Err(wrong()),
    })
}

/// Protocol JSON as a value of catalog type `ty`.
fn from_json(catalog: &Value, ty: &str, value: &Value) -> Result<Datum, String> {
    let representation = &catalog["representations"][ty];
    let wrong = || format!("{value} is not a {ty}");
    Ok(match representation {
        Value::String(kind) if kind == "i64" => Datum::Integer(value.as_i64().ok_or_else(wrong)?),
        Value::String(kind) if kind == "bool" => Datum::Boolean(value.as_bool().ok_or_else(wrong)?),
        Value::String(kind) if kind == "text" => {
            Datum::Text(value.as_str().ok_or_else(wrong)?.into())
        }
        Value::Object(shape) if shape.contains_key("list") => {
            let element = shape["list"].as_str().unwrap_or_default();
            Datum::List(
                value
                    .as_array()
                    .ok_or_else(wrong)?
                    .iter()
                    .map(|item| from_json(catalog, element, item))
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            )
        }
        Value::Object(shape) if shape.contains_key("record") => {
            let object = value.as_object().ok_or_else(wrong)?;
            Datum::Record(
                shape["record"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|field| {
                        let name = field[0].as_str().unwrap_or_default();
                        let ty = field[1].as_str().unwrap_or_default();
                        let value = object
                            .get(name)
                            .ok_or_else(|| format!("{ty} has no field '{name}'"))?;
                        from_json(catalog, ty, value)
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            )
        }
        _ => Datum::Number(value.as_f64().ok_or_else(wrong)?),
    })
}

/// A running implementation: requests take turns on its pipes.
struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Server {
    fn call(&mut self, method: &str, params: Vec<Value>) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        let request = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        writeln!(self.stdin, "{request}")
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("cannot write to the process: {e}"))?;
        let mut line = String::new();
        if self
            .stdout
            .read_line(&mut line)
            .map_err(|e| format!("cannot read from the process: {e}"))?
            == 0
        {
            return Err("the process closed its output".into());
        }
        let response: Value =
            serde_json::from_str(&line).map_err(|e| format!("not a JSON-RPC response: {e}"))?;
        if !response.is_object()
            || response["jsonrpc"] != "2.0"
            || response.get("result").is_some() == response.get("error").is_some()
        {
            return Err("invalid JSON-RPC response envelope".into());
        }
        if let Some(error) = response.get("error")
            && (!error.is_object()
                || (error["code"].as_i64().is_none() && error["code"].as_u64().is_none())
                || error["message"].as_str().is_none())
        {
            return Err("invalid JSON-RPC error object".into());
        }
        if response["id"] != id {
            return Err(format!("response {} answers request {id}", response["id"]));
        }
        Ok(response)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Bindings for every operation `binding` (one entry of prepared data's
/// `bindings`, with target `process`) maps to its program, started in
/// `directory`. `prepared` is the project's prepared data.
pub fn bindings<'g>(
    language: &Language<'g>,
    prepared: &Value,
    binding: &Value,
    directory: &Path,
) -> Result<Vec<HostBinding<'g>>, String> {
    let program = binding["program"]
        .as_str()
        .ok_or("binding has no program")?;
    let args: Vec<&str> = binding["args"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    // A program without '/' is found on PATH; any other runs from `directory`.
    let command = if program.contains('/') {
        directory.join(program).into_os_string()
    } else {
        program.into()
    };
    let mut child = Command::new(&command)
        .args(&args)
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("cannot start {program}: {e}"))?;
    let stdin = child.stdin.take().ok_or("no process stdin")?;
    let stdout = BufReader::new(child.stdout.take().ok_or("no process stdout")?);
    let server = Arc::new(Mutex::new(Server {
        child,
        stdin,
        stdout,
        next: 0,
    }));
    let catalog = Arc::new(prepared["catalog"].clone());
    let contract = binding["contract"]
        .as_str()
        .ok_or("binding has no contract")?
        .to_owned();
    let mut bindings = Vec::new();
    for (op, method) in binding["exports"]
        .as_object()
        .ok_or("binding has no exports")?
    {
        let signature = *language
            .operations
            .iter()
            .find(|s| s.id == op)
            .ok_or_else(|| format!("the language has no operation {op}"))?;
        let operation = catalog["operations"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|o| o["id"] == op.as_str())
            .cloned()
            .unwrap_or_default();
        let method = method
            .as_str()
            .unwrap_or_default()
            .trim_start_matches('%')
            .to_owned();
        let (server, catalog) = (server.clone(), catalog.clone());
        // The closure outlives the language's borrow: own the types.
        let inputs: Vec<String> = signature.inputs.iter().map(|t| t.to_string()).collect();
        let result = signature.result.to_owned();
        let call = move |given: Inputs<'_>| -> Result<Datum, Failure> {
            let host = |message: String| Failure {
                id: "host.process".into(),
                message: message.into(),
            };
            let params = match given {
                Inputs::Text(text) if inputs == ["Core.Text"] => vec![json!(text)],
                Inputs::Values(values) if values.len() == inputs.len() => values
                    .iter()
                    .zip(&inputs)
                    .map(|(value, ty)| to_json(&catalog, ty, value))
                    .collect::<Result<_, _>>()
                    .map_err(host)?,
                _ => return Err(host("inputs do not match the signature".into())),
            };
            let response = server
                .lock()
                .map_err(|_| host("process poisoned".into()))?
                .call(&method, params)
                .map_err(host)?;
            if let Some(error) = response.get("error") {
                let declared = error["data"]["failure"].as_str().and_then(|name| {
                    operation["fails"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .find(|id| kebab(id.rsplit('.').next().unwrap_or(id)) == name)
                });
                return Err(match declared {
                    Some(id) => Failure {
                        id: id.to_owned().into(),
                        message: catalog["failures"][id]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned()
                            .into(),
                    },
                    None => host(error["message"].as_str().unwrap_or("error").to_owned()),
                });
            }
            from_json(&catalog, &result, &response["result"]).map_err(host)
        };
        bindings.push(HostBinding::contract(signature, contract.clone(), call));
    }
    Ok(bindings)
}
