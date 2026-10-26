//! Observations for recovery tooling; this path never executes a program.
use nitride_core::*;
use serde_json::{Value, json};

pub fn recovery_language(profile: &str) -> Option<&'static Language<'static>> {
    Some(match profile {
        "portable" => &crate::LANGUAGE,
        "power" => &crate::power::LANGUAGE,
        "composition" => &crate::composition::LANGUAGE,
        "xml" => &crate::xml::LANGUAGE,
        "namespaces" => &crate::namespaces::LANGUAGE,
        "inference" => &crate::inference::LANGUAGE,
        "calls" => &crate::calls::LANGUAGE,
        "functions" => &crate::functions::LANGUAGE,
        "dates" => &crate::dates::LANGUAGE,
        "rewrites" => &crate::rewrites::LANGUAGE,
        "csharp4" => &crate::csharp4::LANGUAGE,
        "grammar" => &crate::grammar_fixture::LANGUAGE,
        _ => return None,
    })
}

pub fn recovery_observe(language: &'static Language<'static>, source: &str) -> Value {
    let start = std::time::Instant::now();
    let parsed = parse_recovering(
        language,
        source,
        Limits::default(),
        RecoveryOptions::default(),
    );
    let parse_ns = start.elapsed().as_nanos() as u64;
    let analysis = compile_recovering(language, &parsed, Limits::default());
    let tree = &parsed.tree;
    let diagnostics: Vec<_> = analysis
        .diagnostics
        .iter()
        .map(|d| json!({"category":d.category,"span":d.span,"message":d.message,"code":d.code}))
        .collect();
    let damaged: std::collections::HashSet<_> =
        tree.recovery().map_or_else(Default::default, |r| {
            r.damaged_nodes.iter().copied().collect()
        });
    let nodes: Vec<_> = tree.nodes.iter().enumerate().map(|(id,n)| {
        let rule = &language.rules[n.rule as usize];
        json!({"id":id,"rule":rule.name,"span":n.span,"lexical":rule.lexical,"missing":damaged.contains(&(id as u32)),"item":matches!(rule.action,Action::Declare { .. } | Action::Show { .. } | Action::Template { .. } | Action::Alias { .. }),"children":tree.children(id as u32).iter().map(|e|json!({"label":tree.label(e),"node":e.node})).collect::<Vec<_>>()})
    }).collect();
    let repairs: Vec<_> = tree.recovery().map(|r|r.repairs.iter().map(|repair| match repair {
        Repair::Missing {expected,at} => json!({"kind":"missing","at":at,"expected":match expected {Expected::Literal(text)=>text.to_string(),Expected::Rule(id)=>language.rules[*id as usize].name.into(),Expected::Character=>"character".into()}}),
        Repair::Skipped {span} => json!({"kind":"skipped","span":span}),
    }).collect()).unwrap_or_default();
    let coverage: Vec<_> = tree
        .recovery()
        .map(|r| {
            r.coverage
                .iter()
                .map(|c| json!({"span":c.span,"kind":format!("{:?}",c.kind)}))
                .collect()
        })
        .unwrap_or_else(|| {
            if source.is_empty() {
                vec![]
            } else {
                vec![json!({"span":[0,source.len()],"kind":"Retained"})]
            }
        });
    let mut hir = vec![];
    let mut bindings = vec![];
    let mut statements = vec![];
    if let Some(program) = &analysis.program {
        for (id, n) in program.nodes.iter().enumerate() {
            let mut value = match &n.kind {
                HirKind::Error => json!({"kind":"error"}),
                HirKind::Constant(value) => json!({"kind":"constant","value":value}),
                HirKind::Reference(symbol) => json!({"kind":"reference","symbol":symbol}),
                HirKind::Builtin(value) => {
                    json!({"kind":"builtin","name":value.name,"namespace":value.kind})
                }
                HirKind::Text { start, end } => {
                    json!({"kind":"text","value":&program.text[*start as usize..*end as usize]})
                }
                HirKind::Operation { operation, args } => {
                    json!({"kind":"operation","operation":language.operations[*operation as usize].id,"args":&program.args[args.start as usize..args.end as usize]})
                }
            };
            value["id"] = json!(id);
            value["span"] = json!(n.span);
            value["type"] = json!(n.ty);
            value["origins"] = json!(program.origins(id as u32));
            value["origin_rewrites"] = json!(program.origin_in_rewrite(id as u32));
            hir.push(value);
        }
        bindings = program
            .bindings
            .iter()
            .map(|b| json!({"namespace":b.kind,"span":b.span,"symbol":b.symbol}))
            .collect();
        statements = program
            .statements
            .iter()
            .map(|s| json!({"value":s.value,"symbol":s.symbol}))
            .collect();
    }
    json!({"ok":diagnostics.is_empty(),"root":tree.root,"nodes":nodes,"repairs":repairs,"coverage":coverage,"diagnostics":diagnostics,"hir":hir,"bindings":bindings,"statements":statements,"trace":[],"parse_ns":parse_ns,"stats":{"work":parsed.stats.work,"search_states":parsed.stats.search_states,"repair_count":parsed.stats.repair_count,"fallback_count":parsed.stats.fallback_count}})
}
