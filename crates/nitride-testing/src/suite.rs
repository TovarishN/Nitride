use nitride_core::{Limits, SyntaxTree};
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Parse,
    Compile,
    Run,
}
impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::Compile => "compile",
            Self::Run => "run",
        }
    }
}
#[derive(Clone, Debug)]
pub enum Input {
    Inline {
        text: String,
        raw_span: Option<[usize; 2]>,
    },
    File(String),
}
#[derive(Clone, Debug)]
pub struct ExpectedDiagnostic {
    pub category: String,
    pub code: Option<Option<String>>,
    pub span: Option<[u32; 2]>,
}
#[derive(Clone, Debug)]
pub struct Expectation {
    pub phase: Phase,
    pub accepted: bool,
    pub values: Option<Value>,
    pub diagnostics: Option<Vec<ExpectedDiagnostic>>,
}
#[derive(Clone, Debug)]
pub struct Case {
    pub name: String,
    pub span: [usize; 2],
    pub name_span: [usize; 2],
    pub input: Input,
    pub expect: Expectation,
}
#[derive(Clone, Debug)]
pub struct Suite {
    pub project: String,
    pub tests: Vec<Case>,
}
#[derive(Clone, Debug)]
pub struct SuiteError {
    pub span: [usize; 2],
    pub message: String,
}
impl std::fmt::Display for SuiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [{}..{}]", self.message, self.span[0], self.span[1])
    }
}
impl std::error::Error for SuiteError {}
struct Decoder<'a> {
    tree: &'a SyntaxTree<'static>,
}
impl Decoder<'_> {
    fn kind(&self, id: u32) -> &str {
        super::frontend::LANGUAGE.rules[self.tree.nodes[id as usize].rule as usize]
            .name
            .rsplit('.')
            .next()
            .unwrap()
    }
    fn span(&self, id: u32) -> [usize; 2] {
        self.tree.nodes[id as usize].span.map(|b| b as usize)
    }
    fn text(&self, id: u32) -> &str {
        self.tree.text(id).unwrap()
    }
    fn field(&self, id: u32, name: &str) -> u32 {
        self.tree.field(id, name).expect("generated test field")
    }
    fn fields(&self, id: u32, name: &str) -> Vec<u32> {
        self.tree
            .children(id)
            .iter()
            .filter(|e| self.tree.label(e) == name)
            .map(|e| e.node)
            .collect()
    }
    fn concrete(&self, mut id: u32, kinds: &[&str]) -> u32 {
        while !kinds.contains(&self.kind(id)) {
            id = self.tree.children(id)[0].node;
        }
        id
    }
    fn error(&self, id: u32, message: impl Into<String>) -> SuiteError {
        SuiteError {
            span: self.span(id),
            message: message.into(),
        }
    }
    fn string(&self, id: u32) -> Result<String, SuiteError> {
        serde_json::from_str(self.text(id))
            .map_err(|e| self.error(id, format!("invalid string: {e}")))
    }
    fn value(&self, id: u32, depth: usize) -> Result<Value, SuiteError> {
        if depth > 128 {
            return Err(self.error(id, "value nesting limit"));
        }
        let id = self.concrete(id, &["NumberValue", "StringValue", "BooleanValue", "Array"]);
        match self.kind(id) {
            "Array" => Ok(Value::Array(
                self.fields(id, "Items")
                    .into_iter()
                    .map(|v| self.value(v, depth + 1))
                    .collect::<Result<_, _>>()?,
            )),
            "StringValue" => Ok(self.string(self.field(id, "Value"))?.into()),
            "BooleanValue" => Ok(Value::Bool(self.text(id).trim() == "true")),
            _ => serde_json::from_str(self.text(self.field(id, "Value")))
                .map_err(|e| self.error(id, format!("invalid finite value: {e}"))),
        }
    }
    fn diagnostic(&self, id: u32) -> Result<ExpectedDiagnostic, SuiteError> {
        let mut result = ExpectedDiagnostic {
            category: self.string(self.field(id, "Category"))?,
            code: None,
            span: None,
        };
        for clause in self.fields(id, "Clauses") {
            let c = self.concrete(clause, &["Code", "Span"]);
            match self.kind(c) {
                "Code" => {
                    if result.code.is_some() {
                        return Err(self.error(c, "repeated diagnostic code"));
                    }
                    result.code = Some(
                        self.tree
                            .field(c, "Value")
                            .map(|value| self.string(value))
                            .transpose()?,
                    );
                }
                _ => {
                    if result.span.is_some() {
                        return Err(self.error(c, "repeated diagnostic span"));
                    }
                    let integer = |name| {
                        self.text(self.field(c, name))
                            .parse::<u32>()
                            .map_err(|_| self.error(c, "diagnostic span exceeds u32"))
                    };
                    let span = [integer("Start")?, integer("End")?];
                    if span[0] > span[1] {
                        return Err(self.error(c, "diagnostic span is reversed"));
                    }
                    result.span = Some(span);
                }
            }
        }
        Ok(result)
    }
    fn expectation(&self, id: u32) -> Result<Expectation, SuiteError> {
        let phase = self.field(id, "Phase");
        let phase = match self.text(phase) {
            "parse" => Phase::Parse,
            "compile" => Phase::Compile,
            "run" => Phase::Run,
            _ => return Err(self.error(phase, "expected parse, compile or run phase")),
        };
        let body = self.concrete(self.field(id, "Body"), &["Accept", "RunValues", "Reject"]);
        let accepted = self.kind(body) != "Reject";
        let values = if self.kind(body) == "RunValues" {
            Some(self.value(self.field(body, "Value"), 0)?)
        } else {
            None
        };
        if accepted && (phase == Phase::Run) != values.is_some() {
            return Err(self.error(
                body,
                "successful run requires values; parse/compile require accept",
            ));
        }
        let diagnostics = self.fields(body, "Diagnostics");
        let diagnostics = if diagnostics.is_empty() {
            None
        } else {
            Some(
                diagnostics
                    .into_iter()
                    .map(|d| self.diagnostic(d))
                    .collect::<Result<_, _>>()?,
            )
        };
        Ok(Expectation {
            phase,
            accepted,
            values,
            diagnostics,
        })
    }
    fn case(&self, id: u32) -> Result<Case, SuiteError> {
        let name_id = self.field(id, "Name");
        let name = self.string(name_id)?;
        if name.is_empty() {
            return Err(self.error(name_id, "test name must not be empty"));
        }
        let (mut input, mut expect) = (None, None);
        for member in self.fields(id, "Members") {
            let m = self.concrete(member, &["Source", "File", "Expectation"]);
            if self.kind(m) == "Expectation" {
                if expect.is_some() {
                    return Err(self.error(m, "repeated expectation"));
                }
                expect = Some(self.expectation(m)?);
            } else {
                if input.is_some() {
                    return Err(self.error(m, "exactly one source or file is required"));
                }
                input = Some(if self.kind(m) == "File" {
                    Input::File(self.string(self.field(m, "Path"))?)
                } else {
                    let body = self.field(m, "Body");
                    let text = self.text(body);
                    if text.starts_with("<[") {
                        let span = self.span(body);
                        Input::Inline {
                            text: text[2..text.len() - 2].into(),
                            raw_span: Some([span[0] + 2, span[1] - 2]),
                        }
                    } else {
                        Input::Inline {
                            text: self.string(body)?,
                            raw_span: None,
                        }
                    }
                });
            }
        }
        let input = input.ok_or_else(|| self.error(id, "test needs source or file"))?;
        let expect = expect.ok_or_else(|| self.error(id, "test needs expectation"))?;
        if let Input::Inline { text, .. } = &input {
            validate_spans(text, &expect).map_err(|message| self.error(id, message))?;
        }
        Ok(Case {
            name,
            span: self.span(id),
            name_span: self.span(name_id),
            input,
            expect,
        })
    }
}
pub fn validate_spans(text: &str, expect: &Expectation) -> Result<(), String> {
    for diagnostic in expect.diagnostics.iter().flatten() {
        if let Some([start, end]) = diagnostic.span {
            if start > end
                || !text.is_char_boundary(start as usize)
                || !text.is_char_boundary(end as usize)
            {
                return Err("diagnostic span must be in bounds at UTF-8 boundaries".into());
            }
        }
    }
    Ok(())
}
pub fn parse_suite(text: &str) -> Result<Suite, SuiteError> {
    if text.len() > 8 * 1024 * 1024 {
        return Err(SuiteError {
            span: [0, 0],
            message: "suite exceeds 8 MiB".into(),
        });
    }
    let tree = nitride_core::parse_with_options(
        &super::frontend::LANGUAGE,
        text,
        Limits::default(),
        nitride_core::ParseOptions {
            strict_comments: true,
            token_children: false,
            alias_nodes: true,
        },
    )
    .map_err(|e| SuiteError {
        span: e.span.map(|b| b as usize),
        message: e.message,
    })?;
    let d = Decoder { tree: &tree };
    let root = d.concrete(tree.root, &["Suite"]);
    if d.text(d.field(root, "Version")) != "1" {
        return Err(d.error(
            d.field(root, "Version"),
            "unsupported test suite version; expected 1",
        ));
    }
    let (mut project, mut tests) = (None, Vec::new());
    let mut names = std::collections::HashSet::new();
    for member in d.fields(root, "Members") {
        let m = d.concrete(member, &["Project", "Test"]);
        if d.kind(m) == "Project" {
            if project.is_some() {
                return Err(d.error(m, "repeated project declaration"));
            }
            let path = d.string(d.field(m, "Path"))?;
            if path.is_empty() {
                return Err(d.error(m, "project path must not be empty"));
            }
            project = Some(path);
        } else {
            let case = d.case(m)?;
            if !names.insert(case.name.clone()) {
                return Err(d.error(m, "duplicate test name"));
            }
            tests.push(case);
        }
    }
    if tests.is_empty() || tests.len() > 1000 {
        return Err(d.error(root, "suite must contain 1..1000 tests"));
    }
    Ok(Suite {
        project: project.ok_or_else(|| d.error(root, "suite needs one project"))?,
        tests,
    })
}
impl Case {
    pub fn metadata(&self) -> Value {
        json!({"name":self.name,"span":self.span,"name_span":self.name_span,"phase":self.expect.phase.name(),"source":match &self.input{Input::File(path)=>json!({"file":path}),Input::Inline{text,raw_span}=>json!({"text":text,"raw_span":raw_span})}})
    }
}
