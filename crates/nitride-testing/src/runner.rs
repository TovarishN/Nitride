use crate::{
    Case, Input, Phase, Snapshot, Suite, SuiteError, identity, parse_suite, validate_spans,
    values_equal,
};
use nitride_core::*;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
pub struct Discovery {
    pub path: PathBuf,
    pub project: PathBuf,
    pub suite: Suite,
    pub dependencies: Vec<PathBuf>,
    pub project_error: Option<String>,
}
fn problem(message: impl Into<String>) -> SuiteError {
    SuiteError {
        span: [0, 0],
        message: message.into(),
    }
}
fn project_paths(project: &Path, snapshot: &Snapshot) -> Result<Vec<PathBuf>, String> {
    let text = snapshot.read(project).map_err(|e| e.to_string())?;
    let base = project.parent().ok_or("project needs folder")?;
    if project.extension().is_some_and(|s| s == "nproj") {
        let language = nitride_prepare::read_language(
            &nitride_prepare::Source {
                path: project.display().to_string(),
                text,
            },
            &|_| true,
        )
        .map_err(|e| e[0].to_string())?;
        Ok(language
            .grammars
            .into_iter()
            .chain(language.catalogs)
            .chain(language.bindings)
            .collect())
    } else {
        let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let mut paths = Vec::new();
        for key in ["grammars", "catalogs", "bindings"] {
            if let Some(items) = value.get(key) {
                for item in items
                    .as_array()
                    .ok_or_else(|| format!("{key} must be array"))?
                {
                    paths.push(base.join(item.as_str().ok_or("input path must be string")?));
                }
            }
        }
        Ok(paths)
    }
}
pub fn discover(path: &Path, snapshot: &Snapshot) -> Result<Discovery, SuiteError> {
    let path = identity(path).map_err(|e| problem(e.to_string()))?;
    let text = snapshot.read(&path).map_err(|e| problem(e.to_string()))?;
    let suite = parse_suite(&text)?;
    let base = path.parent().unwrap();
    let project = identity(&base.join(&suite.project)).map_err(|e| problem(e.to_string()))?;
    let mut dependencies = vec![path.clone(), project.clone()];
    let project_error = match project_paths(&project, snapshot) {
        Ok(paths) => {
            for path in paths {
                dependencies.push(identity(&path).map_err(|e| problem(e.to_string()))?);
            }
            None
        }
        Err(e) => Some(e),
    };
    for case in &suite.tests {
        if let Input::File(file) = &case.input {
            if file.is_empty() {
                return Err(problem("fixture path must not be empty"));
            }
            dependencies.push(identity(&base.join(file)).map_err(|e| problem(e.to_string()))?);
        }
    }
    dependencies.sort();
    dependencies.dedup();
    Ok(Discovery {
        path,
        project,
        suite,
        dependencies,
        project_error,
    })
}
impl Discovery {
    pub fn json(&self) -> Value {
        json!({"path":self.path,"project":self.project,"dependencies":self.dependencies,"project_error":self.project_error,"tests":self.suite.tests.iter().map(Case::metadata).collect::<Vec<_>>()})
    }
}
#[derive(Default)]
pub struct Report {
    pub results: Vec<Value>,
    pub errors: Vec<Value>,
}
impl Report {
    pub fn exit_code(&self) -> i32 {
        if !self.errors.is_empty() || self.results.iter().any(|r| r["status"] == "errored") {
            2
        } else if self.results.iter().any(|r| r["status"] == "failed") {
            1
        } else {
            0
        }
    }
    pub fn json(&self) -> Value {
        let count = |s| self.results.iter().filter(|r| r["status"] == s).count();
        json!({"version":1,"results":self.results,"errors":self.errors,"counts":{"passed":count("passed"),"failed":count("failed"),"errored":count("errored")+self.errors.len()},"exit_code":self.exit_code()})
    }
    pub fn append(&mut self, other: Self) {
        self.results.extend(other.results);
        self.errors.extend(other.errors);
    }
}
struct Loaded {
    data: Value,
    language: nitride_codegen::LoadedLanguage,
}
pub struct Runner<'a> {
    snapshot: &'a Snapshot,
    loaded: HashMap<PathBuf, Result<Loaded, String>>,
}
impl<'a> Runner<'a> {
    pub fn new(snapshot: &'a Snapshot) -> Self {
        Self {
            snapshot,
            loaded: HashMap::new(),
        }
    }
    pub fn run(
        &mut self,
        path: &Path,
        selectors: &[String],
        event: &mut dyn FnMut(Value),
    ) -> Report {
        let mut report = Report::default();
        let discovery = match discover(path, self.snapshot) {
            Ok(d) => d,
            Err(e) => {
                report
                    .errors
                    .push(json!({"path":path,"span":e.span,"message":e.message}));
                return report;
            }
        };
        if let Some(name) = selectors
            .iter()
            .find(|name| !discovery.suite.tests.iter().any(|c| c.name == **name))
        {
            report
                .errors
                .push(json!({"path":path,"message":format!("unknown case {name}")}));
            return report;
        }
        let loaded = self
            .loaded
            .entry(discovery.project.clone())
            .or_insert_with(|| {
                let data = nitride_prepare::prepare_project_with(&discovery.project, self.snapshot)
                    .map_err(|e| e.to_string())?
                    .data;
                let language = nitride_codegen::LoadedLanguage::load(&data.to_string())?;
                Ok(Loaded { data, language })
            });
        for case in &discovery.suite.tests {
            if !selectors.is_empty() && !selectors.contains(&case.name) {
                continue;
            }
            let observed = (|| -> Result<Value, String> {
                let loaded = loaded.as_ref().map_err(Clone::clone)?;
                let text = match &case.input {
                    Input::Inline { text, .. } => text.clone(),
                    Input::File(file) => self
                        .snapshot
                        .read(&discovery.path.parent().unwrap().join(file))
                        .map_err(|e| e.to_string())?,
                };
                validate_spans(&text, &case.expect)?;
                observe(loaded, &text, case.expect.phase)
            })();
            let result = match observed {
                Err(message) => {
                    json!({"suite":discovery.path,"name":case.name,"status":"errored","message":message,"requested_phase":case.expect.phase.name()})
                }
                Ok(actual) => {
                    let matches = actual["phase"] == case.expect.phase.name()
                        && actual["accepted"] == case.expect.accepted
                        && case
                            .expect
                            .values
                            .as_ref()
                            .is_none_or(|v| values_equal(v, &actual["values"]))
                        && case.expect.diagnostics.as_ref().is_none_or(|expected| {
                            let actual = actual["diagnostics"]
                                .as_array()
                                .map(Vec::as_slice)
                                .unwrap_or_default();
                            expected.len() == actual.len()
                                && expected.iter().zip(actual).all(|(e, a)| {
                                    a["category"] == e.category
                                        && e.code
                                            .as_ref()
                                            .is_none_or(|code| a["code"] == json!(code))
                                        && e.span.is_none_or(|span| a["span"] == json!(span))
                                })
                        });
                    json!({"suite":discovery.path,"name":case.name,"status":if matches{"passed"}else{"failed"},"requested_phase":case.expect.phase.name(),"actual":actual,"expected":{"phase":case.expect.phase.name(),"accepted":case.expect.accepted,"values":case.expect.values,"diagnostics":case.expect.diagnostics.as_ref().map(|ds|ds.iter().map(|d|json!({"category":d.category,"code":d.code,"span":d.span})).collect::<Vec<_>>())},"message":if matches{"matched expectation"}else{"phase, diagnostics or values differ from expectation"}})
                }
            };
            let mut result = result;
            result["source"] = match &case.input {
                Input::Inline { raw_span, .. } => {
                    json!({"suite":discovery.path,"raw_span":raw_span})
                }
                Input::File(file) => {
                    json!({"file":identity(&discovery.path.parent().unwrap().join(file)).map_err(|e|e.to_string()).ok()})
                }
            };
            if report
                .results
                .iter()
                .map(|r| r.to_string().len())
                .sum::<usize>()
                + result.to_string().len()
                > 32 * 1024 * 1024
            {
                report
                    .errors
                    .push(json!({"path":discovery.path,"message":"test report exceeds32 MiB"}));
                break;
            }
            event(json!({"version":1,"event":"caseFinished","result":result}));
            report.results.push(result);
        }
        report
    }
}
fn diagnostics(phase: Phase, errors: Vec<Diagnostic>) -> Result<Value, String> {
    if errors
        .iter()
        .any(|e| matches!(e.category, "limit" | "cancelled"))
    {
        return Err(errors[0].message.clone());
    }
    Ok(
        json!({"phase":phase.name(),"accepted":false,"diagnostics":errors.iter().map(|e|json!({"category":e.category,"code":e.code,"span":e.span,"message":e.message})).collect::<Vec<_>>()}),
    )
}
fn datum(value: &Datum) -> Result<Value, String> {
    Ok(match value {
        Datum::Number(n) => {
            if !n.is_finite() {
                return Err("nonfinite test result".into());
            }
            json!(n)
        }
        Datum::Integer(i) => json!(i),
        Datum::Boolean(b) => json!(b),
        Datum::Text(t) => json!(t),
        Datum::Record(items) | Datum::List(items) => {
            Value::Array(items.iter().map(datum).collect::<Result<_, _>>()?)
        }
    })
}
fn observe(loaded: &Loaded, text: &str, phase: Phase) -> Result<Value, String> {
    let language = loaded.language.language();
    let tree = match parse(language, text, Limits::default()) {
        Ok(tree) => tree,
        Err(e) => return diagnostics(Phase::Parse, vec![e]),
    };
    if phase == Phase::Parse {
        return Ok(json!({"phase":"parse","accepted":true,"diagnostics":[]}));
    }
    let program = match compile(language, &tree, Limits::default()) {
        Ok(program) => program,
        Err(errors) => return diagnostics(Phase::Compile, errors),
    };
    if phase == Phase::Compile {
        return Ok(json!({"phase":"compile","accepted":true,"diagnostics":[]}));
    }
    let bindings = host::grant(
        host::bindings(language),
        &|id| {
            loaded.data["catalog"]["operations"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|o| o["id"] == id)
                .flat_map(|o| o["effects"].as_array().into_iter().flatten())
                .filter_map(|e| e.as_str().map(str::to_owned))
                .collect()
        },
        &[],
    );
    let values = host::host_values(language, iso_date_day_number("2000-01-01").unwrap() as f64);
    preflight_with_values(&program, &bindings, &values).map_err(|e| {
        format!(
            "unsupported test execution: {}",
            e.iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    match execute_with_values(&program, &bindings, &values, Limits::default()) {
        Ok(report) => Ok(
            json!({"phase":"run","accepted":true,"diagnostics":[],"values":report.values.iter().map(datum).collect::<Result<Vec<_>,_>>()?}),
        ),
        Err(failure) => diagnostics(Phase::Run, vec![failure.diagnostic]),
    }
}
