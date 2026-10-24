use crate::{
    PrepareError, Source,
    parser::{self, Module, Rule},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub fn prepare(
    sources: &[Source],
    catalog: &str,
    start: &str,
    mode: &str,
) -> Result<Value, PrepareError> {
    let fallback = Source {
        path: "<project>".into(),
        text: String::new(),
    };
    let first = sources.first().unwrap_or(&fallback);
    if sources.is_empty() || !matches!(mode, "typed" | "syntax") {
        return Err(first.error([0, 0], "empty input or unknown admission mode"));
    }
    let catalog_value: Value = serde_json::from_str(catalog)
        .map_err(|e| first.error([0, 0], format!("invalid catalog JSON: {e}")))?;
    let mut modules = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        modules.extend(parser::parse(source, index)?);
    }
    let mut module_names = HashSet::new();
    let mut symbols = HashMap::new();
    for (mi, module) in modules.iter().enumerate() {
        let source = &sources[module.source];
        if !module_names.insert(module.name.clone()) {
            return Err(source.error(module.span, "duplicate module"));
        }
        let kinds: HashSet<_> = module.symbols.iter().collect();
        if kinds.len() != module.symbols.len() {
            return Err(source.error(module.span, "duplicate symbol namespace"));
        }
        for (ri, rule) in module.rules.iter().enumerate() {
            if symbols
                .insert(format!("{}.{}", module.name, rule.name), (mi, ri))
                .is_some()
            {
                return Err(source.error(rule.span, "duplicate rule"));
            }
        }
    }
    for module in &modules {
        for import in &module.imports {
            if !module_names.contains(import) {
                return Err(
                    sources[module.source].error(module.span, format!("unknown module {import}"))
                );
            }
        }
    }
    let resolve = |name: &str,
                   module: &Module,
                   implicit: Option<&str>,
                   span: [usize; 2]|
     -> Result<String, PrepareError> {
        let error =
            || sources[module.source].error(span, format!("unknown or ambiguous rule {name}"));
        if name.contains('.') {
            return symbols
                .contains_key(name)
                .then(|| name.into())
                .ok_or_else(error);
        }
        let local = format!("{}.{}", module.name, name);
        if symbols.contains_key(&local) {
            return Ok(local);
        }
        let candidates: HashSet<_> = module
            .imports
            .iter()
            .map(String::as_str)
            .chain(implicit)
            .filter(|m| *m != module.name)
            .map(|m| format!("{m}.{name}"))
            .filter(|n| symbols.contains_key(n))
            .collect();
        if candidates.len() == 1 {
            Ok(candidates.into_iter().next().unwrap())
        } else {
            Err(error())
        }
    };
    let mut points: HashMap<String, Vec<(usize, Rule, String)>> = HashMap::new();
    for (mi, module) in modules.iter().enumerate() {
        for rule in &module.rules {
            if let Some(alternatives) = &rule.alternatives {
                points.insert(
                    format!("{}.{}", module.name, rule.name),
                    alternatives
                        .iter()
                        .cloned()
                        .map(|a| (mi, a, module.name.clone()))
                        .collect(),
                );
            }
        }
    }
    for (mi, module) in modules.iter().enumerate() {
        for (target, alternatives) in &module.extensions {
            let target = resolve(target, module, None, module.span)?;
            let (owner, _) = symbols[&target];
            let point = points.get_mut(&target).ok_or_else(|| {
                sources[module.source].error(module.span, "extension target is not extensible")
            })?;
            point.extend(
                alternatives
                    .iter()
                    .cloned()
                    .map(|a| (mi, a, modules[owner].name.clone())),
            );
        }
    }
    let mut rules = Vec::new();
    let mut omitted = Vec::new();
    let mut add = |module: &Module,
                   rule: &Rule,
                   owner: Option<&str>,
                   implicit: Option<&str>,
                   associativity: Option<&str>|
     -> Result<Value, PrepareError> {
        let source = &sources[module.source];
        let name = format!("{}.{}", module.name, rule.name);
        if mode == "typed" && rule.block {
            return Err(source.error(
                rule.span,
                format!("C# semantics in {name} are not admitted"),
            ));
        }
        if mode == "syntax" && (rule.block || !rule.clauses.is_empty()) {
            omitted.push(json!({"rule":name,"block":rule.block,"clauses":rule.clauses.iter().map(|c|c["kind"].clone()).collect::<Vec<_>>()}));
        }
        validate_fields(rule, source, owner.is_none())?;
        for clause in &rule.clauses {
            let kinds: HashSet<_> = module
                .symbols
                .iter()
                .cloned()
                .chain(module.imports.iter().flat_map(|import| {
                    modules
                        .iter()
                        .filter(move |m| &m.name == import)
                        .flat_map(|m| m.symbols.clone())
                }))
                .collect();
            for kind in clause["symbol_kinds"].as_array().unwrap() {
                if !kinds.contains(kind.as_str().unwrap()) {
                    return Err(source.error(rule.span, format!("unknown symbol kind {kind}")));
                }
            }
        }
        let mut expr = rule.body.clone();
        normalize_expr(&mut expr, &mut |name, span| {
            let resolved = resolve(name, module, implicit, span)?;
            let (mi, ri) = symbols[&resolved];
            if rule.lexical && !modules[mi].rules[ri].lexical {
                return Err(source.error(span, "token cannot reference a syntax rule"));
            }
            Ok(resolved)
        })?;
        Ok(json!({"name":name,"lexical":rule.lexical,"expr":expr,
            "clauses":if mode=="syntax"{Vec::<Value>::new()}else{rule.clauses.clone()},"except":rule.except,
            "precedence":rule.precedence,"owner":owner,"associativity":associativity}))
    };
    for module in &modules {
        for rule in &module.rules {
            if rule.alternatives.is_some() {
                let point = format!("{}.{}", module.name, rule.name);
                let mut prefixes = Vec::new();
                let mut infixes = Vec::new();
                let mut operators = HashSet::new();
                let mut alternative_names = HashSet::new();
                for (mi, alternative, implicit) in &points[&point] {
                    let from = &modules[*mi];
                    if !alternative_names.insert(&alternative.name) {
                        return Err(sources[from.source]
                            .error(alternative.span, "duplicate extension alternative name"));
                    }
                    let head = first_expr(&alternative.body);
                    let span = expr_span(head);
                    let infix = head["kind"] == "ref"
                        && resolve(head["name"].as_str().unwrap(), from, Some(implicit), span)?
                            == point;
                    if infix {
                        if alternative.precedence.is_none_or(|p| p == 0) {
                            return Err(sources[from.source]
                                .error(alternative.span, "infix precedence must be 1..255"));
                        }
                        let mut operator = &alternative.body["items"][1];
                        if operator["kind"] == "label" {
                            operator = &operator["inner"];
                        }
                        let spelling = operator["text"]
                            .as_str()
                            .filter(|_| operator["kind"] == "literal")
                            .ok_or_else(|| {
                                sources[from.source].error(
                                    alternative.span,
                                    "infix must start with a literal operator",
                                )
                            })?;
                        if !operators.insert(spelling.to_owned()) {
                            return Err(sources[from.source]
                                .error(alternative.span, "conflicting extension operator"));
                        }
                    } else if alternative.associativity.is_some() {
                        return Err(sources[from.source]
                            .error(alternative.span, "prefix associativity is not admitted"));
                    }
                    let assoc =
                        infix.then(|| alternative.associativity.as_deref().unwrap_or("Left"));
                    let normalized = add(from, alternative, Some(&point), Some(implicit), assoc)?;
                    let name = normalized["name"].clone();
                    rules.push(normalized);
                    if infix {
                        infixes.push(name);
                    } else {
                        prefixes.push(name);
                    }
                }
                rules.push(json!({"name":point,"lexical":false,"expr":{"kind":"pratt","prefixes":prefixes,"infixes":infixes},"clauses":[],"except":[]}));
            } else {
                rules.push(add(module, rule, None, None, None)?);
            }
        }
    }
    let Some((mi, ri)) = symbols.get(start) else {
        return Err(first.error([0, 0], format!("unknown syntax start rule {start}")));
    };
    if modules[*mi].rules[*ri].lexical {
        return Err(first.error([0, 0], "start rule must be syntax"));
    }
    let identity = if sources.len() == 1 {
        sources[0].text.clone()
    } else {
        format!(
            "[{}]",
            sources
                .iter()
                .map(|s| dotnet_string(&s.text))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let mut symbol_kinds = Vec::new();
    for module in &modules {
        for kind in &module.symbols {
            if !symbol_kinds.contains(kind) {
                symbol_kinds.push(kind.clone());
            }
        }
    }
    let output = json!({"version":1,"symbol_kinds":symbol_kinds,"mode":mode,"omitted_semantics":omitted,
        "modules":modules.iter().map(|m|json!({"name":m.name,"imports":m.imports})).collect::<Vec<_>>(),
        "source_hash":hash(&identity),"catalog_hash":hash(catalog),"start":start,"rules":rules,"catalog":catalog_value});
    nitride_codegen::generate(&output.to_string())
        .map_err(|e| first.error([0, 0], format!("profile validation: {e}")))?;
    Ok(output)
}
fn expr_span(expr: &Value) -> [usize; 2] {
    [
        expr["_span"][0].as_u64().unwrap_or(0) as usize,
        expr["_span"][1].as_u64().unwrap_or(0) as usize,
    ]
}
fn normalize_expr(
    expr: &mut Value,
    resolve: &mut impl FnMut(&str, [usize; 2]) -> Result<String, PrepareError>,
) -> Result<(), PrepareError> {
    let span = expr_span(expr);
    if expr["kind"] == "ref" {
        expr["name"] = json!(resolve(expr["name"].as_str().unwrap(), span)?);
    }
    if let Some(items) = expr.get_mut("items").and_then(Value::as_array_mut) {
        for child in items {
            normalize_expr(child, resolve)?;
        }
    }
    if let Some(inner) = expr.get_mut("inner") {
        normalize_expr(inner, resolve)?;
    }
    expr.as_object_mut().unwrap().remove("_span");
    Ok(())
}
fn first_expr(expr: &Value) -> &Value {
    if expr["kind"] == "sequence" {
        first_expr(&expr["items"][0])
    } else if expr["kind"] == "label" {
        first_expr(&expr["inner"])
    } else {
        expr
    }
}
fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

// Match System.Text.Json's default ASCII/HTML-safe string serialization for
// the historical multi-source identity. Output JSON formatting is independent.
fn dotnet_string(text: &str) -> String {
    let mut out = String::from("\"");
    for unit in text.encode_utf16() {
        match unit {
            8 => out.push_str("\\b"),
            9 => out.push_str("\\t"),
            10 => out.push_str("\\n"),
            12 => out.push_str("\\f"),
            13 => out.push_str("\\r"),
            92 => out.push_str("\\\\"),
            32..=126 if !matches!(unit, 34 | 38 | 39 | 43 | 60 | 62 | 96) => {
                out.push(char::from_u32(unit as u32).unwrap())
            }
            _ => {
                use std::fmt::Write;
                write!(out, "\\u{unit:04X}").unwrap();
            }
        }
    }
    out.push('"');
    out
}

fn validate_fields(rule: &Rule, source: &Source, plain: bool) -> Result<(), PrepareError> {
    fn labels(expr: &Value, source: &Source) -> Result<(), PrepareError> {
        if let Some(items) = expr["items"].as_array() {
            let mut seen = HashSet::new();
            for item in items {
                if expr["kind"] == "sequence"
                    && item["kind"] == "label"
                    && !seen.insert(item["label"].as_str().unwrap())
                {
                    return Err(source.error(
                        expr_span(item),
                        format!("duplicate label {}", item["label"]),
                    ));
                }
                labels(item, source)?;
            }
        }
        if let Some(inner) = expr.get("inner") {
            labels(inner, source)?;
        }
        Ok(())
    }
    labels(&rule.body, source)?;
    let body = &rule.body;
    let refs = body["kind"] == "ref"
        || (body["kind"] == "choice"
            && body["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["kind"] == "ref"));
    if plain && refs && !rule.clauses.is_empty() {
        return Err(source.error(rule.span, "alias rules cannot carry binding clauses"));
    }
    let elements = body["items"]
        .as_array()
        .filter(|_| body["kind"] == "sequence")
        .map(Vec::as_slice)
        .unwrap_or(std::slice::from_ref(body));
    let fields: HashSet<_> = elements
        .iter()
        .filter(|e| e["kind"] == "label" && e["inner"]["kind"] != "predicate")
        .filter_map(|e| e["label"].as_str())
        .collect();
    let mut seen = HashSet::new();
    for clause in &rule.clauses {
        let kind = clause["kind"].as_str().unwrap();
        let key = if kind.starts_with("Lowers") {
            "Lowers"
        } else {
            kind
        };
        if !seen.insert(key) {
            return Err(source.error(rule.span, format!("duplicate {key} clause")));
        }
        if kind != "Lowers" {
            let field = clause["field"].as_str().unwrap();
            if field != "this" && !fields.contains(field) {
                return Err(source.error(rule.span, format!("unknown clause field {field}")));
            }
        }
        for arg in clause["args"].as_array().unwrap() {
            let name = arg.as_str().unwrap();
            if !fields.contains(name) {
                return Err(source.error(rule.span, format!("unknown argument field {name}")));
            }
        }
        if kind == "Declares" && !clause["target"].is_null() {
            let name = clause["target"].as_str().unwrap();
            if !name.contains('.') && !fields.contains(name) {
                return Err(source.error(rule.span, format!("unknown annotation field {name}")));
            }
        }
    }
    Ok(())
}
