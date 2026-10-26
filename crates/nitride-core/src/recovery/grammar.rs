use crate::*;
use std::collections::HashMap;

pub(super) type Code = u32;
pub(super) enum Instruction<'g> {
    Terminal(Expected<'g>, &'g Expr<'g>),
    Predicate(&'g Expr<'g>),
    Ref(u16),
    Sequence(Vec<Code>),
    Choice(Vec<Code>),
    Label(&'g str, Code),
    Repeat {
        inner: Code,
        min: u32,
        max: u32,
    },
    Pratt {
        prefixes: &'g [u16],
        infixes: &'g [u16],
    },
}

pub(super) struct Grammar<'g> {
    pub code: Vec<Instruction<'g>>,
    pub rules: Vec<Code>,
    pub expressions: Vec<&'g Expr<'g>>,
    pub nullable: Vec<bool>,
}

impl<'g> Grammar<'g> {
    pub fn new(language: &'g Language<'g>) -> Self {
        let mut graph = Self {
            code: vec![],
            rules: vec![],
            expressions: vec![],
            nullable: vec![],
        };
        // Explicit postorder avoids recursion through large expression arrays.
        let mut known = HashMap::new();
        for rule in language.rules {
            let mut stack = vec![(&rule.expr, false)];
            while let Some((expr, visited)) = stack.pop() {
                let key = std::ptr::from_ref(expr) as usize;
                if known.contains_key(&key) {
                    continue;
                }
                if !visited {
                    stack.push((expr, true));
                    match expr {
                        Expr::Sequence(parts) | Expr::Choice(parts) => {
                            stack.extend(parts.iter().rev().map(|p| (p, false)))
                        }
                        Expr::Label(_, inner) | Expr::Repeat { inner, .. } => {
                            stack.push((inner, false))
                        }
                        _ => {}
                    }
                    continue;
                }
                let get = |e: &'g Expr<'g>| known[&(std::ptr::from_ref(e) as usize)];
                let instruction = match expr {
                    Expr::Ref(id) => Instruction::Ref(*id),
                    Expr::Sequence(parts) => Instruction::Sequence(parts.iter().map(get).collect()),
                    Expr::Choice(parts) => Instruction::Choice(parts.iter().map(get).collect()),
                    Expr::Label(label, inner) => Instruction::Label(label, get(inner)),
                    Expr::Repeat { inner, min, max } => Instruction::Repeat {
                        inner: get(inner),
                        min: *min,
                        max: *max,
                    },
                    Expr::Pratt {
                        prefixes, infixes, ..
                    } => Instruction::Pratt { prefixes, infixes },
                    Expr::Predicate { .. } => Instruction::Predicate(expr),
                    Expr::Literal(text) => Instruction::Terminal(Expected::Literal(text), expr),
                    _ => Instruction::Terminal(Expected::Character, expr),
                };
                known.insert(key, graph.code.len() as Code);
                graph.code.push(instruction);
                graph.expressions.push(expr);
            }
            graph
                .rules
                .push(known[&(std::ptr::from_ref(&rule.expr) as usize)]);
        }
        graph.nullable = vec![false; graph.code.len()];
        loop {
            let mut changed = false;
            for (id, instruction) in graph.code.iter().enumerate() {
                let yes = match instruction {
                    Instruction::Terminal(Expected::Literal(""), _) | Instruction::Predicate(_) => {
                        true
                    }
                    Instruction::Label(_, inner) => graph.nullable[*inner as usize],
                    Instruction::Ref(rule) => graph.nullable[graph.rules[*rule as usize] as usize],
                    Instruction::Sequence(parts) => {
                        parts.iter().all(|p| graph.nullable[*p as usize])
                    }
                    Instruction::Choice(parts) => parts.iter().any(|p| graph.nullable[*p as usize]),
                    Instruction::Repeat { inner, min, .. } => {
                        *min == 0 || graph.nullable[*inner as usize]
                    }
                    _ => false,
                };
                if yes && !graph.nullable[id] {
                    graph.nullable[id] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        graph
    }
}
