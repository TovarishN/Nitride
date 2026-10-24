//! Bounded document-local declarative templates. No host handler runs here.
use crate::*;
use std::{collections::HashMap, rc::Rc};

type Id = u32;
#[derive(Clone)]
enum RawKind {
    Constant(f64),
    Operation(u16, Vec<Rc<Raw>>),
}
struct Raw {
    kind: RawKind,
    ty: &'static str,
    origins: Vec<[u32; 2]>,
}
struct Compiler<'a> {
    language: &'static Language,
    tree: &'a SyntaxTree,
    limits: Limits,
    work: u64,
    owner: Vec<Option<Id>>,
    parameters: Vec<Vec<Id>>,
    bound: HashMap<Id, Id>,
    parameter_types: Vec<Option<&'static str>>,
    states: Vec<u8>,
    types: Vec<Option<&'static str>>,
    errors: Vec<Diagnostic>,
    program: Program,
}
impl<'a> Compiler<'a> {
    fn action(&self, id: Id) -> &Action {
        &self.language.rules[self.tree.nodes[id as usize].rule as usize].action
    }
    fn span(&self, id: Id) -> [u32; 2] {
        self.tree.nodes[id as usize].span
    }
    fn field(&self, id: Id, label: &str) -> Result<Id, Diagnostic> {
        self.tree.field(id, label).ok_or_else(|| {
            Diagnostic::new("grammar", self.span(id), format!("missing field {label}"))
        })
    }
    fn fields(&self, id: Id, label: &str) -> Vec<Id> {
        self.tree
            .children(id)
            .iter()
            .filter(|e| self.tree.label(e) == label)
            .map(|e| e.node)
            .collect()
    }
    fn text(&self, id: Id) -> &'a str {
        self.tree.text(id).unwrap()
    }
    fn guard(&mut self, id: Id, depth: u32) -> Result<(), Diagnostic> {
        self.work += 1;
        if depth > self.limits.depth.min(128) || self.work > self.limits.work {
            return Err(Diagnostic::new(
                "limit",
                self.span(id),
                "template depth or work limit exceeded",
            ));
        }
        Ok(())
    }
    fn name(&self, id: Id) -> Result<Id, Diagnostic> {
        let label = match self.action(id) {
            Action::Template { field, .. }
            | Action::Parameter { field, .. }
            | Action::Alias { field, .. } => *field,
            _ => {
                return Err(Diagnostic::new(
                    "grammar",
                    self.span(id),
                    "invalid declaration",
                ));
            }
        };
        self.field(id, label)
    }
    fn bind(&mut self) -> Result<(), Diagnostic> {
        let mut stack = vec![(self.tree.root, None)];
        let mut visited = vec![false; self.tree.nodes.len()];
        while let Some((id, owner)) = stack.pop() {
            self.guard(id, 0)?;
            if std::mem::replace(&mut visited[id as usize], true) {
                return Err(Diagnostic::new(
                    "grammar",
                    self.span(id),
                    "shared syntax child in template tree",
                ));
            }
            let next = if matches!(self.action(id), Action::Template { .. }) {
                if owner.is_some() {
                    return Err(Diagnostic::new(
                        "grammar",
                        self.span(id),
                        "nested template definitions are not admitted",
                    ));
                }
                Some(id)
            } else {
                owner
            };
            self.owner[id as usize] = next;
            stack.extend(self.tree.children(id).iter().map(|e| (e.node, next)));
        }
        if visited.iter().any(|seen| !seen) {
            return Err(Diagnostic::new(
                "grammar",
                [0, 0],
                "detached syntax in template tree",
            ));
        }
        let mut globals = HashMap::<(&str, &str), Vec<Id>>::new();
        let mut locals = HashMap::<(Id, &str, &str), Vec<Id>>::new();
        for id in 0..self.tree.nodes.len() as Id {
            match *self.action(id) {
                Action::Template {
                    kind, parameters, ..
                } => {
                    globals
                        .entry((kind, self.text(self.name(id)?)))
                        .or_default()
                        .push(id);
                    let params = self.fields(id, parameters);
                    for parameter in &params {
                        if !matches!(self.action(*parameter), Action::Parameter { .. }) {
                            return Err(Diagnostic::new(
                                "grammar",
                                self.span(*parameter),
                                "template list must contain typed parameters",
                            ));
                        }
                    }
                    self.parameters[id as usize] = params;
                }
                Action::Alias { kind, .. } => {
                    globals
                        .entry((kind, self.text(self.name(id)?)))
                        .or_default()
                        .push(id);
                }
                Action::Parameter {
                    kind, annotation, ..
                } => {
                    let owner = self.owner[id as usize].ok_or_else(|| {
                        Diagnostic::new("grammar", self.span(id), "parameter outside template")
                    })?;
                    locals
                        .entry((owner, kind, self.text(self.name(id)?)))
                        .or_default()
                        .push(id);
                    let annotation = self.field(id, annotation)?;
                    let name = self.text(annotation);
                    let matches = self
                        .language
                        .types
                        .iter()
                        .copied()
                        .filter(|t| *t == name || t.rsplit('.').next() == Some(name))
                        .collect::<Vec<_>>();
                    if matches.len() == 1 {
                        self.parameter_types[id as usize] = Some(matches[0]);
                    } else {
                        self.errors.push(Diagnostic::new(
                            "type",
                            self.span(annotation),
                            "unknown or ambiguous parameter type",
                        ));
                    }
                }
                Action::Call { .. } | Action::Declare { .. } | Action::Overload { .. } => {
                    return Err(Diagnostic::new(
                        "grammar",
                        self.span(id),
                        "unsupported action in template profile",
                    ));
                }
                _ => {}
            }
        }
        // Declaration maps borrow syntax text, not mutable compiler storage.
        let mut binding_errors = Vec::new();
        for declarations in globals.values().chain(locals.values()) {
            for declaration in declarations.iter().skip(1) {
                binding_errors.push(Diagnostic::new(
                    "binding",
                    self.span(self.name(*declaration)?),
                    "duplicate declaration",
                ));
            }
        }
        let mut bound = HashMap::new();
        let mut observations = Vec::new();
        for id in 0..self.tree.nodes.len() as Id {
            if let Action::Reference { kinds, field, .. } | Action::Expand { kinds, field, .. } =
                *self.action(id)
            {
                let name = self.field(id, field)?;
                let text = self.text(name);
                let found = self.owner[id as usize]
                    .and_then(|owner| {
                        kinds.iter().find_map(|kind| {
                            locals.get(&(owner, *kind, text)).map(|ids| (*kind, ids))
                        })
                    })
                    .or_else(|| {
                        kinds
                            .iter()
                            .find_map(|kind| globals.get(&(*kind, text)).map(|ids| (*kind, ids)))
                    });
                if let Some((kind, ids)) = found.filter(|(_, ids)| ids.len() == 1) {
                    bound.insert(id, ids[0]);
                    observations.push(Binding {
                        kind,
                        span: self.span(name),
                        symbol: self.span(self.name(ids[0])?)[0],
                    });
                } else {
                    binding_errors.push(Diagnostic::new(
                        "binding",
                        self.span(name),
                        "unresolved or ambiguous name",
                    ));
                }
            }
        }
        self.bound = bound;
        self.program.bindings = observations;
        if !binding_errors.is_empty() {
            binding_errors.sort_by_key(|d| d.span[0]);
            return Err(binding_errors.remove(0));
        }
        Ok(())
    }
    fn ty(&mut self, id: Id, depth: u32) -> Result<Option<&'static str>, Diagnostic> {
        self.guard(id, depth)?;
        if self.states[id as usize] != 0 {
            return Ok(self.types[id as usize]);
        }
        self.states[id as usize] = 1;
        let ty = match *self.action(id) {
            Action::Literal { ty, field } => {
                let number = self.field(id, field)?;
                if self
                    .text(number)
                    .parse::<f64>()
                    .ok()
                    .is_none_or(|n| !n.is_finite())
                {
                    self.errors.push(Diagnostic::new(
                        "type",
                        self.span(id),
                        "literal must be finite",
                    ));
                }
                Some(ty)
            }
            Action::Reference { .. } => self.parameter_types[self.bound[&id] as usize],
            Action::Operation {
                id: operation,
                fields,
                ..
            } => {
                let signature = *self
                    .language
                    .operations
                    .get(operation as usize)
                    .ok_or_else(|| {
                        Diagnostic::new("grammar", self.span(id), "unknown operation")
                    })?;
                if fields.len() != signature.inputs.len() {
                    return Err(Diagnostic::new(
                        "grammar",
                        self.span(id),
                        "operation arity mismatch",
                    ));
                }
                for (field, expected) in fields.iter().zip(signature.inputs) {
                    let arg = self.field(id, field)?;
                    if self
                        .ty(arg, depth + 1)?
                        .is_some_and(|actual| actual != *expected)
                    {
                        self.errors.push(Diagnostic::new(
                            "type",
                            self.span(arg),
                            "operation argument type mismatch",
                        ));
                    }
                }
                Some(signature.result)
            }
            Action::Expand {
                field, arguments, ..
            } => {
                let declaration = self.bound[&id];
                let args = self.fields(id, arguments);
                let mut actuals = Vec::new();
                for arg in &args {
                    actuals.push(self.ty(*arg, depth + 1)?);
                }
                if let Action::Template { body, .. } = *self.action(declaration) {
                    let parameters = self.parameters[declaration as usize].clone();
                    if args.len() != parameters.len() {
                        self.errors.push(Diagnostic::new(
                            "type",
                            self.span(self.field(id, field)?),
                            "template argument count mismatch",
                        ));
                    } else {
                        for ((arg, actual), parameter) in args.iter().zip(actuals).zip(parameters) {
                            if let (Some(actual), Some(expected)) =
                                (actual, self.parameter_types[parameter as usize])
                            {
                                if actual != expected {
                                    self.errors.push(Diagnostic::new(
                                        "type",
                                        self.span(*arg),
                                        "template argument type mismatch",
                                    ));
                                }
                            }
                        }
                    }
                    self.ty(self.field(declaration, body)?, depth + 1)?
                } else {
                    self.errors.push(Diagnostic::new(
                        "type",
                        self.span(self.field(id, field)?),
                        "selected symbol is not a template",
                    ));
                    None
                }
            }
            Action::Show { field } => self.ty(self.field(id, field)?, depth + 1)?,
            Action::None => {
                let children = self
                    .tree
                    .children(id)
                    .iter()
                    .map(|e| e.node)
                    .collect::<Vec<_>>();
                let mut found = None;
                for child in children {
                    if let Some(ty) = self.ty(child, depth + 1)? {
                        found = Some(ty);
                        break;
                    }
                }
                found
            }
            _ => None,
        };
        self.types[id as usize] = ty;
        self.states[id as usize] = 2;
        Ok(ty)
    }
    fn lower(
        &mut self,
        id: Id,
        env: &HashMap<Id, Rc<Raw>>,
        active: &mut Vec<Id>,
        depth: u32,
    ) -> Result<Rc<Raw>, Diagnostic> {
        self.guard(id, depth)?;
        let own = self.span(id);
        let raw = match *self.action(id) {
            Action::Literal { ty, field } => Raw {
                kind: RawKind::Constant(
                    self.text(self.field(id, field)?)
                        .parse()
                        .map_err(|_| Diagnostic::new("type", own, "invalid numeric literal"))?,
                ),
                ty,
                origins: vec![own],
            },
            Action::Reference { .. } => {
                let arg = env.get(&self.bound[&id]).ok_or_else(|| {
                    Diagnostic::new("lowering", own, "parameter outside expansion")
                })?;
                Raw {
                    kind: arg.kind.clone(),
                    ty: arg.ty,
                    origins: prepend(own, &arg.origins),
                }
            }
            Action::Operation {
                id: operation,
                fields,
                ..
            } => {
                let mut args = Vec::new();
                for field in fields {
                    let child = self.field(id, field)?;
                    args.push(self.lower(child, env, active, depth + 1)?);
                }
                Raw {
                    kind: RawKind::Operation(operation, args),
                    ty: self.language.operations[operation as usize].result,
                    origins: vec![own],
                }
            }
            Action::Expand {
                field, arguments, ..
            } => {
                let declaration = self.bound[&id];
                if active.contains(&declaration) {
                    return Err(Diagnostic::new(
                        "lowering",
                        self.span(self.field(id, field)?),
                        "cyclic template expansion",
                    ));
                }
                let body = if let Action::Template { body, .. } = *self.action(declaration) {
                    self.field(declaration, body)?
                } else {
                    return Err(Diagnostic::new("type", own, "not a template"));
                };
                active.push(declaration);
                let parameters = self.parameters[declaration as usize].clone();
                let args = self.fields(id, arguments);
                if parameters.len() != args.len() {
                    return Err(Diagnostic::new(
                        "type",
                        own,
                        "template argument count mismatch",
                    ));
                }
                let mut substitutions = HashMap::new();
                for (parameter, arg) in parameters.into_iter().zip(args) {
                    substitutions.insert(parameter, self.lower(arg, env, active, depth + 1)?);
                }
                let result = self.lower(body, &substitutions, active, depth + 1)?;
                active.pop();
                Raw {
                    kind: result.kind.clone(),
                    ty: result.ty,
                    origins: prepend(own, &result.origins),
                }
            }
            Action::None => {
                if let [edge] = self.tree.children(id) {
                    return self.lower(edge.node, env, active, depth + 1);
                }
                for edge in self.tree.children(id) {
                    if self.types[edge.node as usize].is_some() {
                        return self.lower(edge.node, env, active, depth + 1);
                    }
                }
                return Err(Diagnostic::new(
                    "lowering",
                    own,
                    "no supported declarative lowering",
                ));
            }
            _ => {
                return Err(Diagnostic::new(
                    "grammar",
                    own,
                    "unexpected template expression",
                ));
            }
        };
        Ok(Rc::new(raw))
    }
    fn flatten(&mut self, raw: &Raw, depth: u32) -> Result<Id, Diagnostic> {
        // Origin zero remains the primary diagnostic location.
        let site = raw.origins[0];
        self.work += 1;
        if depth > self.limits.depth.min(128)
            || self.work > self.limits.work
            || self.program.nodes.len() >= u32::MAX as usize
        {
            return Err(Diagnostic::new(
                "limit",
                site,
                "expanded HIR depth or work limit exceeded",
            ));
        }
        let kind = match &raw.kind {
            RawKind::Constant(n) => HirKind::Constant(*n),
            RawKind::Operation(operation, args) => {
                let mut ids = Vec::new();
                for arg in args {
                    ids.push(self.flatten(arg, depth + 1)?);
                }
                let start = self.program.args.len() as u32;
                self.program.args.extend(ids);
                HirKind::Operation {
                    operation: *operation,
                    args: start..self.program.args.len() as u32,
                }
            }
        };
        let id = self.program.nodes.len() as u32;
        let start = self.program.origin_spans.len() as u32;
        self.program.origin_spans.extend_from_slice(&raw.origins);
        self.program
            .origin_ranges
            .push(start..self.program.origin_spans.len() as u32);
        self.program.nodes.push(HirNode {
            kind,
            ty: raw.ty,
            span: site,
        });
        Ok(id)
    }
}
fn prepend(span: [u32; 2], origins: &[[u32; 2]]) -> Vec<[u32; 2]> {
    let mut result = vec![span];
    for origin in origins {
        if !result.contains(origin) {
            result.push(*origin);
        }
    }
    result
}

pub(crate) fn compile(
    language: &'static Language,
    tree: &SyntaxTree,
    limits: Limits,
) -> Result<Program, Diagnostic> {
    if tree.nodes.len() as u64 > limits.work {
        return Err(Diagnostic::new(
            "limit",
            [0, 0],
            "compiler work limit exceeded",
        ));
    }
    let count = tree.nodes.len();
    let mut c = Compiler {
        language,
        tree,
        limits,
        work: 0,
        owner: vec![None; count],
        parameters: vec![Vec::new(); count],
        bound: HashMap::new(),
        parameter_types: vec![None; count],
        states: vec![0; count],
        types: vec![None; count],
        errors: Vec::new(),
        program: Program {
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
        },
    };
    c.bind()?;
    // Cyclic queries can temporarily lack a result even when an operation in
    // the component fixes its nominal type. Revisit those dependencies until
    // facts stabilize, then use constraints from that final pass. Pure cycles
    // retain unknown results. Every visit consumes the shared work budget.
    let declaration_errors = c.errors.clone();
    loop {
        let previous = c.types.clone();
        c.states.fill(0);
        c.errors = declaration_errors.clone();
        for id in 0..count as Id {
            c.ty(id, 0)?;
        }
        if c.types == previous {
            break;
        }
    }
    if !c.errors.is_empty() {
        c.errors.sort_by_key(|d| d.span[0]);
        return Err(c.errors.remove(0));
    }
    let roots = (0..count as Id)
        .filter(|id| matches!(c.action(*id), Action::Show { .. }))
        .collect::<Vec<_>>();
    for root in roots {
        let field = if let Action::Show { field } = *c.action(root) {
            field
        } else {
            unreachable!()
        };
        let body = c.field(root, field)?;
        if c.types[body as usize].is_none() {
            return Err(Diagnostic::new(
                "lowering",
                c.span(root),
                "selected syntax has no supported declarative lowering",
            ));
        }
        let raw = c.lower(body, &HashMap::new(), &mut Vec::new(), 0)?;
        let value = c.flatten(&raw, 0)?;
        c.program.statements.push(Statement {
            value,
            symbol: None,
        });
    }
    Ok(c.program)
}
