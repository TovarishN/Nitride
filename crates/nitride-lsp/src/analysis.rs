//! Whole-document analysis: syntax tree, diagnostics and declared names.
use crate::lines::LineIndex;
use crate::registry::{EMPTY_CATALOG, LanguageState, Loaded, Registry, same_file};
use nitride_core::*;
use nitride_prepare::{GrammarItem, PrepareError, Source};
use std::path::Path;
use std::sync::Arc;

/// The language a tree was parsed with: a workspace language, or the
/// compiled grammar frontend for `.ngr` files.
#[derive(Clone)]
pub enum Grammar {
    Workspace(Arc<Loaded>),
    Frontend,
}

impl Grammar {
    pub fn language(&self) -> &Language<'_> {
        match self {
            Grammar::Workspace(loaded) => loaded.language.language(),
            Grammar::Frontend => nitride_prepare::grammar_language(),
        }
    }
}

/// A syntax tree that owns its data; it keeps its language alive.
pub struct Tree {
    pub grammar: Grammar,
    pub nodes: Vec<Node>,
    /// Child node ids, indexed by `Node::first_child`.
    pub edges: Vec<u32>,
    pub root: u32,
}

impl Tree {
    fn new(grammar: Grammar, tree: &SyntaxTree<'_>) -> Self {
        Self {
            nodes: tree.nodes.clone(),
            edges: tree.edges.iter().map(|e| e.node).collect(),
            root: tree.root,
            grammar,
        }
    }
    pub fn children(&self, id: u32) -> &[u32] {
        let node = self.nodes[id as usize];
        let start = node.first_child as usize;
        self.edges
            .get(start..start + node.child_count as usize)
            .unwrap_or(&[])
    }
    pub fn rule(&self, id: u32) -> &Rule<'_> {
        &self.grammar.language().rules[self.nodes[id as usize].rule as usize]
    }
}

/// A name a `declares` clause introduces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredName {
    pub name: String,
    pub kind: String,
    pub node_span: [u32; 2],
    pub name_span: [u32; 2],
}

/// An LSP-neutral diagnostic over byte spans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub span: [u32; 2],
    /// 1 error, 2 warning.
    pub severity: u8,
    pub message: String,
    pub code: Option<String>,
}

pub struct Analysis {
    pub lines: LineIndex,
    pub tree: Option<Tree>,
    pub problems: Vec<Problem>,
    pub declarations: Vec<DeclaredName>,
    /// A grammar file's modules and members; empty for programs.
    pub outline: Vec<GrammarItem>,
}

/// Analyses one document; `None` when no language serves it.
pub fn analyze(
    registry: &Registry,
    path: &Path,
    text: &str,
    open: &dyn Fn(&Path) -> Option<String>,
) -> Option<Analysis> {
    if path.extension().is_some_and(|e| e == "ngr") {
        return Some(grammar(registry, path, text, open));
    }
    let state = registry.for_path(path)?;
    Some(program(state, text))
}

fn program(state: &LanguageState, text: &str) -> Analysis {
    let lines = LineIndex::new(text);
    let Some(loaded) = state.current.clone() else {
        let reason = state
            .errors
            .first()
            .map_or_else(|| "it did not prepare".to_owned(), |e| e.to_string());
        return Analysis {
            lines,
            tree: None,
            problems: vec![Problem {
                span: [0, 0],
                severity: 1,
                message: format!("language {} is unavailable: {reason}", state.entry.name),
                code: None,
            }],
            declarations: vec![],
            outline: vec![],
        };
    };
    let language = loaded.language.language();
    let limits = Limits::default();
    let parsed = parse_recovering(language, text, limits, RecoveryOptions::default());
    let mut problems: Vec<Problem> = Vec::new();
    let mut add = |d: &Diagnostic| {
        let problem = Problem {
            span: d.span,
            severity: 1,
            message: d.message.clone(),
            code: d.code.as_deref().map(str::to_owned),
        };
        if !problems.contains(&problem) {
            problems.push(problem);
        }
    };
    parsed.diagnostics.iter().for_each(&mut add);
    // Syntax-only languages report recovery diagnostics; typed ones also check.
    compile_recovering(language, &parsed, limits)
        .diagnostics
        .iter()
        .for_each(&mut add);
    let declarations = declared_names(&loaded, &parsed.tree);
    let tree = Tree::new(Grammar::Workspace(loaded.clone()), &parsed.tree);
    Analysis {
        lines,
        tree: Some(tree),
        problems,
        declarations,
        outline: vec![],
    }
}

fn declared_names(loaded: &Loaded, tree: &SyntaxTree<'_>) -> Vec<DeclaredName> {
    let language = loaded.language.language();
    let mut names = Vec::new();
    for (id, node) in tree.nodes.iter().enumerate() {
        let rule = language.rules[node.rule as usize].name;
        for declaration in loaded
            .prepared
            .declarations
            .iter()
            .filter(|d| d.rule == rule)
        {
            let Some(field) = tree.field(id as u32, &declaration.field) else {
                continue;
            };
            let Some(text) = tree.text(field) else {
                continue;
            };
            names.push(DeclaredName {
                name: text.to_owned(),
                kind: declaration.kinds.first().cloned().unwrap_or_default(),
                node_span: node.span,
                name_span: tree.nodes[field as usize].span,
            });
        }
    }
    names.sort_by_key(|n| n.name_span[0]);
    names
}

/// A grammar file: preparation problems of every language built from it (or
/// of the file alone), and the frontend's tree for structure and colour.
fn grammar(
    registry: &Registry,
    path: &Path,
    text: &str,
    open: &dyn Fn(&Path) -> Option<String>,
) -> Analysis {
    let lines = LineIndex::new(text);
    let parsed = parse_recovering(
        nitride_prepare::grammar_language(),
        text,
        Limits::default(),
        RecoveryOptions::default(),
    );
    let mut reported: Vec<(PrepareError, u8)> = Vec::new();
    let mut users = registry.using_grammar(path).peekable();
    if users.peek().is_some() {
        for state in users {
            for error in &state.errors {
                reported.push((error.clone(), 1));
            }
            for warning in &state.warnings {
                reported.push((warning.clone(), 2));
            }
        }
    } else {
        let source = Source {
            path: path.to_string_lossy().into_owned(),
            text: open(path).unwrap_or_else(|| text.to_owned()),
        };
        match standalone(&source) {
            Ok(warnings) => reported.extend(warnings.into_iter().map(|w| (w, 2))),
            Err(error) => reported.push((error, 1)),
        }
    }
    let mut problems = Vec::new();
    for (error, severity) in reported {
        // Project errors (start rule, catalog) belong to the manifest.
        if error.project || !same_file(Path::new(&error.path), path) {
            continue;
        }
        let problem = Problem {
            span: [error.span[0] as u32, error.span[1] as u32],
            severity,
            message: error.message,
            code: None,
        };
        if !problems.contains(&problem) {
            problems.push(problem);
        }
    }
    let outline = nitride_prepare::grammar_outline(&Source {
        path: path.to_string_lossy().into_owned(),
        text: text.to_owned(),
    })
    .unwrap_or_default();
    Analysis {
        lines,
        tree: Some(Tree::new(Grammar::Frontend, &parsed.tree)),
        problems,
        declarations: vec![],
        outline,
    }
}

/// Prepares a grammar no manifest lists, starting at its first syntax rule.
fn standalone(source: &Source) -> Result<Vec<PrepareError>, PrepareError> {
    let outline = nitride_prepare::grammar_outline(source)?;
    let start = outline
        .iter()
        .find_map(|m| {
            m.children
                .iter()
                .find(|c| c.kind == "syntax" || c.kind == "extensible")
                .map(|c| format!("{}.{}", m.name, c.name))
        })
        .unwrap_or_default();
    if start.is_empty() {
        return Ok(vec![]);
    }
    nitride_prepare::prepare_sources_tolerant(std::slice::from_ref(source), EMPTY_CATALOG, &start)
        .map(|p| p.warnings)
}
