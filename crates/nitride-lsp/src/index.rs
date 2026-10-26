//! A document's symbols: definitions, the uses that resolve to them,
//! built-ins and expression types.
use crate::analysis::DeclaredName;
use nitride_core::{Language, Program, SyntaxTree};
use nitride_prepare::Prepared;

#[derive(Clone, Debug, Default)]
pub struct SymbolIndex {
    pub definitions: Vec<Definition>,
    pub references: Vec<Use>,
    pub builtins: Vec<BuiltinUse>,
    /// Typed expression spans, from the engine's HIR.
    pub types: Vec<([u32; 2], String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub kind: String,
    pub name_span: [u32; 2],
    pub node_span: [u32; 2],
    pub ty: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Use {
    pub span: [u32; 2],
    /// An index into `definitions`.
    pub target: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltinUse {
    pub span: [u32; 2],
    pub kind: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Definition(usize),
    Builtin(usize),
}

fn contains(span: [u32; 2], offset: u32) -> bool {
    span[0] <= offset && offset <= span[1]
}

impl SymbolIndex {
    /// The symbol whose name is at `offset` (its end included).
    pub fn at(&self, offset: u32) -> Option<Target> {
        if let Some(i) = self
            .definitions
            .iter()
            .position(|d| contains(d.name_span, offset))
        {
            return Some(Target::Definition(i));
        }
        if let Some(u) = self.references.iter().find(|u| contains(u.span, offset)) {
            return Some(Target::Definition(u.target));
        }
        self.builtins
            .iter()
            .position(|b| contains(b.span, offset))
            .map(Target::Builtin)
    }

    /// A definition's name and every use of it, in text order; true marks
    /// the declaration.
    pub fn occurrences(&self, definition: usize) -> Vec<([u32; 2], bool)> {
        let mut found = vec![(self.definitions[definition].name_span, true)];
        found.extend(
            self.references
                .iter()
                .filter(|u| u.target == definition)
                .map(|u| (u.span, false)),
        );
        found.sort_by_key(|(span, _)| span[0]);
        found
    }

    /// From the engine: its bindings resolve each use to the start of the
    /// declared name, and its HIR carries types.
    pub fn typed(declared: &[DeclaredName], program: &Program<'_>) -> Self {
        let mut index = Self::definitions(declared);
        for definition in &mut index.definitions {
            definition.ty = program
                .statements
                .iter()
                .find(|s| s.symbol == Some(definition.name_span[0]))
                .and_then(|s| program.nodes.get(s.value as usize))
                .map(|n| n.ty.to_owned());
        }
        for binding in &program.bindings {
            if let Some(target) = index
                .definitions
                .iter()
                .position(|d| d.name_span[0] == binding.symbol)
            {
                index.references.push(Use {
                    span: binding.span,
                    target,
                });
            }
        }
        index.builtins = program
            .builtin_bindings
            .iter()
            .map(|b| BuiltinUse {
                span: b.span,
                kind: b.kind.to_owned(),
                name: b.name.to_owned(),
            })
            .collect();
        let length = program.text.len() as u32;
        index.types = program
            .nodes
            .iter()
            .filter(|n| {
                n.ty != nitride_core::ERROR_TYPE && n.span[1] <= length && n.span[0] < n.span[1]
            })
            .map(|n| (n.span, n.ty.to_owned()))
            .collect();
        index.references.sort_by_key(|u| u.span[0]);
        index
    }

    /// By spelling, from the grammar's `references` clauses: a use names the
    /// nearest earlier definition of a matching kind, else the first later one.
    /// A syntax-only alias rule (`Ref = Name:Identifier`) leaves no node, so its
    /// uses are the remaining tokens of the rule it stands for.
    pub fn by_name(
        declared: &[DeclaredName],
        prepared: &Prepared,
        tree: &SyntaxTree<'_>,
        language: &Language<'_>,
    ) -> Self {
        let mut index = Self::definitions(declared);
        let mut uses: Vec<([u32; 2], &str, &[String])> = Vec::new();
        for clause in &prepared.references {
            let Some(rule) = language.rules.iter().position(|r| r.name == clause.rule) else {
                continue;
            };
            if language.rules[rule].alias {
                let Some(token) = alias_target(prepared, &clause.rule, &clause.field) else {
                    continue;
                };
                for node in tree
                    .nodes
                    .iter()
                    .filter(|n| language.rules[n.rule as usize].name == token)
                {
                    if let Some(text) = tree
                        .source
                        .get(node.span[0] as usize..node.span[1] as usize)
                    {
                        uses.push((node.span, text, &clause.kinds));
                    }
                }
                continue;
            }
            for (id, _) in tree
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.rule as usize == rule)
            {
                let Some(field) = tree.field(id as u32, &clause.field) else {
                    continue;
                };
                if let Some(text) = tree.text(field) {
                    uses.push((tree.nodes[field as usize].span, text, &clause.kinds));
                }
            }
        }
        uses.sort_by_key(|(span, _, _)| span[0]);
        uses.dedup_by_key(|(span, _, _)| *span);
        for (span, text, kinds) in uses {
            let candidates = index
                .definitions
                .iter()
                .enumerate()
                .filter(|(_, d)| d.name == text && kinds.contains(&d.kind));
            if candidates.clone().any(|(_, d)| d.name_span == span) {
                continue; // The declaration itself.
            }
            let target = candidates
                .clone()
                .rfind(|(_, d)| d.name_span[0] < span[0])
                .or_else(|| candidates.clone().next())
                .map(|(i, _)| i);
            if let Some(target) = target {
                index.references.push(Use { span, target });
            }
        }
        index
    }

    fn definitions(declared: &[DeclaredName]) -> Self {
        Self {
            definitions: declared
                .iter()
                .map(|d| Definition {
                    name: d.name.clone(),
                    kind: d.kind.clone(),
                    name_span: d.name_span,
                    node_span: d.node_span,
                    ty: None,
                })
                .collect(),
            ..Self::default()
        }
    }
}

/// The rule an alias rule's labelled field refers to, from the prepared JSON.
fn alias_target(prepared: &Prepared, rule: &str, field: &str) -> Option<String> {
    let rules = prepared.data["rules"].as_array()?;
    let expr = &rules.iter().find(|r| r["name"] == rule)?["expr"];
    fn find(e: &serde_json::Value, field: &str) -> Option<String> {
        if e["kind"] == "label" && e["label"] == field && e["inner"]["kind"] == "ref" {
            return e["inner"]["name"].as_str().map(str::to_owned);
        }
        e["items"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(e.get("inner"))
            .find_map(|c| find(c, field))
    }
    find(expr, field)
}
