//! Project files (`.nproj`): parsed by the generated project frontend and
//! resolved into a language, or the workspace that lists language folders.
use crate::{PrepareError, Source};
use nitride_core::{Limits, SyntaxTree};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One language as its `.nproj` declares it; paths are joined to the file's
/// directory, lists keep source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLanguage {
    pub path: PathBuf,
    pub name: String,
    pub name_span: [usize; 2],
    /// With their leading dot.
    pub extensions: Vec<String>,
    /// The span of each extension, in `extensions` order.
    pub extension_spans: Vec<[usize; 2]>,
    pub grammars: Vec<PathBuf>,
    pub start: String,
    pub start_span: [usize; 2],
    pub catalogs: Vec<PathBuf>,
    pub nitrogen: Option<NitrogenSection>,
}

impl ProjectLanguage {
    /// A language with a catalog is typed; one without is syntax-only.
    pub fn typed(&self) -> bool {
        !self.catalogs.is_empty()
    }
}

/// What differs for Nitrogen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NitrogenSection {
    pub namespace: String,
    /// The block's own grammar list, or the language's when it gives none.
    pub grammars: Vec<PathBuf>,
    pub sources: Vec<PathBuf>,
}

/// The languages a workspace file includes, in include order, and the
/// problems of the workspace and of each included file.
#[derive(Clone, Debug, Default)]
pub struct ProjectWorkspace {
    pub languages: Vec<ProjectLanguage>,
    pub errors: Vec<PrepareError>,
}

/// The project frontend's tree of one source.
fn syntax(source: &Source) -> Result<SyntaxTree<'static>, PrepareError> {
    nitride_core::parse_with_options(
        &crate::project_seed::LANGUAGE,
        &source.text,
        Limits::default(),
        nitride_core::ParseOptions {
            strict_comments: true,
            token_children: false,
            alias_nodes: true,
        },
    )
    .map_err(|e| source.error(e.span.map(|v| v as usize), e.message))
}

/// A path or name as written, where it was written.
struct Written {
    text: String,
    span: [usize; 2],
}

struct Decoder<'a> {
    tree: &'a SyntaxTree<'static>,
}
impl Decoder<'_> {
    fn kind(&self, id: u32) -> &str {
        crate::project_seed::LANGUAGE.rules[self.tree.nodes[id as usize].rule as usize]
            .name
            .rsplit('.')
            .next()
            .unwrap()
    }
    fn span(&self, id: u32) -> [usize; 2] {
        self.tree.nodes[id as usize].span.map(|v| v as usize)
    }
    /// The node of one of `kinds` that `id` stands for (choices wrap it).
    fn concrete(&self, mut id: u32, kinds: &[&str]) -> u32 {
        while !kinds.contains(&self.kind(id)) {
            id = self.tree.children(id)[0].node;
        }
        id
    }
    fn written(&self, id: u32) -> Written {
        Written {
            text: self.tree.text(id).unwrap().into(),
            span: self.span(id),
        }
    }
    fn field(&self, id: u32, label: &str) -> Written {
        self.written(self.tree.field(id, label).expect("generated project field"))
    }
    fn fields(&self, id: u32, label: &str) -> Vec<Written> {
        self.tree
            .children(id)
            .iter()
            .filter(|e| self.tree.label(e) == label)
            .map(|e| self.written(e.node))
            .collect()
    }
    /// The root item: a `Language` or `Workspace` node.
    fn item(&self) -> u32 {
        let item = self.tree.field(self.tree.root, "Item").expect("project item");
        self.concrete(item, &["Language", "Workspace"])
    }
}

/// The node id of each `label` child of `id`.
fn children(tree: &SyntaxTree<'static>, id: u32, label: &str) -> Vec<u32> {
    tree.children(id)
        .iter()
        .filter(|e| tree.label(e) == label)
        .map(|e| e.node)
        .collect()
}

fn directory(source: &Source) -> PathBuf {
    Path::new(&source.path)
        .parent()
        .map(Path::to_owned)
        .unwrap_or_default()
}

/// One language file. `exists` says whether a referenced file exists.
pub fn read_language(
    source: &Source,
    exists: &dyn Fn(&Path) -> bool,
) -> Result<ProjectLanguage, Vec<PrepareError>> {
    let tree = syntax(source).map_err(|e| vec![e])?;
    let d = Decoder { tree: &tree };
    let item = d.item();
    if d.kind(item) != "Language" {
        return Err(vec![source.error(
            [d.span(item)[0], d.span(item)[0] + "workspace".len()],
            "expected a language, not a workspace",
        )]);
    }
    let mut errors = Vec::new();
    let directory = directory(source);
    let resolve = |paths: Vec<Written>, errors: &mut Vec<PrepareError>| -> Vec<PathBuf> {
        paths
            .into_iter()
            .map(|p| {
                let path = directory.join(&p.text);
                if !exists(&path) {
                    errors.push(source.error(p.span, format!("file not found: {}", p.text)));
                }
                path
            })
            .collect()
    };
    let name = d.field(item, "Name");
    let mut extensions = Vec::new();
    let mut grammars = Vec::new();
    let mut start: Option<Written> = None;
    let mut catalogs = Vec::new();
    let mut nitrogen: Option<(u32, Option<String>, Option<Vec<PathBuf>>, Vec<PathBuf>)> = None;
    for member in children(&tree, item, "Members") {
        let m = d.concrete(
            member,
            &["Extensions", "Grammar", "Start", "Catalog", "Nitrogen"],
        );
        match d.kind(m) {
            "Extensions" => extensions.extend(d.fields(m, "Items")),
            "Grammar" => {
                let paths = resolve(d.fields(m, "Paths"), &mut errors);
                grammars.extend(paths);
            }
            "Catalog" => {
                let paths = resolve(d.fields(m, "Paths"), &mut errors);
                catalogs.extend(paths);
            }
            "Start" => {
                if start.is_some() {
                    errors.push(source.error(d.span(m), "repeated start rule"));
                } else {
                    start = Some(d.field(m, "Rule"));
                }
            }
            _ => {
                if nitrogen.is_some() {
                    errors.push(source.error(d.span(m), "repeated nitrogen block"));
                    continue;
                }
                let (mut namespace, mut own, mut sources) = (None, None::<Vec<PathBuf>>, vec![]);
                for member in children(&tree, m, "Members") {
                    let n = d.concrete(member, &["Namespace", "Grammar", "Source"]);
                    match d.kind(n) {
                        "Namespace" if namespace.is_some() => {
                            errors.push(source.error(d.span(n), "repeated namespace"))
                        }
                        "Namespace" => namespace = Some(d.field(n, "Name").text),
                        "Grammar" => {
                            let paths = resolve(d.fields(n, "Paths"), &mut errors);
                            own.get_or_insert_with(Vec::new).extend(paths);
                        }
                        _ => sources.extend(resolve(d.fields(n, "Paths"), &mut errors)),
                    }
                }
                nitrogen = Some((m, namespace, own, sources));
            }
        }
    }
    if grammars.is_empty() {
        errors.push(source.error(name.span, "language needs a grammar"));
    }
    let Some(start) = start else {
        errors.push(source.error(name.span, "language needs a start rule"));
        return Err(errors);
    };
    let nitrogen = match nitrogen {
        None => None,
        Some((block, None, _, _)) => {
            let at = d.span(block)[0];
            errors.push(source.error(
                [at, at + "nitrogen".len()],
                "nitrogen block needs a namespace",
            ));
            None
        }
        Some((_, Some(namespace), own, sources)) => Some(NitrogenSection {
            namespace,
            grammars: own.unwrap_or_else(|| grammars.clone()),
            sources,
        }),
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(ProjectLanguage {
        path: PathBuf::from(&source.path),
        name: name.text,
        name_span: name.span,
        extension_spans: extensions.iter().map(|e| e.span).collect(),
        extensions: extensions.into_iter().map(|e| e.text).collect(),
        grammars,
        start: start.text,
        start_span: start.span,
        catalogs,
        nitrogen,
    })
}

/// A workspace file and every language it includes, in include order. `read`
/// supplies file text (editor buffers first); an extension claimed twice stays
/// with the earlier language.
pub fn read_workspace(
    source: &Source,
    read: &dyn Fn(&Path) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
) -> ProjectWorkspace {
    let mut workspace = ProjectWorkspace::default();
    let tree = match syntax(source) {
        Ok(tree) => tree,
        Err(error) => {
            workspace.errors.push(error);
            return workspace;
        }
    };
    let d = Decoder { tree: &tree };
    let item = d.item();
    if d.kind(item) != "Workspace" {
        let at = d.span(item)[0];
        workspace.errors.push(source.error(
            [at, at + "language".len()],
            "expected a workspace, not a language",
        ));
        return workspace;
    }
    let base = directory(source);
    let mut claimed: HashMap<String, String> = HashMap::new();
    for include in children(&tree, item, "Includes") {
        let path = d.field(include, "Path");
        let folder = base.join(&path.text);
        let mut found: Vec<PathBuf> = std::fs::read_dir(&folder)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "nproj"))
            .collect();
        found.sort();
        let file = match found.as_slice() {
            [] => {
                workspace
                    .errors
                    .push(source.error(path.span, format!("no .nproj in {}", path.text)));
                continue;
            }
            [file] => file.clone(),
            _ => {
                workspace
                    .errors
                    .push(source.error(path.span, format!("several .nproj in {}", path.text)));
                continue;
            }
        };
        let Some(text) = read(&file) else {
            workspace
                .errors
                .push(source.error(path.span, format!("cannot read {}", file.display())));
            continue;
        };
        let included = Source {
            path: file.display().to_string(),
            text,
        };
        match read_language(&included, exists) {
            Err(errors) => workspace.errors.extend(errors),
            Ok(mut language) => {
                let mut kept = (Vec::new(), Vec::new());
                for (extension, span) in language.extensions.iter().zip(&language.extension_spans)
                {
                    match claimed.get(&extension.to_lowercase()) {
                        Some(owner) => workspace.errors.push(included.error(
                            *span,
                            format!("extension {extension} is already claimed by {owner}"),
                        )),
                        None => {
                            claimed.insert(extension.to_lowercase(), language.name.clone());
                            kept.0.push(extension.clone());
                            kept.1.push(*span);
                        }
                    }
                }
                (language.extensions, language.extension_spans) = kept;
                workspace.languages.push(language);
            }
        }
    }
    workspace
}

/// A language resolved for comparison across implementations: paths relative
/// to `root`, with `/` and without `.` or `..`. Nitrogen's own resolution of
/// the same `.nproj` must give this value.
pub fn snapshot(language: &ProjectLanguage, root: &Path) -> serde_json::Value {
    let relative = |path: &PathBuf| {
        let mut parts: Vec<String> = Vec::new();
        let path = path.strip_prefix(root).unwrap_or(path);
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    parts.pop();
                }
                other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
            }
        }
        parts.join("/")
    };
    let all = |paths: &[PathBuf]| paths.iter().map(relative).collect::<Vec<_>>();
    serde_json::json!({
        "name": language.name,
        "extensions": language.extensions,
        "grammars": all(&language.grammars),
        "start": language.start,
        "typed": language.typed(),
        "catalogs": all(&language.catalogs),
        "nitrogen": language.nitrogen.as_ref().map(|n| serde_json::json!({
            "namespace": n.namespace,
            "grammars": all(&n.grammars),
            "sources": all(&n.sources),
        })),
    })
}

/// A name a `.nproj` uses: a "grammar", "catalog" or "source" file, an
/// "include" folder (its target is the folder's one `.nproj`), or the start
/// "rule".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectReference {
    pub kind: &'static str,
    pub text: String,
    pub span: [usize; 2],
    pub target: Option<PathBuf>,
}

/// The language or workspace a `.nproj` declares (kind "language" or
/// "workspace").
pub fn outline(source: &Source) -> Result<Vec<crate::GrammarItem>, PrepareError> {
    let tree = syntax(source)?;
    let d = Decoder { tree: &tree };
    let item = d.item();
    let (kind, name) = if d.kind(item) == "Language" {
        ("language", d.field(item, "Name"))
    } else {
        let at = d.span(item)[0];
        (
            "workspace",
            Written {
                text: "workspace".into(),
                span: [at, at + "workspace".len()],
            },
        )
    };
    Ok(vec![crate::GrammarItem {
        kind,
        name: name.text,
        span: d.span(item),
        name_span: name.span,
        children: vec![],
    }])
}

/// Every file, folder and rule a `.nproj` names, in source order.
pub fn references(source: &Source) -> Result<Vec<ProjectReference>, PrepareError> {
    let tree = syntax(source)?;
    let d = Decoder { tree: &tree };
    let base = directory(source);
    let mut found = Vec::new();
    let files = |kind: &'static str, paths: Vec<Written>, found: &mut Vec<ProjectReference>| {
        for path in paths {
            found.push(ProjectReference {
                kind,
                target: Some(base.join(&path.text)),
                text: path.text,
                span: path.span,
            });
        }
    };
    let item = d.item();
    if d.kind(item) == "Workspace" {
        for include in children(&tree, item, "Includes") {
            let path = d.field(include, "Path");
            let folder = base.join(&path.text);
            let nproj: Vec<PathBuf> = std::fs::read_dir(&folder)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "nproj"))
                .collect();
            found.push(ProjectReference {
                kind: "include",
                target: (nproj.len() == 1).then(|| nproj[0].clone()),
                text: path.text,
                span: path.span,
            });
        }
        return Ok(found);
    }
    let mut members = children(&tree, item, "Members");
    while let Some(member) = members.pop() {
        let m = d.concrete(
            member,
            &["Extensions", "Grammar", "Start", "Catalog", "Nitrogen", "Namespace", "Source"],
        );
        match d.kind(m) {
            "Grammar" => files("grammar", d.fields(m, "Paths"), &mut found),
            "Catalog" => files("catalog", d.fields(m, "Paths"), &mut found),
            "Source" => files("source", d.fields(m, "Paths"), &mut found),
            "Nitrogen" => members.extend(children(&tree, m, "Members")),
            "Start" => {
                let rule = d.field(m, "Rule");
                found.push(ProjectReference {
                    kind: "rule",
                    text: rule.text,
                    span: rule.span,
                    target: None,
                });
            }
            _ => {}
        }
    }
    found.sort_by_key(|r| r.span[0]);
    Ok(found)
}
