use crate::*;

#[derive(Debug)]
pub enum HirKind {
    Constant(f64),
    Reference(u32),
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
pub struct Program {
    pub language: &'static Language,
    pub nodes: Vec<HirNode>,
    pub args: Vec<u32>,
    pub statements: Vec<Statement>,
    pub bindings: Vec<Binding>,
}

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
        if node.span[0] > node.span[1]
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
    let mut program = Program {
        language,
        nodes: Vec::new(),
        args: Vec::new(),
        statements: Vec::new(),
        bindings: Vec::new(),
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
            } => {
                let name = field(id as u32, label)?;
                let text = tree.text(name).ok_or_else(|| {
                    Diagnostic::new("grammar", node.span, "invalid identifier span")
                })?;
                let (kind, binding) = kinds
                    .iter()
                    .find_map(|kind| {
                        environment
                            .get(&(*kind, text))
                            .copied()
                            .map(|binding| (*kind, binding))
                    })
                    .ok_or_else(|| {
                        Diagnostic::new(
                            "binding",
                            tree.nodes[name as usize].span,
                            format!("unresolved name {text}"),
                        )
                    })?;
                references.insert(id as u32, binding);
                program.bindings.push(Binding {
                    kind,
                    span: tree.nodes[name as usize].span,
                    symbol: binding.0,
                });
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
            Action::Operation { fields, .. } | Action::Overload { fields, .. } => {
                let args = fields
                    .iter()
                    .map(|f| value_of(*f))
                    .collect::<Result<Vec<_>, _>>()?;
                let operation = match rule.action {
                    Action::Operation { id, .. } => id,
                    Action::Overload { candidates, .. } => {
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
