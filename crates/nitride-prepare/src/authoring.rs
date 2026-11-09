//! Shared deterministic language recipes and create-only filesystem execution.
mod execute;
pub use execute::{CreationReport, create_language, workspace_target};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateKind {
    Syntax,
    Typed,
}
#[derive(Clone, Debug)]
pub struct TemplateOptions {
    pub name: String,
    pub extension: String,
    pub kind: TemplateKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedFile {
    pub relative_path: PathBuf,
    pub contents: String,
    pub kind: &'static str,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectPlan {
    pub files: Vec<PlannedFile>,
}
#[derive(Debug)]
pub struct AuthoringError {
    pub message: String,
    pub retained: Vec<PathBuf>,
}
impl std::fmt::Display for AuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if !self.retained.is_empty() {
            write!(f, "; retained partial paths: {:?}", self.retained)?;
        }
        Ok(())
    }
}
impl std::error::Error for AuthoringError {}
pub(crate) fn error(message: impl Into<String>) -> AuthoringError {
    AuthoringError {
        message: message.into(),
        retained: vec![],
    }
}
pub fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
pub fn plan_language(options: &TemplateOptions) -> Result<ProjectPlan, AuthoringError> {
    let TemplateOptions {
        name,
        extension,
        kind,
    } = options;
    if !identifier(name) {
        return Err(error("language name must be an ASCII identifier"));
    }
    if !extension.starts_with('.')
        || extension.len() < 2
        || !extension[1..]
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(error(
            "extension must be a dot followed by ASCII letters, digits or underscores",
        ));
    }
    let typed = *kind == TemplateKind::Typed;
    let catalog = if typed {
        format!("  catalog {name}.ncatalog;\n")
    } else {
        String::new()
    };
    let mut files = Vec::new();
    let mut add = |path: String, contents: String, kind| {
        files.push(PlannedFile {
            relative_path: path.into(),
            contents,
            kind,
        })
    };
    let grammar = if typed {
        format!(
            r#"syntax module {name}
{{
  symbols {{ value }}
  token Identifier = Nitride.Lexical.AsciiIdentifier except "let" "show";
  token Number = Nitride.Lexical.UnsignedDecimal;
  syntax Program = Statements:Statement*;
  syntax Statement = Let / Show;
  syntax Let = "let" Name:Identifier "=" Value:Expr ";" declares value Name sequential;
  syntax Show = "show" Value:Expr ";";
  syntax Expr = NumberValue / Reference;
  syntax NumberValue = Value:Number lowers literal Core.Scalar Value;
  syntax Reference = Name:Identifier references value Name;
}}
"#
        )
    } else {
        format!(
            r#"syntax module {name}
{{
  token Identifier = Nitride.Lexical.AsciiIdentifier;
  token Number = Nitride.Lexical.UnsignedDecimal;
  syntax Program = Items:Item*;
  syntax Item = Name:Identifier "=" Value:Number ";";
}}
"#
        )
    };
    add(format!("{name}.ngr"), grammar, "grammar");
    if typed {
        add(
            format!("{name}.ncatalog"),
            format!(
                "catalog module {name}\n{{\n  let Let(Value) infer;\n  show Show(Value);\n}}\n"
            ),
            "catalog",
        );
    }
    add(
        format!("sample{extension}"),
        if typed {
            "let answer = 42;\nshow answer;\n"
        } else {
            "answer = 42;\n"
        }
        .into(),
        "sample",
    );
    let command = if typed { "run" } else { "parse" };
    add(
        "README.md".into(),
        format!(
            "# {name}\n\nRun `nitride {command} nitride.nproj sample{extension}`.\n\nThe project pins the offline bundled `Nitride.Lexical@1.0.0` library.\nKeep keyword exclusions and trivia policy in your language.\nUse Go to Definition on a qualified library token to inspect its read-only source.\n\nPrepare standalone tables with `nitride prepare nitride.nproj --output language.json`.\n"
        ),
        "readme",
    );
    // Commit marker last: a partial directory is never mistaken for a project.
    add(
        "nitride.nproj".into(),
        format!(
            "language {name}\n{{\n  extensions {extension};\n  library Nitride.Lexical version 1.0.0;\n  grammar {name}.ngr;\n  start {name}.Program;\n{catalog}}}\n"
        ),
        "project",
    );
    let mut names = std::collections::HashSet::new();
    if files
        .iter()
        .any(|f| !names.insert(f.relative_path.to_string_lossy().to_lowercase()))
    {
        return Err(error("template file names collide"));
    }
    Ok(ProjectPlan { files })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectIssueKind {
    MissingGrammar,
    MissingCatalog,
    MissingLexicalPin,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectIssue {
    pub code: &'static str,
    pub kind: ProjectIssueKind,
    pub span: [usize; 2],
    pub target: Option<PathBuf>,
    pub project: PathBuf,
}
/// Structured project issues; consumers never interpret diagnostic prose.
pub fn project_issues(
    source: &crate::Source,
    exists: &dyn Fn(&std::path::Path) -> bool,
) -> Vec<ProjectIssue> {
    crate::project_references(source)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| {
            let target = r.target?;
            let kind = match r.kind {
                "grammar" => ProjectIssueKind::MissingGrammar,
                "catalog" => ProjectIssueKind::MissingCatalog,
                _ => return None,
            };
            (!exists(&target)).then(|| ProjectIssue {
                code: if kind == ProjectIssueKind::MissingGrammar {
                    "NPROJ0001"
                } else {
                    "NPROJ0002"
                },
                kind,
                span: r.span,
                target: Some(target),
                project: source.path.clone().into(),
            })
        })
        .collect()
}
/// A missing file resolves only its declared reference. A start rule is added
/// only when the owning project names it and no other input already defines it.
pub fn missing_file_scaffold(
    kind: &ProjectIssueKind,
    path: &std::path::Path,
    start: Option<&str>,
) -> Result<String, AuthoringError> {
    missing_file_scaffold_in(kind, path, start, &[])
}
/// Contextual recipe: never redeclare a module already present in the project.
pub fn missing_file_scaffold_in(
    kind: &ProjectIssueKind,
    path: &std::path::Path,
    start: Option<&str>,
    occupied: &[String],
) -> Result<String, AuthoringError> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| identifier(s))
        .unwrap_or("Language");
    let mut fallback = stem.to_owned();
    let mut suffix = 1;
    while occupied.contains(&fallback) {
        fallback = format!("{stem}_{suffix}");
        suffix += 1;
    }
    if *kind == ProjectIssueKind::MissingCatalog {
        return Ok(format!("catalog module {fallback}\n{{\n}}\n"));
    }
    let declared = start
        .and_then(|s| s.rsplit_once('.'))
        .filter(|(module, rule)| {
            module.split('.').all(identifier)
                && identifier(rule)
                && !occupied.iter().any(|m| m == module)
        });
    let (module,body)=declared.map_or((fallback,String::new()),|(module,rule)|(module.into(),format!("  // Replace this placeholder with your language's syntax.\n  syntax {rule} = \"TODO\";\n")));
    Ok(format!("syntax module {module}\n{{\n{body}}}\n"))
}

/// Locate a real language-block boundary, then verify the proposed insertion
/// with the actual frontend (comments may contain arbitrary braces).
pub fn library_pin_insertion(source: &crate::Source) -> Option<usize> {
    let outline = crate::project_outline(source).ok()?;
    let item = outline.first().filter(|i| i.kind == "language")?;
    let insert = item.span[1].checked_sub(1)?;
    if source.text.as_bytes().get(insert) != Some(&b'}') {
        return None;
    }
    let mut text = source.text.clone();
    text.insert_str(insert, "\n  library Nitride.Lexical version 1.0.0;\n");
    let parsed = crate::read_language(
        &crate::Source {
            path: source.path.clone(),
            text,
        },
        &|_| true,
    )
    .ok()?;
    (parsed
        .libraries
        .iter()
        .filter(|r| r.name == "Nitride.Lexical" && r.version == "1.0.0")
        .count()
        == 1)
        .then_some(insert)
}
