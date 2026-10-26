use crate::*;
use std::collections::HashMap;

#[derive(Debug)]
pub enum HirKind<'g> {
    /// A damaged or invalid value. Never admitted to execution.
    Error,
    Constant(f64),
    Reference(u32),
    Builtin(&'g BuiltinValue<'g>),
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
pub struct HirNode<'g> {
    pub kind: HirKind<'g>,
    pub ty: &'g str,
    pub span: [u32; 2],
}
#[derive(Debug)]
pub struct Statement {
    pub value: u32,
    pub symbol: Option<u32>,
}
#[derive(Debug)]
pub struct Binding<'g> {
    pub kind: &'g str,
    pub span: [u32; 2],
    pub symbol: u32,
}
#[derive(Debug)]
pub struct BuiltinBinding<'g> {
    pub kind: &'g str,
    pub name: &'g str,
    pub span: [u32; 2],
}
#[derive(Debug)]
pub struct Program<'g> {
    pub language: &'g Language<'g>,
    pub nodes: Vec<HirNode<'g>>,
    pub args: Vec<u32>,
    pub statements: Vec<Statement>,
    pub bindings: Vec<Binding<'g>>,
    pub builtin_bindings: Vec<BuiltinBinding<'g>>,
    pub origin_spans: Vec<[u32; 2]>,
    pub origin_ranges: Vec<std::ops::Range<u32>>,
    /// Parallel to `origin_spans` in rewrite profiles: true for a span within
    /// a quote template (`nitrogen:rewrite`) rather than the document.
    pub origin_rewrites: Vec<bool>,
    pub text: String,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

pub const TEXT: &str = "Core.Text";
pub const ERROR_TYPE: &str = "<error>";

pub fn compile<'g>(
    language: &'g Language<'g>,
    tree: &SyntaxTree<'g>,
    limits: Limits,
) -> Result<Program<'g>, Vec<Diagnostic>> {
    if let Some(recovery) = tree.recovery() {
        return Err(recovery.diagnostics.clone());
    }
    if language.profile == Profile::SyntaxOnly {
        return Err(vec![Diagnostic::new(
            "capability",
            [0, 0],
            "syntax-only language does not admit semantic checking or execution",
        )]);
    }
    compile_checked(language, tree, limits).map_err(|e| vec![e])
}

/// Name resolution facts for one syntax tree.
#[derive(Default)]
struct Bound<'g> {
    references: HashMap<u32, (u32, u32)>,
    values: HashMap<u32, &'g BuiltinValue<'g>>,
    calls: HashMap<u32, &'g [u16]>,
}

/// One parsed quote template and its holes.
struct Template<'g> {
    tree: SyntaxTree<'g>,
    holes: Vec<(u32, &'g Hole<'g>)>,
    bound: Bound<'g>,
}

struct HelperTemplates<'g> {
    body: Template<'g>,
    arms: Vec<(Template<'g>, Template<'g>)>,
}

/// The rewrite use a template tree is lowered for.
struct Expansion<'a> {
    use_span: [u32; 2],
    name: String,
    holes: &'a HashMap<u32, u32>,
}

type Origin = (bool, [u32; 2]);

fn check_storage<'g>(language: &'g Language<'g>, tree: &SyntaxTree) -> Result<(), Diagnostic> {
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
    Ok(())
}

fn compile_checked<'g>(
    language: &'g Language<'g>,
    tree: &SyntaxTree<'g>,
    limits: Limits,
) -> Result<Program<'g>, Diagnostic> {
    compile_mode(language, tree, limits, false)
}

pub(crate) fn compile_mode<'g>(
    language: &'g Language<'g>,
    tree: &SyntaxTree<'g>,
    limits: Limits,
    recovering: bool,
) -> Result<Program<'g>, Diagnostic> {
    check_storage(language, tree)?;
    if language.rules.iter().any(|r| {
        matches!(
            r.action,
            Action::Template { .. }
                | Action::Parameter { .. }
                | Action::Alias { .. }
                | Action::Expand { .. }
        )
    }) {
        return crate::templates::compile_mode(language, tree, limits, recovering);
    }
    // Every template is validated on each compile, used or not.
    let templates = if language.helpers.is_empty() {
        None
    } else {
        Some(parse_templates(language, limits)?)
    };
    let mut program = Program {
        language,
        nodes: Vec::new(),
        args: Vec::new(),
        statements: Vec::new(),
        bindings: Vec::new(),
        builtin_bindings: Vec::new(),
        origin_spans: Vec::new(),
        origin_ranges: Vec::new(),
        origin_rewrites: Vec::new(),
        text: String::new(),
        diagnostics: tree
            .recovery()
            .map_or_else(Vec::new, |r| r.diagnostics.clone()),
    };
    if tree.nodes.len() as u64 > limits.work {
        return Err(Diagnostic::new(
            "limit",
            [0, 0],
            "compiler work limit exceeded",
        ));
    }
    let mut type_errors = Vec::new();
    let (bound, mut binding_errors) = bind(
        language,
        tree,
        Some(&mut program),
        &mut type_errors,
        recovering,
    )?;
    if !binding_errors.is_empty() && !recovering {
        binding_errors.sort_by_key(|e| e.span[0]);
        return Err(binding_errors.remove(0));
    }
    if recovering {
        program.diagnostics.append(&mut binding_errors);
    }
    let mut lowerer = Lowerer {
        recovering,
        language,
        limits,
        program: &mut program,
        type_errors,
        depths: Vec::new(),
        templates: templates.as_deref(),
        origins: templates.as_ref().map(|_| Vec::new()),
        active: Vec::new(),
        work: tree.nodes.len() as u64,
    };
    lowerer.lower_tree(tree, &bound, None)?;
    let Lowerer {
        type_errors,
        origins,
        ..
    } = lowerer;
    if recovering {
        program.diagnostics.extend(type_errors);
        program.diagnostics.sort_by_key(|e| e.span[0]);
    } else if let Some(error) = type_errors.into_iter().min_by_key(|e| e.span[0]) {
        return Err(error);
    }
    if let Some(origins) = origins {
        reemit(&mut program, origins, limits)?;
    }
    Ok(program)
}

/// Binds a document (`program` given: locals, observations) or a template
/// (no locals: names resolve only against built-ins, as in Nitrogen).
fn bind<'g>(
    language: &'g Language<'g>,
    tree: &SyntaxTree<'g>,
    mut program: Option<&mut Program<'g>>,
    type_errors: &mut Vec<Diagnostic>,
    recovering: bool,
) -> Result<(Bound<'g>, Vec<Diagnostic>), Diagnostic> {
    let mut environment = HashMap::<(&'g str, &str), (u32, u32)>::new();
    let mut bound = Bound::default();
    let mut binding_errors = Vec::new();
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
                if recovering && text.is_empty() {
                    continue;
                }
                // All local kinds precede all builtins, matching Nitrogen Project.Lookup.
                if let Some((kind, binding)) = kinds
                    .iter()
                    .find_map(|kind| environment.get(&(*kind, text)).copied().map(|b| (*kind, b)))
                {
                    bound.references.insert(id as u32, binding);
                    if let Some(program) = program.as_deref_mut() {
                        program.bindings.push(Binding {
                            kind,
                            span: tree.nodes[name as usize].span,
                            symbol: binding.0,
                        });
                    }
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
                        bound.calls.insert(id as u32, builtin.candidates);
                        if let Some(program) = program.as_deref_mut() {
                            program.builtin_bindings.push(BuiltinBinding {
                                kind: builtin.kind,
                                name: builtin.name,
                                span: tree.nodes[name as usize].span,
                            });
                        }
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
                    bound.values.insert(id as u32, value);
                    if let Some(program) = program.as_deref_mut() {
                        program.builtin_bindings.push(BuiltinBinding {
                            kind: value.kind,
                            name: value.name,
                            span: tree.nodes[name as usize].span,
                        });
                    }
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
                if program.is_none() {
                    return Err(Diagnostic::new(
                        "grammar",
                        [0, 0],
                        "templates may not declare names",
                    ));
                }
                let name = field(id as u32, label)?;
                if recovering && tree.text(name).is_none_or(str::is_empty) {
                    continue;
                }
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
                    if matches.len() != 1 && !(recovering && spelling.is_empty()) {
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
            Action::Show { .. } if program.is_none() => {
                return Err(Diagnostic::new(
                    "grammar",
                    [0, 0],
                    "templates may not contain statements",
                ));
            }
            _ => {}
        }
    }
    Ok((bound, binding_errors))
}

fn parse_templates<'g>(
    language: &'g Language<'g>,
    limits: Limits,
) -> Result<Vec<HelperTemplates<'g>>, Diagnostic> {
    let template = |helper: &'g Helper<'g>,
                    text: &str,
                    category: u16,
                    holes: &[&'g Hole<'g>],
                    bind_names: bool|
     -> Result<Template<'g>, Diagnostic> {
        let invalid = |message: &str| {
            Diagnostic::new(
                "grammar",
                [0, 0],
                format!("invalid template in {}: {message}", helper.name),
            )
        };
        let (tree, found) = parse_template(language, text, category, holes, limits)
            .map_err(|e| invalid(&e.message))?;
        let bound = if bind_names {
            let (bound, errors) = bind(language, &tree, None, &mut Vec::new(), false)
                .map_err(|e| invalid(&e.message))?;
            if let Some(error) = errors.first() {
                return Err(invalid(&error.message));
            }
            bound
        } else {
            Bound::default()
        };
        Ok(Template {
            tree,
            holes: found,
            bound,
        })
    };
    let mut result = Vec::with_capacity(language.helpers.len());
    for helper in language.helpers {
        let parameters: Vec<_> = helper.parameters.iter().collect();
        let body = template(helper, helper.body, helper.result, &parameters, true)?;
        let mut arms = Vec::new();
        for arm in helper.arms {
            let selector = helper
                .selector
                .and_then(|s| helper.parameters.get(s as usize))
                .ok_or_else(|| Diagnostic::new("grammar", [0, 0], "invalid match selector"))?;
            let captures: Vec<_> = arm.captures.iter().collect();
            let pattern = template(helper, arm.pattern, selector.category, &captures, false)?;
            // Every declared capture must occur as a parsed hole.
            if captures
                .iter()
                .any(|c| !pattern.holes.iter().any(|(_, h)| std::ptr::eq(*h, *c)))
            {
                return Err(Diagnostic::new(
                    "grammar",
                    [0, 0],
                    format!("unused match capture in {}", helper.name),
                ));
            }
            let visible: Vec<_> = parameters.iter().chain(&captures).copied().collect();
            let result = template(helper, arm.result, helper.result, &visible, true)?;
            arms.push((pattern, result));
        }
        result.push(HelperTemplates { body, arms });
    }
    Ok(result)
}

/// Structural syntax equality: rules, labels and children; token text for lexical nodes.
fn same_syntax(language: &Language, a: &SyntaxTree, x: u32, b: &SyntaxTree, y: u32) -> bool {
    let (p, q) = (a.nodes[x as usize], b.nodes[y as usize]);
    if p.rule != q.rule {
        return false;
    }
    if language.rules[p.rule as usize].lexical {
        return a.text(x) == b.text(y);
    }
    let (left, right) = (a.children(x), b.children(y));
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(l, r)| a.label(l) == b.label(r) && same_syntax(language, a, l.node, b, r.node))
}

/// Matches original caller syntax against a pattern, binding captures.
fn match_pattern<'g>(
    language: &Language,
    pattern: &Template<'g>,
    node: u32,
    tree: &SyntaxTree<'g>,
    target: u32,
    captures: &mut HashMap<&'g str, u32>,
) -> bool {
    if let Some((_, hole)) = pattern.holes.iter().find(|(n, _)| *n == node) {
        let rule = &language.rules[tree.nodes[target as usize].rule as usize];
        if tree.nodes[target as usize].rule != hole.category && rule.owner != Some(hole.category) {
            return false;
        }
        return match captures.get(hole.name) {
            Some(previous) => same_syntax(language, tree, *previous, tree, target),
            None => {
                captures.insert(hole.name, target);
                true
            }
        };
    }
    let (p, q) = (
        pattern.tree.nodes[node as usize],
        tree.nodes[target as usize],
    );
    if p.rule != q.rule {
        return false;
    }
    if language.rules[p.rule as usize].lexical {
        return pattern.tree.text(node) == tree.text(target);
    }
    let (left, right) = (pattern.tree.children(node), tree.children(target));
    left.len() == right.len()
        && left.iter().zip(right).all(|(l, r)| {
            pattern.tree.label(l) == tree.label(r)
                && match_pattern(language, pattern, l.node, tree, r.node, captures)
        })
}

struct Lowerer<'g, 'p> {
    recovering: bool,
    language: &'g Language<'g>,
    limits: Limits,
    program: &'p mut Program<'g>,
    type_errors: Vec<Diagnostic>,
    depths: Vec<u32>,
    templates: Option<&'p [HelperTemplates<'g>]>,
    // Provisional origins per HIR node, rewrite profiles only.
    origins: Option<Vec<Vec<Origin>>>,
    active: Vec<u16>,
    work: u64,
}

impl<'g> Lowerer<'g, '_> {
    fn push(&mut self, node: HirNode<'g>, depth: u32, origin: Origin) -> u32 {
        let id = self.program.nodes.len() as u32;
        self.program.nodes.push(node);
        self.depths.push(depth);
        if let Some(origins) = &mut self.origins {
            origins.push(vec![origin]);
        }
        id
    }

    /// Errors inside an expansion report at the use, naming the rewrite.
    fn error(&mut self, expansion: Option<&Expansion>, mut diagnostic: Diagnostic) {
        if let Some(expansion) = expansion {
            diagnostic.span = expansion.use_span;
            diagnostic.message = format!(
                "in the expansion of {}: {}",
                expansion.name, diagnostic.message
            );
        }
        self.type_errors.push(diagnostic);
    }

    fn lower_tree(
        &mut self,
        tree: &SyntaxTree<'g>,
        bound: &Bound<'g>,
        expansion: Option<&Expansion>,
    ) -> Result<Vec<Option<u32>>, Diagnostic> {
        let language = self.language;
        let field = |id, label| {
            tree.field(id, label)
                .ok_or_else(|| Diagnostic::new("grammar", [0, 0], format!("missing field {label}")))
        };
        let missing_nodes: std::collections::HashSet<_> =
            tree.recovery().map_or_else(Default::default, |r| {
                r.damaged_nodes.iter().copied().collect()
            });
        let mut lowered = vec![None; tree.nodes.len()];
        for (id, node) in tree.nodes.iter().enumerate() {
            if let Some(hir) = expansion.and_then(|e| e.holes.get(&(id as u32))) {
                lowered[id] = Some(*hir);
                continue;
            }
            let rule = &language.rules[node.rule as usize];
            let span = expansion.map_or(node.span, |e| e.use_span);
            let origin = (expansion.is_some(), node.span);
            let value_of = |lowered: &[Option<u32>], label| -> Result<u32, Diagnostic> {
                let child = field(id as u32, label)?;
                lowered[child as usize].ok_or_else(|| {
                    Diagnostic::new("type", node.span, format!("untyped argument {label}"))
                })
            };
            if self.recovering {
                let missing = missing_nodes.contains(&(id as u32));
                let poisoned = tree
                    .children(id as u32)
                    .iter()
                    .filter_map(|e| lowered[e.node as usize])
                    .any(|hir| self.program.nodes[hir as usize].ty == ERROR_TYPE);
                let unresolved = !bound.references.contains_key(&(id as u32))
                    && match rule.action {
                        Action::Reference { .. } => !bound.values.contains_key(&(id as u32)),
                        Action::Call { .. } => !bound.calls.contains_key(&(id as u32)),
                        _ => false,
                    };
                if missing
                    || (!matches!(
                        rule.action,
                        Action::None | Action::Declare { .. } | Action::Show { .. }
                    ) && (poisoned || unresolved))
                {
                    lowered[id] = Some(self.push(
                        HirNode {
                            kind: HirKind::Error,
                            ty: ERROR_TYPE,
                            span,
                        },
                        1,
                        origin,
                    ));
                    continue;
                }
            }
            let mut depth = 1;
            let errors_before = self.type_errors.len();
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
                    diagnostic.code = Some(check.code.into());
                    self.error(expansion, diagnostic);
                }
            }
            if self.recovering && self.type_errors.len() > errors_before {
                lowered[id] = Some(self.push(
                    HirNode {
                        kind: HirKind::Error,
                        ty: ERROR_TYPE,
                        span,
                    },
                    1,
                    origin,
                ));
                continue;
            }
            let value = match rule.action {
                Action::Literal { ty, field: label } => {
                    let child = field(id as u32, label)?;
                    let number = match tree
                        .text(child)
                        .and_then(|s| s.parse::<f64>().ok())
                        .filter(|n| n.is_finite())
                    {
                        Some(number) => number,
                        None => {
                            self.error(
                                expansion,
                                Diagnostic::new("type", node.span, "literal must be finite"),
                            );
                            0.0 // Private placeholder; any diagnostic prevents Program publication.
                        }
                    };
                    Some(HirNode {
                        kind: HirKind::Constant(number),
                        ty,
                        span,
                    })
                }
                Action::Reference { .. } if bound.values.contains_key(&(id as u32)) => {
                    let value = bound.values[&(id as u32)];
                    Some(HirNode {
                        kind: HirKind::Builtin(value),
                        ty: value.ty,
                        span,
                    })
                }
                Action::Reference { .. } => {
                    let (symbol, declaration) = bound.references[&(id as u32)];
                    let initializer = lowered[declaration as usize].ok_or_else(|| {
                        Diagnostic::new(
                            "grammar",
                            node.span,
                            "reference declaration has no validated initializer",
                        )
                    })?;
                    let declaration_node = tree.nodes[declaration as usize];
                    let ty = if self.recovering
                        && self.program.nodes[initializer as usize].ty == ERROR_TYPE
                    {
                        ERROR_TYPE
                    } else if let Action::Declare {
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
                        self.program.nodes[initializer as usize].ty
                    };
                    Some(HirNode {
                        kind: HirKind::Reference(symbol),
                        ty,
                        span,
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
                            let start = self.program.text.len() as u32;
                            self.program.text.push_str(spelling);
                            let end = self.program.text.len() as u32;
                            let child_span = tree.nodes[child as usize].span;
                            args.push(self.push(
                                HirNode {
                                    kind: HirKind::Text { start, end },
                                    ty: TEXT,
                                    span: expansion.map_or(child_span, |e| e.use_span),
                                },
                                1,
                                (expansion.is_some(), child_span),
                            ));
                        } else {
                            args.push(value_of(&lowered, label)?);
                        }
                    }
                    let operation = match rule.action {
                        Action::Operation { id, .. } => id,
                        Action::Overload { .. } | Action::Call { .. } => {
                            let candidates =
                                if let Action::Overload { candidates, .. } = rule.action {
                                    candidates
                                } else {
                                    bound.calls.get(&(id as u32)).copied().unwrap_or(&[])
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
                                    && args.iter().zip(signature.inputs).all(|(arg, ty)| {
                                        self.program.nodes[*arg as usize].ty == *ty
                                    })
                                {
                                    if selected.is_some() {
                                        ambiguous = true;
                                        break;
                                    }
                                    selected = Some(*candidate);
                                }
                            }
                            if selected.is_none() || ambiguous {
                                self.error(
                                    expansion,
                                    Diagnostic::new(
                                        "type",
                                        node.span,
                                        if ambiguous {
                                            "ambiguous exact overload"
                                        } else {
                                            "no exact overload"
                                        },
                                    ),
                                );
                                // Preserve a typed error fact so enclosing checks can finish
                                // and diagnostics can be selected in source order. This
                                // provisional HIR is never returned when errors exist.
                                lowered[id] = Some(self.push(
                                    HirNode {
                                        kind: if self.recovering {
                                            HirKind::Error
                                        } else {
                                            HirKind::Constant(0.0)
                                        },
                                        ty: ERROR_TYPE,
                                        span,
                                    },
                                    1,
                                    origin,
                                ));
                                continue;
                            }
                            selected.unwrap()
                        }
                        _ => unreachable!(),
                    };
                    let signature =
                        language.operations.get(operation as usize).ok_or_else(|| {
                            Diagnostic::new("grammar", node.span, "unknown operation")
                        })?;
                    if args.len() != signature.inputs.len() {
                        self.error(
                            expansion,
                            Diagnostic::new("type", node.span, "operation arity mismatch"),
                        );
                    }
                    let mut mismatched = args.len() != signature.inputs.len();
                    for (argument, ty) in args.iter().zip(signature.inputs) {
                        let child = &self.program.nodes[*argument as usize];
                        if child.ty != *ty {
                            mismatched = true;
                            let child_span = child.span;
                            self.error(
                                expansion,
                                Diagnostic::new(
                                    "type",
                                    child_span,
                                    "operation argument type mismatch",
                                ),
                            );
                        }
                        depth = depth.max(self.depths[*argument as usize] + 1);
                    }
                    if self.recovering && mismatched {
                        lowered[id] = Some(self.push(
                            HirNode {
                                kind: HirKind::Error,
                                ty: ERROR_TYPE,
                                span,
                            },
                            1,
                            origin,
                        ));
                        continue;
                    }
                    let start = self.program.args.len() as u32;
                    self.program.args.extend(args);
                    Some(HirNode {
                        kind: HirKind::Operation {
                            operation,
                            args: start..self.program.args.len() as u32,
                        },
                        ty: signature.result,
                        span,
                    })
                }
                Action::Rewrite {
                    helper,
                    fields,
                    name,
                } => {
                    lowered[id] = Some(
                        self.expand(tree, id as u32, &lowered, helper, fields, name, expansion)?,
                    );
                    continue;
                }
                Action::Declare {
                    field: name,
                    annotation,
                    value,
                    ..
                } => {
                    let root = value_of(&lowered, value)?;
                    if let Some(annotation) = annotation {
                        let annotation = field(id as u32, annotation)?;
                        let spelling = tree.text(annotation).unwrap();
                        let ty = language
                            .types
                            .iter()
                            .copied()
                            .find(|t| *t == spelling || t.rsplit('.').next() == Some(spelling))
                            .unwrap_or("<error>");
                        let actual = self.program.nodes[root as usize].ty;
                        if actual != ty && ty != "<error>" && actual != "<error>" {
                            let root_span = self.program.nodes[root as usize].span;
                            self.type_errors.push(Diagnostic::new(
                                "type",
                                root_span,
                                "initializer differs from declared type",
                            ));
                        }
                    }
                    let name = field(id as u32, name)?;
                    let symbol = if self.recovering && tree.text(name).is_none_or(str::is_empty) {
                        None
                    } else {
                        Some(tree.nodes[name as usize].span[0])
                    };
                    self.program.statements.push(Statement {
                        value: root,
                        symbol,
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
                    let root = value_of(&lowered, label)?;
                    self.program.statements.push(Statement {
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
            if depth > self.limits.depth {
                return Err(Diagnostic::new(
                    "limit",
                    node.span,
                    "HIR depth limit exceeded",
                ));
            }
            if let Some(value) = value {
                let value = if self.recovering && self.type_errors.len() > errors_before {
                    HirNode {
                        kind: HirKind::Error,
                        ty: ERROR_TYPE,
                        span,
                    }
                } else {
                    value
                };
                lowered[id] = Some(self.push(value, depth, origin));
            }
        }
        Ok(lowered)
    }

    /// Lowers a rewrite use as its selected template, holes filled by the
    /// caller's lowered syntax. The caller's own HIR stays unreferenced unless
    /// a hole uses it; re-emission drops unreachable nodes.
    #[allow(clippy::too_many_arguments)]
    fn expand(
        &mut self,
        tree: &SyntaxTree<'g>,
        id: u32,
        lowered: &[Option<u32>],
        helper_index: u16,
        fields: &'g [&'g str],
        name: &'g str,
        outer: Option<&Expansion>,
    ) -> Result<u32, Diagnostic> {
        let language = self.language;
        let node = tree.nodes[id as usize];
        let use_span = outer.map_or(node.span, |e| e.use_span);
        let display = outer.map_or_else(
            || name.to_owned(),
            |e| format!("{}: in the expansion of {name}", e.name),
        );
        let helper = language
            .helpers
            .get(helper_index as usize)
            .ok_or_else(|| Diagnostic::new("grammar", node.span, "unknown rewrite helper"))?;
        let templates = &self.templates.ok_or_else(|| {
            Diagnostic::new("grammar", node.span, "rewrite outside a helper profile")
        })?[helper_index as usize];
        let args = fields
            .iter()
            .map(|label| {
                tree.field(id, label).ok_or_else(|| {
                    Diagnostic::new("grammar", node.span, format!("missing field {label}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let placeholder = |this: &mut Self, message: &str| {
            this.type_errors
                .push(Diagnostic::new("type", use_span, message.to_owned()));
            this.push(
                HirNode {
                    kind: if this.recovering {
                        HirKind::Error
                    } else {
                        HirKind::Constant(0.0)
                    },
                    ty: ERROR_TYPE,
                    span: use_span,
                },
                1,
                (outer.is_some(), node.span),
            )
        };
        if self.active.contains(&helper_index) {
            return Ok(placeholder(
                self,
                "a rewrite that reaches itself prevents lowering",
            ));
        }
        if self.active.len() as u32 >= self.limits.depth {
            return Err(Diagnostic::new(
                "limit",
                use_span,
                "rewrite depth limit exceeded",
            ));
        }
        let mut captures = HashMap::new();
        let mut selected = &templates.body;
        if let Some(selector) = helper.selector {
            let subject = args[selector as usize];
            // Ordered arms; the fallback (the helper body) binds no captures.
            let mut matched = false;
            for (pattern, result) in &templates.arms {
                captures.clear();
                if match_pattern(
                    language,
                    pattern,
                    pattern.tree.root,
                    tree,
                    subject,
                    &mut captures,
                ) {
                    selected = result;
                    matched = true;
                    break;
                }
            }
            if !matched {
                captures.clear();
            }
        }
        let mut holes = HashMap::with_capacity(selected.holes.len());
        for (template_node, hole) in &selected.holes {
            let caller = helper
                .parameters
                .iter()
                .position(|p| std::ptr::eq(p, *hole))
                .map(|i| args[i])
                .or_else(|| captures.get(hole.name).copied())
                .ok_or_else(|| Diagnostic::new("grammar", node.span, "unbound template hole"))?;
            match lowered[caller as usize] {
                Some(hir) => {
                    holes.insert(*template_node, hir);
                }
                None => return Ok(placeholder(self, "untyped rewrite argument")),
            }
        }
        self.work += selected.tree.nodes.len() as u64;
        if self.work > self.limits.work {
            return Err(Diagnostic::new(
                "limit",
                use_span,
                "compiler work limit exceeded",
            ));
        }
        let expansion = Expansion {
            use_span,
            name: display,
            holes: &holes,
        };
        let first_new = self.program.nodes.len() as u32;
        self.active.push(helper_index);
        let result = self.lower_tree(&selected.tree, &selected.bound, Some(&expansion));
        self.active.pop();
        let result = result?;
        let root = selected.tree.root;
        let Some(hir) = result[root as usize] else {
            return Ok(placeholder(self, "untyped rewrite result"));
        };
        // A single-hole template is the caller's HIR itself, with its own origins.
        if holes.contains_key(&root) || self.origins.is_none() {
            return Ok(hir);
        }
        // Otherwise the result carries the use first. A caller node reached
        // through a wrapper (`<[ ($X) ]>`) is copied, leaving its other uses alone.
        let hir = if hir < first_new {
            let copy = &self.program.nodes[hir as usize];
            let kind = match &copy.kind {
                HirKind::Error => HirKind::Error,
                HirKind::Constant(n) => HirKind::Constant(*n),
                HirKind::Reference(s) => HirKind::Reference(*s),
                HirKind::Builtin(b) => HirKind::Builtin(b),
                HirKind::Text { start, end } => HirKind::Text {
                    start: *start,
                    end: *end,
                },
                HirKind::Operation { operation, args } => HirKind::Operation {
                    operation: *operation,
                    args: args.clone(),
                },
            };
            let node = HirNode {
                kind,
                ty: copy.ty,
                span: copy.span,
            };
            let depth = self.depths[hir as usize];
            let previous = self.origins.as_ref().unwrap()[hir as usize].clone();
            let id = self.push(node, depth, previous[0]);
            self.origins.as_mut().unwrap()[id as usize] = previous;
            id
        } else {
            hir
        };
        let list = &mut self.origins.as_mut().unwrap()[hir as usize];
        let origin = (outer.is_some(), node.span);
        list.retain(|o| *o != origin);
        list.insert(0, origin);
        Ok(hir)
    }
}

/// Emits only HIR reachable from statements, in evaluation order. A node
/// reached twice is emitted twice, so repeated holes repeat their effects and
/// discarded caller syntax never runs, as Nitrogen evaluates HIR trees.
fn reemit<'g>(
    program: &mut Program<'g>,
    origins: Vec<Vec<Origin>>,
    limits: Limits,
) -> Result<(), Diagnostic> {
    let nodes = std::mem::take(&mut program.nodes);
    let args = std::mem::take(&mut program.args);
    fn emit<'g>(
        id: u32,
        nodes: &[HirNode<'g>],
        args: &[u32],
        origins: &[Vec<Origin>],
        program: &mut Program<'g>,
        limits: Limits,
    ) -> Result<u32, Diagnostic> {
        let node = &nodes[id as usize];
        let kind = match &node.kind {
            HirKind::Operation {
                operation,
                args: range,
            } => {
                let mut emitted = Vec::with_capacity(range.len());
                for arg in &args[range.start as usize..range.end as usize] {
                    emitted.push(emit(*arg, nodes, args, origins, program, limits)?);
                }
                let start = program.args.len() as u32;
                program.args.extend(emitted);
                HirKind::Operation {
                    operation: *operation,
                    args: start..program.args.len() as u32,
                }
            }
            HirKind::Error => HirKind::Error,
            HirKind::Constant(n) => HirKind::Constant(*n),
            HirKind::Reference(s) => HirKind::Reference(*s),
            HirKind::Builtin(b) => HirKind::Builtin(b),
            HirKind::Text { start, end } => HirKind::Text {
                start: *start,
                end: *end,
            },
        };
        if program.nodes.len() as u64 >= limits.work {
            return Err(Diagnostic::new(
                "limit",
                node.span,
                "rewrite expansion work limit exceeded",
            ));
        }
        let start = program.origin_spans.len() as u32;
        for (in_template, span) in &origins[id as usize] {
            program.origin_spans.push(*span);
            program.origin_rewrites.push(*in_template);
        }
        program
            .origin_ranges
            .push(start..program.origin_spans.len() as u32);
        program.nodes.push(HirNode {
            kind,
            ty: node.ty,
            span: node.span,
        });
        Ok(program.nodes.len() as u32 - 1)
    }
    let mut statements = std::mem::take(&mut program.statements);
    for statement in &mut statements {
        statement.value = emit(statement.value, &nodes, &args, &origins, program, limits)?;
    }
    program.statements = statements;
    Ok(())
}

impl<'g> Program<'g> {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
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

    /// Whether each origin of `node` lies in a quote template (`nitrogen:rewrite`).
    pub fn origin_in_rewrite(&self, node: u32) -> &[bool] {
        self.origin_ranges
            .get(node as usize)
            .and_then(|r| self.origin_rewrites.get(r.start as usize..r.end as usize))
            .unwrap_or(&[])
    }
}
