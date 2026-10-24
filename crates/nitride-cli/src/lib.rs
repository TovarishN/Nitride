pub mod functions {
    include!(concat!(env!("OUT_DIR"), "/functions.rs"));
}
pub mod rewrites {
    include!(concat!(env!("OUT_DIR"), "/rewrites.rs"));
}
pub mod dates {
    include!(concat!(env!("OUT_DIR"), "/dates.rs"));
}
pub mod calls {
    include!(concat!(env!("OUT_DIR"), "/calls.rs"));
}
include!(concat!(env!("OUT_DIR"), "/language.rs"));
pub mod inference {
    include!(concat!(env!("OUT_DIR"), "/inference.rs"));
}
pub mod namespaces {
    include!(concat!(env!("OUT_DIR"), "/namespaces.rs"));
}
pub mod xml {
    include!(concat!(env!("OUT_DIR"), "/xml.rs"));
}
pub mod composition {
    include!(concat!(env!("OUT_DIR"), "/composition.rs"));
}
pub mod power {
    include!(concat!(env!("OUT_DIR"), "/power.rs"));
}

use nitride_core::*;
use serde_json::{Value, json};

pub fn syntax_observe(language: &'static Language, source: &str) -> Value {
    let tree = match parse(language, source, Limits::default()) {
        Ok(tree) => tree,
        Err(error) => {
            return json!({"ok":false,"error":error.category,"span":error.span,"semantic_validation":false});
        }
    };
    fn alias(expr: &Expr) -> bool {
        match expr {
            Expr::Ref(_) => true,
            Expr::Label(_, inner) => alias(inner),
            Expr::Choice(items) => !items.is_empty() && items.iter().all(alias),
            _ => false,
        }
    }
    let mut nodes = Vec::new();
    let mut stack = vec![(tree.root, None::<usize>)];
    while let Some((id, parent)) = stack.pop() {
        let node = tree.nodes[id as usize];
        let rule = &language.rules[node.rule as usize];
        // Aliases and transparent alternatives have no node of their own in Nitrogen.
        let next_parent = if !alias(&rule.expr) && !rule.transparent {
            let index = nodes.len();
            nodes.push(json!({"rule":rule.name,"span":node.span,"parent":parent}));
            Some(index)
        } else {
            parent
        };
        stack.extend(
            tree.children(id)
                .iter()
                .rev()
                .map(|edge| (edge.node, next_parent)),
        );
    }
    json!({"ok":true,"profile":"syntax-only","semantic_validation":false,"nodes":nodes})
}
pub fn observe_power(source: &str) -> Value {
    observe_language(&power::LANGUAGE, source)
}

pub fn fixture_bindings() -> Vec<HostBinding> {
    fixture_bindings_for(&LANGUAGE)
}

pub fn fixture_bindings_for(language: &'static Language) -> Vec<HostBinding> {
    language
        .operations
        .iter()
        .zip(language.handlers)
        .map(|(signature, handler)| {
            if *handler == "iso-date" {
                return HostBinding::text(*signature, |text| {
                    iso_date_day_number(text)
                        .map(|day| day as f64)
                        .ok_or_else(|| "not a calendar date".into())
                });
            }
            HostBinding::new(*signature, move |a| {
                Ok(match *handler {
                    "zero" => 0.0,
                    "identity" => a[0],
                    "neg" => -a[0],
                    "add" => a[0] + a[1],
                    "sub" => a[0] - a[1],
                    "mul" => a[0] * a[1],
                    "div" => a[0] / a[1],
                    "pow" => a[0].powf(a[1]),
                    _ => return Err("unknown native handler".into()),
                })
            })
        })
        .collect()
}

/// Days since 0001-01-01 for the current UTC date; the fixture has no time-zone policy.
pub fn utc_today() -> f64 {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (719_162 + seconds / 86_400) as f64
}

/// The Dates fixture host's built-in values: `pi`, and `today` as a day number.
pub fn dates_values(today: f64) -> Vec<HostValue> {
    vec![
        HostValue::new("value", "pi", "Core.Scalar", || Ok(std::f64::consts::PI)),
        HostValue::new("value", "today", "Dates.Date", move || Ok(today)),
    ]
}

pub fn observe_dates(source: &str, today: f64) -> Value {
    observe_with_values(&dates::LANGUAGE, source, true, &dates_values(today))
}

pub fn observe_rewrites(source: &str, today: f64) -> Value {
    observe_with_values(&rewrites::LANGUAGE, source, true, &dates_values(today))
}

pub fn observe(source: &str) -> Value {
    observe_language(&LANGUAGE, source)
}

pub fn observe_functions(source: &str) -> Value {
    observe_with_kinds(&functions::LANGUAGE, source, true)
}

pub fn observe_calls(source: &str) -> Value {
    observe_with_kinds(&calls::LANGUAGE, source, true)
}

pub fn observe_inference(source: &str) -> Value {
    observe_with_kinds(&inference::LANGUAGE, source, true)
}

pub fn observe_namespaces(source: &str) -> Value {
    observe_with_kinds(&namespaces::LANGUAGE, source, true)
}

pub fn observe_language(language: &'static Language, source: &str) -> Value {
    observe_with_kinds(language, source, false)
}

fn observe_with_kinds(language: &'static Language, source: &str, include_kinds: bool) -> Value {
    observe_with_values(language, source, include_kinds, &[])
}

fn observe_with_values(
    language: &'static Language,
    source: &str,
    include_kinds: bool,
    values: &[HostValue],
) -> Value {
    let error = |e: Diagnostic| {
        let mut observation =
            json!({"ok":false,"error":e.category,"span":e.span,"message":e.message,"trace":[]});
        if let Some(code) = e.code {
            observation["code"] = json!(code);
        }
        observation
    };
    let tree = match parse(language, source, Limits::default()) {
        Ok(t) => t,
        Err(e) => return error(e),
    };
    let program = match compile(language, &tree, Limits::default()) {
        Ok(p) => p,
        Err(e) => return error(e.into_iter().next().unwrap()),
    };
    let report = match execute_with_values(
        &program,
        &fixture_bindings_for(language),
        values,
        Limits::default(),
    ) {
        Ok(r) => r,
        Err(failure) => {
            return json!({"ok":false,"error":failure.diagnostic.category,"message":failure.diagnostic.message,"trace":failure.trace});
        }
    };
    let mut syntax = Vec::new();
    let mut stack = vec![tree.root];
    while let Some(id) = stack.pop() {
        let node = tree.nodes[id as usize];
        let rule = &language.rules[node.rule as usize];
        if !matches!(rule.action, Action::None)
            || rule.name == "Portable.Paren"
            || rule.name == "Namespaces.Paren"
            || rule.name == "Inference.Paren"
            || rule.name == "Calls.Paren"
            || rule.name == "Functions.Paren"
            || rule.name == "Dates.Paren"
        {
            syntax.push(json!({"kind":rule.name,"span":node.span}));
        }
        stack.extend(tree.children(id).iter().rev().map(|e| e.node));
    }
    let mut hir = Vec::<Value>::with_capacity(program.nodes.len());
    for node in &program.nodes {
        let mut observation = match &node.kind {
            HirKind::Constant(number) => {
                json!({"kind":"constant","type":node.ty,"span":node.span,"number":number})
            }
            HirKind::Reference(symbol) => {
                json!({"kind":"reference","type":node.ty,"span":node.span,"symbol":symbol})
            }
            HirKind::Builtin(value) => {
                json!({"kind":"reference","type":node.ty,"span":node.span,"builtin":value.name})
            }
            HirKind::Text { start, end } => {
                json!({"kind":"text","type":node.ty,"span":node.span,"text":&program.text[*start as usize..*end as usize]})
            }
            HirKind::Operation { operation, args } => {
                json!({"kind":"operation","type":node.ty,"span":node.span,"operation":language.operations[*operation as usize].id,
                "args":program.args[args.start as usize..args.end as usize].iter().map(|id| hir[*id as usize].clone()).collect::<Vec<_>>()})
            }
        };
        if std::ptr::eq(language, &functions::LANGUAGE) {
            observation["origins"] = json!(program.origins(hir.len() as u32));
        }
        if !language.helpers.is_empty() {
            // Rewrite profiles report every origin with its source, as Nitrogen does.
            let id = hir.len() as u32;
            let paths = program.origin_in_rewrite(id);
            observation.as_object_mut().unwrap().remove("span");
            observation["origins"] =
                json!(program.origins(id).iter().zip(paths).map(|(span, rewrite)| {
                json!({"path":if *rewrite {"nitrogen:rewrite"} else {"input"},"span":span})
            }).collect::<Vec<_>>());
        }
        hir.push(observation);
    }
    let mut bindings = program
        .bindings
        .iter()
        .map(|b| {
            if include_kinds {
                json!({"span":b.span,"symbol":b.symbol,"kind":b.kind})
            } else {
                json!({"span":b.span,"symbol":b.symbol})
            }
        })
        .chain(
            program
                .builtin_bindings
                .iter()
                .map(|b| json!({"span":b.span,"builtin":b.name,"kind":b.kind})),
        )
        .collect::<Vec<_>>();
    bindings.sort_by_key(|b| b["span"][0].as_u64().unwrap());
    json!({"ok":true,"syntax":syntax,"bindings":bindings,
        "hir":program.statements.iter().map(|s| &hir[s.value as usize]).collect::<Vec<_>>(),"values":report.values,"trace":report.trace})
}
