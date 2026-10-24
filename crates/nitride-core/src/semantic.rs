use crate::*;

#[derive(Debug)]
pub enum HirKind {
    Constant(f64),
    Reference(u32),
    Builtin(&'static BuiltinValue),
    /// An exact source spelling, `Program::text[start..end]`, feeding a text operation.
    Text {
        start: u32,
        end: u32,
    },
    Operation {
        operation: u16,
        args: std::ops::Range<u32>,
    },
}
#[derive(Debug)]
pub struct HirNode {
    pub kind: HirKind,
    pub ty: &'static str,
    pub span: [u32; 2],
}
#[derive(Debug)]
pub struct Statement {
    pub value: u32,
    pub symbol: Option<u32>,
}
#[derive(Debug)]
pub struct Binding {
    pub kind: &'static str,
    pub span: [u32; 2],
    pub symbol: u32,
}
#[derive(Debug)]
pub struct BuiltinBinding {
    pub kind: &'static str,
    pub name: &'static str,
    pub span: [u32; 2],
}
#[derive(Debug)]
pub struct Program {
    pub language: &'static Language,
    pub nodes: Vec<HirNode>,
    pub args: Vec<u32>,
    pub statements: Vec<Statement>,
    pub bindings: Vec<Binding>,
    pub builtin_bindings: Vec<BuiltinBinding>,
    pub origin_spans: Vec<[u32; 2]>,
    pub origin_ranges: Vec<std::ops::Range<u32>>,
    pub text: String,
}

pub const TEXT: &str = "Core.Text";

pub fn compile(
    language: &'static Language,
    tree: &SyntaxTree,
    limits: Limits,
) -> Result<Program, Vec<Diagnostic>> {
    if language.profile == Profile::SyntaxOnly {
        return Err(vec![Diagnostic::new(
            "capability",
            [0, 0],
            "syntax-only language does not admit semantic checking or execution",
        )]);
    }
    compile_checked(language, tree, limits).map_err(|e| vec![e])
}

fn compile_checked(
    language: &'static Language,
    tree: &SyntaxTree,
    limits: Limits,
) -> Result<Program, Diagnostic> {
    use std::collections::HashMap;
    if !std::ptr::eq(language, tree.language) {
        return Err(Diagnostic::new(
            "grammar",
            [0, 0],
            "syntax belongs to a different language snapshot",
        ));
    }
    if tree.root as usize >= tree.nodes.len() {
        return Err(Diagnostic::new("grammar", [0, 0], "invalid syntax root"));
    }
    for (index, node) in tree.nodes.iter().enumerate() {
        let end = node
            .first_child
            .checked_add(node.child_count)
            .ok_or_else(|| Diagnostic::new("grammar", node.span, "invalid child range"))?;
        if node.rule as usize >= language.rules.len()
            || node.span[0] > node.span[1]
            || tree
                .source
                .get(node.span[0] as usize..node.span[1] as usize)
                .is_none()
            || end as usize > tree.edges.len()
            || tree.edges[node.first_child as usize..end as usize]
                .iter()
                .any(|e| e.node as usize >= index)
        {
            return Err(Diagnostic::new(
                "grammar",
                node.span,
                "invalid syntax storage or non-postorder child",
            ));
        }
    }
    if language.rules.iter().any(|r| {
        matches!(
            r.action,
            Action::Template { .. }
                | Action::Parameter { .. }
                | Action::Alias { .. }
                | Action::Expand { .. }
        )
    }) {
        return crate::templates::compile(language, tree, limits);
    }
    let mut program = Program {
        language,
        nodes: Vec::new(),
        args: Vec::new(),
        statements: Vec::new(),
        bindings: Vec::new(),
        builtin_bindings: Vec::new(),
        origin_spans: Vec::new(),
        origin_ranges: Vec::new(),
        text: String::new(),
    };
    if tree.nodes.len() as u64 > limits.work {
        return Err(Diagnostic::new(
            "limit",
            [0, 0],
            "compiler work limit exceeded",
        ));
    }
    let mut environment = HashMap::<(&'static str, &str), (u32, u32)>::new();
    let mut references = HashMap::<u32, (u32, u32)>::new();
    let mut type_errors = Vec::new();
    let mut binding_errors = Vec::new();
    let mut call_targets = HashMap::<u32, &'static [u16]>::new();
    let mut value_targets = HashMap::<u32, &'static BuiltinValue>::new();
    let field = |id, label| {
        tree.field(id, label)
            .ok_or_else(|| Diagnostic::new("grammar", [0, 0], format!("missing field {label}")))
    };
    // Bind the complete document before reporting type errors, matching Nitrogen's stage order.
    for (id, node) in tree.nodes.iter().enumerate() {
        let rule = language
            .rules
            .get(node.rule as usize)
            .ok_or_else(|| Diagnostic::new("grammar", node.span, "unknown syntax kind"))?;
        match rule.action {
            Action::Reference {
                field: label,
                kinds,
                ..
            }
            | Action::Call {
                field: label,
                kinds,
                ..
            } => {
                let name = field(id as u32, label)?;
                let text = tree.text(name).ok_or_else(|| {
                    Diagnostic::new("grammar", node.span, "invalid identifier span")
                })?;
                // All local kinds precede all builtins, matching Nitrogen Project.Lookup.
                if let Some((kind, binding)) = kinds
                    .iter()
                    .find_map(|kind| environment.get(&(*kind, text)).copied().map(|b| (*kind, b)))
                {
                    references.insert(id as u32, binding);
                    program.bindings.push(Binding {
                        kind,
                        span: tree.nodes[name as usize].span,
                        symbol: binding.0,
                    });
                } else if let Action::Call { builtins, .. } = rule.action {
                    for (i, builtin) in builtins.iter().enumerate() {
                        if builtin.kind.is_empty()
                            || builtin.name.is_empty()
                            || !kinds.contains(&builtin.kind)
                            || builtins[..i]
                                .iter()
                                .any(|b| b.kind == builtin.kind && b.name == builtin.name)
                        {
                            return Err(Diagnostic::new(
                                "grammar",
                                node.span,
                                "invalid builtin call table",
                            ));
                        }
                    }
                    if let Some(builtin) = kinds.iter().find_map(|kind| {
                        builtins.iter().find(|b| b.kind == *kind && b.name == text)
                    }) {
                        call_targets.insert(id as u32, builtin.candidates);
                        program.builtin_bindings.push(BuiltinBinding {
                            kind: builtin.kind,
                            name: builtin.name,
                            span: tree.nodes[name as usize].span,
                        });
                    } else {
                        binding_errors.push(Diagnostic::new(
                            "binding",
                            tree.nodes[name as usize].span,
                            format!("unresolved name {text}"),
                        ));
                    }
                } else if let Some(value) = match rule.action {
                    // Built-in values follow every local kind, in authored kind order.
                    Action::Reference { values, .. } => kinds
                        .iter()
                        .find_map(|kind| values.iter().find(|v| v.kind == *kind && v.name == text)),
                    _ => None,
                } {
                    value_targets.insert(id as u32, value);
                    program.builtin_bindings.push(BuiltinBinding {
                        kind: value.kind,
                        name: value.name,
                        span: tree.nodes[name as usize].span,
                    });
                } else {
                    binding_errors.push(Diagnostic::new(
                        "binding",
                        tree.nodes[name as usize].span,
                        format!("unresolved name {text}"),
                    ));
                }
            }
            Action::Declare {
                kind,
                field: label,
                annotation,
                ..
            } => {
                let name = field(id as u32, label)?;
                if let Some(annotation) = annotation {
                    let annotation = field(id as u32, annotation)?;
                    let spelling = tree.text(annotation).ok_or_else(|| {
                        Diagnostic::new("grammar", node.span, "invalid annotation span")
                    })?;
                    let matches: Vec<_> = language
                        .types
                        .iter()
                        .copied()
                        .filter(|t| *t == spelling || t.rsplit('.').next() == Some(spelling))
                        .collect();
                    if matches.len() != 1 {
                        type_errors.push(Diagnostic::new(
                            "type",
                            tree.nodes[annotation as usize].span,
                            "unknown or ambiguous declared type",
                        ));
                    }
                }
                environment.insert(
                    (
                        kind,
                        tree.text(name).ok_or_else(|| {
                            Diagnostic::new("grammar", node.span, "invalid name span")
                        })?,
                    ),
                    (tree.nodes[name as usize].span[0], id as u32),
                );
            }
            _ => {}
        }
    }
    if !binding_errors.is_empty() {
        binding_errors.sort_by_key(|e| e.span[0]);
        return Err(binding_errors.remove(0));
    }
    let mut lowered = vec![None; tree.nodes.len()];
    let mut depths = Vec::<u32>::new();
    for (id, node) in tree.nodes.iter().enumerate() {
        let rule = &language.rules[node.rule as usize];
        let value_of = |label| -> Result<u32, Diagnostic> {
            let child = field(id as u32, label)?;
            lowered[child as usize].ok_or_else(|| {
                Diagnostic::new("type", node.span, format!("untyped argument {label}"))
            })
        };
        let mut depth = 1;
        for check in language.checks.iter().filter(|c| c.rule == node.rule) {
            // An absent optional field has no spelling, so its predicate fails.
            let spelling = tree
                .field(id as u32, check.field)
                .and_then(|c| tree.text(c));
            let holds = spelling.is_some_and(|text| match check.predicate {
                Predicate::IsoDate => iso_date_day_number(text).is_some(),
            });
            if !holds {
                let mut diagnostic = Diagnostic::new("check", node.span, check.message);
                diagnostic.code = Some(check.code);
                type_errors.push(diagnostic);
            }
        }
        let value = match rule.action {
            Action::Literal { ty, field: label } => {
                let child = field(id as u32, label)?;
                let number = tree
                    .text(child)
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|n| n.is_finite())
                    .unwrap_or_else(|| {
                        type_errors.push(Diagnostic::new(
                            "type",
                            node.span,
                            "literal must be finite",
                        ));
                        0.0 // Private placeholder; any diagnostic prevents Program publication.
                    });
                Some(HirNode {
                    kind: HirKind::Constant(number),
                    ty,
                    span: node.span,
                })
            }
            Action::Reference { .. } if value_targets.contains_key(&(id as u32)) => {
                let value = value_targets[&(id as u32)];
                Some(HirNode {
                    kind: HirKind::Builtin(value),
                    ty: value.ty,
                    span: node.span,
                })
            }
            Action::Reference { .. } => {
                let (symbol, declaration) = references[&(id as u32)];
                let initializer = lowered[declaration as usize].ok_or_else(|| {
                    Diagnostic::new(
                        "grammar",
                        node.span,
                        "reference declaration has no validated initializer",
                    )
                })?;
                let declaration_node = tree.nodes[declaration as usize];
                let ty = if let Action::Declare {
                    annotation: Some(label),
                    ..
                } = language.rules[declaration_node.rule as usize].action
                {
                    let text = tree.text(field(declaration, label)?).unwrap();
                    language
                        .types
                        .iter()
                        .copied()
                        .find(|t| *t == text || t.rsplit('.').next() == Some(text))
                        .unwrap_or("<error>")
                } else {
                    program.nodes[initializer as usize].ty
                };
                Some(HirNode {
                    kind: HirKind::Reference(symbol),
                    ty,
                    span: node.span,
                })
            }
            Action::Operation { fields, .. }
            | Action::Overload { fields, .. }
            | Action::Call { fields, .. } => {
                let texts = match rule.action {
                    Action::Operation { texts, .. } => texts,
                    _ => &[],
                };
                let mut args = Vec::with_capacity(fields.len());
                for (index, label) in fields.iter().enumerate() {
                    if texts.get(index) == Some(&true) {
                        let child = field(id as u32, label)?;
                        let spelling = tree.text(child).ok_or_else(|| {
                            Diagnostic::new("grammar", node.span, "invalid text argument span")
                        })?;
                        let start = program.text.len() as u32;
                        program.text.push_str(spelling);
                        args.push(program.nodes.len() as u32);
                        program.nodes.push(HirNode {
                            kind: HirKind::Text {
                                start,
                                end: program.text.len() as u32,
                            },
                            ty: TEXT,
                            span: tree.nodes[child as usize].span,
                        });
                        depths.push(1);
                    } else {
                        args.push(value_of(label)?);
                    }
                }
                let operation = match rule.action {
                    Action::Operation { id, .. } => id,
                    Action::Overload { .. } | Action::Call { .. } => {
                        let candidates = if let Action::Overload { candidates, .. } = rule.action {
                            candidates
                        } else {
                            call_targets.get(&(id as u32)).copied().unwrap_or(&[])
                        };
                        let mut selected = None;
                        let mut ambiguous = false;
                        for candidate in candidates {
                            let signature = language
                                .operations
                                .get(*candidate as usize)
                                .ok_or_else(|| {
                                    Diagnostic::new(
                                        "grammar",
                                        node.span,
                                        "unknown overload operation",
                                    )
                                })?;
                            if signature.inputs.len() == args.len()
                                && args
                                    .iter()
                                    .zip(signature.inputs)
                                    .all(|(arg, ty)| program.nodes[*arg as usize].ty == *ty)
                            {
                                if selected.is_some() {
                                    ambiguous = true;
                                    break;
                                }
                                selected = Some(*candidate);
                            }
                        }
                        if selected.is_none() || ambiguous {
                            type_errors.push(Diagnostic::new(
                                "type",
                                node.span,
                                if ambiguous {
                                    "ambiguous exact overload"
                                } else {
                                    "no exact overload"
                                },
                            ));
                            // Preserve a typed error fact so enclosing checks can finish
                            // and diagnostics can be selected in source order. This
                            // provisional HIR is never returned when errors exist.
                            lowered[id] = Some(program.nodes.len() as u32);
                            program.nodes.push(HirNode {
                                kind: HirKind::Constant(0.0),
                                ty: "<error>",
                                span: node.span,
                            });
                            depths.push(1);
                            continue;
                        }
                        selected.unwrap()
                    }
                    _ => unreachable!(),
                };
                let signature = language
                    .operations
                    .get(operation as usize)
                    .ok_or_else(|| Diagnostic::new("grammar", node.span, "unknown operation"))?;
                if args.len() != signature.inputs.len() {
                    type_errors.push(Diagnostic::new(
                        "type",
                        node.span,
                        "operation arity mismatch",
                    ));
                }
                for (argument, ty) in args.iter().zip(signature.inputs) {
                    let child = &program.nodes[*argument as usize];
                    if child.ty != *ty {
                        type_errors.push(Diagnostic::new(
                            "type",
                            child.span,
                            "operation argument type mismatch",
                        ));
                    }
                    depth = depth.max(depths[*argument as usize] + 1);
                }
                let start = program.args.len() as u32;
                program.args.extend(args);
                Some(HirNode {
                    kind: HirKind::Operation {
                        operation,
                        args: start..program.args.len() as u32,
                    },
                    ty: signature.result,
                    span: node.span,
                })
            }
            Action::Declare {
                field: name,
                annotation,
                value,
                ..
            } => {
                let root = value_of(value)?;
                if let Some(annotation) = annotation {
                    let annotation = field(id as u32, annotation)?;
                    let spelling = tree.text(annotation).unwrap();
                    let ty = language
                        .types
                        .iter()
                        .copied()
                        .find(|t| *t == spelling || t.rsplit('.').next() == Some(spelling))
                        .unwrap_or("<error>");
                    if program.nodes[root as usize].ty != ty
                        && ty != "<error>"
                        && program.nodes[root as usize].ty != "<error>"
                    {
                        type_errors.push(Diagnostic::new(
                            "type",
                            program.nodes[root as usize].span,
                            "initializer differs from declared type",
                        ));
                    }
                }
                let symbol = tree.nodes[field(id as u32, name)? as usize].span[0];
                program.statements.push(Statement {
                    value: root,
                    symbol: Some(symbol),
                });
                lowered[id] = Some(root);
                continue;
            }
            Action::Template { .. }
            | Action::Parameter { .. }
            | Action::Alias { .. }
            | Action::Expand { .. } => {
                return Err(Diagnostic::new(
                    "grammar",
                    node.span,
                    "template action outside template compiler",
                ));
            }
            Action::Show { field: label } => {
                let root = value_of(label)?;
                program.statements.push(Statement {
                    value: root,
                    symbol: None,
                });
                lowered[id] = Some(root);
                continue;
            }
            Action::None => {
                let typed: Vec<_> = tree
                    .children(id as u32)
                    .iter()
                    .filter_map(|e| lowered[e.node as usize])
                    .collect();
                if typed.len() == 1 {
                    lowered[id] = Some(typed[0]);
                }
                continue;
            }
        };
        if depth > limits.depth {
            return Err(Diagnostic::new(
                "limit",
                node.span,
                "HIR depth limit exceeded",
            ));
        }
        if let Some(value) = value {
            lowered[id] = Some(program.nodes.len() as u32);
            program.nodes.push(value);
            depths.push(depth);
        }
    }
    if let Some(error) = type_errors.into_iter().min_by_key(|e| e.span[0]) {
        return Err(error);
    }
    Ok(program)
}

impl Program {
    pub fn origins(&self, node: u32) -> &[[u32; 2]] {
        if let Some(range) = self.origin_ranges.get(node as usize) {
            return self
                .origin_spans
                .get(range.start as usize..range.end as usize)
                .unwrap_or(&[]);
        }
        self.nodes
            .get(node as usize)
            .map_or(&[], |n| std::slice::from_ref(&n.span))
    }
}
