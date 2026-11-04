use crate::{
    PrepareError, Source,
    parser::{self, Module, Rule},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// Prepares a language, reporting every independent error rather than the
/// first. Errors are collected within a stage (each source, member, rule or
/// helper on its own) and preparation stops after a stage that had any, so a
/// failing declaration does not cascade into errors about its uses. The
/// first error is the one a fail-fast preparation would have reported.
pub fn prepare(
    sources: &[Source],
    catalog: &str,
    start: &str,
    mode: &str,
    tolerant: bool,
    // Checks the catalog against the prepared rules before profile validation.
    check: &dyn Fn(&Value) -> Result<(), PrepareError>,
) -> Result<crate::Prepared, Vec<PrepareError>> {
    let mut errors = Vec::new();
    // Ends preparation after a stage that found errors.
    macro_rules! barrier {
        () => {
            if !errors.is_empty() {
                return Err(errors);
            }
        };
    }
    let fallback = Source {
        path: "<project>".into(),
        text: String::new(),
    };
    let first = sources.first().unwrap_or(&fallback);
    if sources.is_empty() || !matches!(mode, "typed" | "syntax") {
        return Err(vec![
            first.project_error("empty input or unknown admission mode"),
        ]);
    }
    let catalog_value: Value = serde_json::from_str(catalog)
        .map_err(|e| vec![first.project_error(format!("invalid catalog JSON: {e}"))])?;
    let mut modules = Vec::new();
    let mut warnings = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        match parser::parse(source, index, tolerant) {
            Ok((parsed, skipped)) => {
                modules.extend(parsed);
                warnings.extend(skipped);
            }
            Err(found) => errors.extend(found),
        }
    }
    barrier!();
    // Grammar functions expand before validation, as in Nitrogen's compiler.
    crate::functions::expand(&mut modules, sources).map_err(|e| vec![e])?;
    let mut module_names = HashSet::new();
    let mut symbols = HashMap::new();
    for (mi, module) in modules.iter().enumerate() {
        let source = &sources[module.source];
        if !module_names.insert(module.name.clone()) {
            errors.push(source.error(module.span, "duplicate module"));
        }
        let kinds: HashSet<_> = module.symbols.iter().collect();
        if kinds.len() != module.symbols.len() {
            errors.push(source.error(module.span, "duplicate symbol namespace"));
        }
        for (ri, rule) in module.rules.iter().enumerate() {
            if symbols
                .insert(format!("{}.{}", module.name, rule.name), (mi, ri))
                .is_some()
            {
                errors.push(source.error(rule.span, "duplicate rule"));
            }
        }
    }
    // Parse-context flags, qualified by their module; each language gives each its own bit.
    let mut contexts = Vec::new();
    for module in &modules {
        for (name, span) in &module.contexts {
            let qualified = format!("{}.{}", module.name, name);
            if contexts.contains(&qualified) || symbols.contains_key(&qualified) {
                errors.push(sources[module.source].error(*span, "duplicate context flag"));
                continue;
            }
            contexts.push(qualified);
        }
    }
    if contexts.len() > 64 {
        errors.push(first.project_error("more than 64 context flags"));
    }
    let resolve_context = |name: &str, module: &Module, span: [usize; 2]| {
        let error = || {
            sources[module.source].error(span, format!("unknown or ambiguous context flag {name}"))
        };
        if name.contains('.') {
            return contexts
                .iter()
                .find(|c| *c == name)
                .cloned()
                .ok_or_else(error);
        }
        let local = format!("{}.{}", module.name, name);
        if contexts.contains(&local) {
            return Ok(local);
        }
        let candidates: Vec<_> = module
            .imports
            .iter()
            .map(|m| format!("{m}.{name}"))
            .filter(|n| contexts.contains(n))
            .collect();
        if candidates.len() == 1 {
            Ok(candidates.into_iter().next().unwrap())
        } else {
            Err(error())
        }
    };
    for module in &modules {
        for import in &module.imports {
            if !module_names.contains(import) {
                errors.push(
                    sources[module.source].error(module.span, format!("unknown module {import}")),
                );
            }
        }
    }
    barrier!();
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
    // Named patterns (Nitrogen PR 92), by qualified name: (module, pattern).
    let mut pattern_names: HashMap<String, (usize, usize)> = HashMap::new();
    for (mi, module) in modules.iter().enumerate() {
        let taken: HashSet<&str> = module
            .rules
            .iter()
            .map(|r| r.name.as_str())
            .chain(module.helpers.iter().map(|h| h.name.as_str()))
            .collect();
        for (pi, pattern) in module.patterns.iter().enumerate() {
            let qualified = format!("{}.{}", module.name, pattern.name);
            if taken.contains(pattern.name.as_str())
                || pattern_names.insert(qualified, (mi, pi)).is_some()
            {
                errors.push(sources[module.source].coded(
                    pattern.name_span,
                    "NGR0164",
                    format!("duplicate rule/helper/pattern '{}'", pattern.name),
                ));
            }
        }
    }
    barrier!();
    // A pattern visible from `module`: qualified, its own, or one import's.
    let find_pattern = |name: &str, module: &Module| -> Option<String> {
        if name.contains('.') {
            return pattern_names.contains_key(name).then(|| name.into());
        }
        let local = format!("{}.{name}", module.name);
        if pattern_names.contains_key(&local) {
            return Some(local);
        }
        let found: Vec<_> = module
            .imports
            .iter()
            .map(|m| format!("{m}.{name}"))
            .filter(|n| pattern_names.contains_key(n))
            .collect();
        (found.len() == 1).then(|| found[0].clone())
    };
    let syntax_category = |name: &str, module: &Module, at: [usize; 2], what: &str| {
        let resolved = resolve(name, module, None, at)?;
        let (mi, ri) = symbols[&resolved];
        if modules[mi].rules[ri].lexical {
            return Err(sources[module.source].error(at, format!("{what} must be syntax rules")));
        }
        Ok(resolved)
    };
    // A capture's category, resolved: a named pattern's category (and the
    // pattern, qualified) when it names one, else a syntax rule.
    let capture = |capture: &mut Value, module: &Module| -> Result<(), PrepareError> {
        let at: [usize; 2] = serde_json::from_value(capture["span"].clone()).unwrap_or(module.span);
        let written = capture["category"].as_str().unwrap().to_owned();
        if let Some(pattern) = find_pattern(&written, module) {
            let (mi, pi) = pattern_names[&pattern];
            let (category, span) = &modules[mi].patterns[pi].category;
            let category = syntax_category(category, &modules[mi], *span, "pattern categories")?;
            capture["category"] = json!(category);
            capture["pattern"] = json!(pattern);
        } else if capture["descendant"] == true {
            return Err(sources[module.source].coded(
                at,
                "NGR0163",
                format!("'{written}' is not a named pattern; a descendant capture needs one"),
            ));
        } else {
            capture["category"] =
                json!(syntax_category(&written, module, at, "capture categories")?);
        }
        capture.as_object_mut().unwrap().remove("span");
        Ok(())
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
            let target = match resolve(target, module, None, module.span) {
                Ok(target) => target,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };
            let (owner, _) = symbols[&target];
            let Some(point) = points.get_mut(&target) else {
                errors.push(
                    sources[module.source].error(module.span, "extension target is not extensible"),
                );
                continue;
            };
            point.extend(
                alternatives
                    .iter()
                    .cloned()
                    .map(|a| (mi, a, modules[owner].name.clone())),
            );
        }
    }
    let mut builtin_keys = HashSet::new();
    let mut builtins = Vec::new();
    for module in &modules {
        let kinds: HashSet<_> = module
            .symbols
            .iter()
            .chain(module.imports.iter().flat_map(|import| {
                modules
                    .iter()
                    .filter(move |m| &m.name == import)
                    .flat_map(|m| m.symbols.iter())
            }))
            .collect();
        for (kind, name) in &module.builtins {
            if !kinds.contains(kind) || !builtin_keys.insert((kind, name)) {
                errors.push(
                    sources[module.source].error(module.span, "unknown kind or duplicate builtin"),
                );
                continue;
            }
            builtins.push(json!({"kind":kind,"name":name}));
        }
    }
    // Quote helpers: categories must be syntax rules; names are module-qualified.
    let mut helper_names = HashSet::new();
    let mut helpers = Vec::new();
    for module in &modules {
        let source = &sources[module.source];
        for helper in &module.helpers {
            let name = format!("{}.{}", module.name, helper.name);
            if symbols.contains_key(&name) || !helper_names.insert(name.clone()) {
                errors.push(source.error(helper.span, "duplicate helper name"));
                continue;
            }
            // Each helper checks on its own: its first error is recorded.
            let value = (|| -> Result<Value, PrepareError> {
                let category = |name: &str, span| -> Result<String, PrepareError> {
                    let resolved = resolve(name, module, None, span)?;
                    let (mi, ri) = symbols[&resolved];
                    if modules[mi].rules[ri].lexical {
                        return Err(source.error(span, "helper categories must be syntax rules"));
                    }
                    Ok(resolved)
                };
                let parameters = |list: &[parser::Parameter], taken: &mut HashSet<String>| {
                    list.iter()
                        .map(|p| {
                            if !taken.insert(p.name.clone()) {
                                return Err(source.error(p.span, "duplicate helper parameter"));
                            }
                            Ok(json!({"name":p.name,"category":category(&p.category, p.span)?}))
                        })
                        .collect::<Result<Vec<_>, _>>()
                };
                let mut taken = HashSet::new();
                let mut value = json!({"name":name,"parameters":parameters(&helper.parameters, &mut taken)?,
                "result":category(&helper.result.0, helper.result.1)?,"body":helper.body,"match":null});
                if let Some(selector) = &helper.selector {
                    if !helper.parameters.iter().any(|p| &p.name == selector) {
                        return Err(source.error(helper.span, "match selector must be a parameter"));
                    }
                    let mut arms = Vec::new();
                    for arm in &helper.arms {
                        let mut names = taken.clone();
                        // Arm captures may name patterns, as check captures do.
                        let mut captures = crate::parser::captures_json(&arm.captures);
                        for (item, written) in captures.iter_mut().zip(&arm.captures) {
                            if !names.insert(written.name.clone()) {
                                return Err(
                                    source.error(written.span, "duplicate helper parameter")
                                );
                            }
                            capture(item, module)?;
                        }
                        arms.push(json!({"captures":captures,
                        "pattern":arm.pattern,"result":arm.result}));
                    }
                    value["match"] = json!({"selector":selector,"arms":arms});
                }
                Ok(value)
            })();
            match value {
                Ok(value) => helpers.push(value),
                Err(error) => errors.push(error),
            }
        }
    }
    barrier!();
    let resolve_helper = |name: &str, module: &Module, span| -> Result<String, PrepareError> {
        let error =
            || sources[module.source].error(span, format!("unknown or ambiguous helper {name}"));
        if name.contains('.') {
            return helper_names
                .contains(name)
                .then(|| name.into())
                .ok_or_else(error);
        }
        let local = format!("{}.{}", module.name, name);
        if helper_names.contains(&local) {
            return Ok(local);
        }
        let candidates: Vec<_> = module
            .imports
            .iter()
            .map(|m| format!("{m}.{name}"))
            .filter(|n| helper_names.contains(n))
            .collect();
        if candidates.len() == 1 {
            Ok(candidates.into_iter().next().unwrap())
        } else {
            Err(error())
        }
    };
    let mut rules = Vec::new();
    let mut omitted = Vec::new();
    let mut declarations = Vec::new();
    let mut references = Vec::new();
    let mut add = |module: &Module,
                   rule: &Rule,
                   owner: Option<&str>,
                   implicit: Option<&str>,
                   associativity: Option<&str>|
     -> Result<Value, PrepareError> {
        let source = &sources[module.source];
        let name = format!("{}.{}", module.name, rule.name);
        for clause in &rule.clauses {
            let field: String = clause["field"].as_str().unwrap_or_default().into();
            let kinds: Vec<String> = clause["symbol_kinds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|k| k.as_str().map(Into::into))
                .collect();
            if clause["kind"] == "Declares" {
                declarations.push(crate::Declaration {
                    rule: name.clone(),
                    field,
                    kinds,
                });
            } else if clause["kind"] == "References" {
                references.push(crate::ReferenceClause {
                    rule: name.clone(),
                    field,
                    kinds,
                });
            }
        }
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
        let mut clauses = rule.clauses.clone();
        for clause in &mut clauses {
            if clause["kind"] == "Rewrites" && !clause["target"].is_null() {
                clause["target"] = json!(resolve_helper(
                    clause["target"].as_str().unwrap(),
                    module,
                    rule.span
                )?);
            }
        }
        let mut expr = rule.body.clone();
        normalize_expr(
            &mut expr,
            &mut |name, span| {
                let resolved = resolve(name, module, implicit, span)?;
                let (mi, ri) = symbols[&resolved];
                if rule.lexical && !modules[mi].rules[ri].lexical {
                    return Err(source.error(span, "token cannot reference a syntax rule"));
                }
                Ok(resolved)
            },
            &mut |name, span| {
                if rule.lexical {
                    return Err(source.error(span, "a token cannot use parse context"));
                }
                resolve_context(name, module, span)
            },
        )?;
        let mut checks = Vec::new();
        for check in &rule.checks {
            let span = |v: &Value| -> [usize; 2] {
                serde_json::from_value(v["span"].clone()).unwrap_or(rule.span)
            };
            let mut check = check.clone();
            let captures = check.get_mut("captures").and_then(Value::as_array_mut);
            let mut names = Vec::new();
            for item in captures.into_iter().flatten() {
                names.push(item["name"].as_str().unwrap().to_owned());
                capture(item, module)?;
            }
            if let Some(context) = check.get_mut("context") {
                let category = match context["category"].as_str() {
                    Some(written) => syntax_category(
                        written,
                        module,
                        span(&json!({"span":context["category_span"]})),
                        "context categories",
                    )?,
                    None => name.clone(),
                };
                context["category"] = json!(category);
                context.as_object_mut().unwrap().remove("category_span");
                for item in context["captures"].as_array_mut().into_iter().flatten() {
                    let own = item["name"].as_str().unwrap().to_owned();
                    if names.contains(&own) {
                        return Err(source.coded(
                            span(item),
                            "NGR0165",
                            format!("capture '{own}' is already a capture of the matched pattern; rename one"),
                        ));
                    }
                    names.push(own);
                    capture(item, module)?;
                }
            }
            let mut labels = Vec::new();
            crate::expr_labels(&rule.body, &mut labels);
            for key in ["left", "right", "at"] {
                if let Some(field) = check[key].as_str()
                    && !labels.contains(&field)
                {
                    return Err(
                        source.error(span(&check), format!("{field} is not a field of {name}"))
                    );
                }
            }
            let captures: Vec<&str> = names.iter().map(String::as_str).collect();
            for placeholder in nitride_core::placeholders(check["message"].as_str().unwrap()) {
                let known = match placeholder.strip_suffix(".Text") {
                    Some(field) => labels.contains(&field),
                    None => captures.contains(&placeholder),
                };
                if !known {
                    return Err(source.error(
                        span(&check),
                        format!("unknown message placeholder {{{placeholder}}}"),
                    ));
                }
            }
            check.as_object_mut().unwrap().remove("span");
            checks.push(check);
        }
        let mut value = json!({"name":name,"lexical":rule.lexical,"expr":expr,
            "clauses":if mode=="syntax"{Vec::<Value>::new()}else{clauses},"except":rule.except,
            "precedence":rule.precedence,"owner":owner,"associativity":associativity});
        if rule.transparent {
            value["transparent"] = json!(true);
        }
        if !checks.is_empty() {
            value["checks"] = json!(checks);
        }
        Ok(value)
    };
    for module in &modules {
        for rule in &module.rules {
            if rule.alternatives.is_some() {
                let point = format!("{}.{}", module.name, rule.name);
                let mut prefixes = Vec::new();
                let mut infixes = Vec::new();
                let mut alternative_names = HashSet::new();
                for (mi, alternative, implicit) in &points[&point] {
                    let from = &modules[*mi];
                    if !alternative_names.insert(&alternative.name) {
                        errors.push(
                            sources[from.source]
                                .error(alternative.span, "duplicate extension alternative name"),
                        );
                        continue;
                    }
                    let mut head = first_expr(&alternative.body);
                    let scoped = head["kind"] == "scope";
                    while head["kind"] == "scope" {
                        head = first_expr(&head["inner"]);
                    }
                    let span = expr_span(head);
                    let infix = if head["kind"] == "ref" {
                        match resolve(head["name"].as_str().unwrap(), from, Some(implicit), span) {
                            Ok(resolved) => resolved == point,
                            Err(error) => {
                                errors.push(error);
                                continue;
                            }
                        }
                    } else {
                        false
                    };
                    if infix && scoped {
                        errors.push(sources[from.source].error(
                            alternative.span,
                            "a context scope around an infix's left operand is not admitted",
                        ));
                        continue;
                    }
                    // Alternatives may share an operator: the parser ranks them by longest
                    // match, then precedence, as Nitrogen does. A prefix alternative's
                    // associativity is accepted and ignored, as Nitrogen's generator ignores
                    // it: its trailing operand binds at its own precedence.
                    if infix && alternative.precedence.is_none_or(|p| p == 0) {
                        errors.push(
                            sources[from.source]
                                .error(alternative.span, "infix precedence must be 1..255"),
                        );
                        continue;
                    }
                    // A transparent alternative yields exactly one node: a literal,
                    // class or reference, labeled or not, or a choice of them.
                    fn single(expr: &Value) -> bool {
                        match expr["kind"].as_str() {
                            Some("literal" | "class" | "ref") => true,
                            Some("label") => single(&expr["inner"]),
                            Some("choice") => expr["items"].as_array().unwrap().iter().all(single),
                            _ => false,
                        }
                    }
                    if alternative.transparent
                        && (infix
                            || alternative.precedence.is_some()
                            || !alternative.clauses.is_empty()
                            || alternative.block
                            || !single(&alternative.body))
                    {
                        errors.push(sources[from.source].error(
                            alternative.span,
                            "a transparent alternative must be a prefix yielding exactly one node",
                        ));
                        continue;
                    }
                    let assoc =
                        infix.then(|| alternative.associativity.as_deref().unwrap_or("Left"));
                    let normalized =
                        match add(from, alternative, Some(&point), Some(implicit), assoc) {
                            Ok(normalized) => normalized,
                            Err(error) => {
                                errors.push(error);
                                continue;
                            }
                        };
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
                match add(module, rule, None, None, None) {
                    Ok(normalized) => rules.push(normalized),
                    Err(error) => errors.push(error),
                }
            }
        }
    }
    barrier!();
    let Some((mi, ri)) = symbols.get(start) else {
        return Err(vec![
            first.project_error(format!("unknown syntax start rule {start}")),
        ]);
    };
    if modules[*mi].rules[*ri].lexical {
        return Err(vec![first.project_error("start rule must be syntax")]);
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
    let mut output = json!({"version":1,"symbol_kinds":symbol_kinds,"builtins":builtins,"mode":mode,"omitted_semantics":omitted,
        "modules":modules.iter().map(|m|json!({"name":m.name,"imports":m.imports})).collect::<Vec<_>>(),
        "source_hash":hash(&identity),"catalog_hash":hash(catalog),"start":start,"rules":rules,"catalog":catalog_value});
    if !contexts.is_empty() {
        output["contexts"] = json!(contexts);
    }
    // Helpers are validated in both modes; syntax-only admission omits them.
    if !helpers.is_empty() && mode == "typed" {
        output["helpers"] = json!(helpers);
    }
    let patterns =
        named_patterns(&modules, sources, &capture, &syntax_category).map_err(|e| vec![e])?;
    if !patterns.is_empty() {
        output["patterns"] = json!(patterns);
    }
    check(&output).map_err(|e| vec![e])?;
    nitride_codegen::generate(&output.to_string())
        .map_err(|e| vec![first.project_error(format!("profile validation: {e}"))])?;
    Ok(crate::Prepared {
        data: output,
        warnings,
        declarations,
        references,
    })
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
    context: &mut impl FnMut(&str, [usize; 2]) -> Result<String, PrepareError>,
) -> Result<(), PrepareError> {
    let span = expr_span(expr);
    if expr["kind"] == "ref" {
        expr["name"] = json!(resolve(expr["name"].as_str().unwrap(), span)?);
    } else if expr["kind"] == "context" || expr["kind"] == "scope" {
        expr["name"] = json!(context(expr["name"].as_str().unwrap(), span)?);
    }
    if let Some(items) = expr.get_mut("items").and_then(Value::as_array_mut) {
        for child in items {
            normalize_expr(child, resolve, context)?;
        }
    }
    if let Some(inner) = expr.get_mut("inner") {
        normalize_expr(inner, resolve, context)?;
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
        let key = if kind.starts_with("Lowers") || kind == "Rewrites" {
            "Lowers"
        } else {
            kind
        };
        if !seen.insert(key) {
            return Err(source.error(rule.span, format!("duplicate {key} clause")));
        }
        if !matches!(kind, "Lowers" | "Scope" | "Rewrites") {
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

/// Resolves a capture in place (category, pattern) against its module.
type CaptureResolver<'a> = dyn Fn(&mut Value, &Module) -> Result<(), PrepareError> + 'a;
/// Resolves a category that must be a syntax rule; the last argument names it for errors.
type CategoryResolver<'a> =
    dyn Fn(&str, &Module, [usize; 2], &str) -> Result<String, PrepareError> + 'a;
/// A pattern's non-consuming references and where it is declared.
type Edges = Vec<(String, Vec<String>, (usize, [usize; 2]))>;

/// Every module's named patterns as prepared data, after rejecting reference
/// cycles that never step into a child (NQP0008), as Nitrogen does.
fn named_patterns(
    modules: &[Module],
    sources: &[Source],
    capture: &CaptureResolver<'_>,
    syntax_category: &CategoryResolver<'_>,
) -> Result<Vec<Value>, PrepareError> {
    let mut patterns = Vec::new();
    // Non-consuming references: a whole alternative that is one capture, or a descendant search.
    let mut edges: Edges = Vec::new();
    for module in modules {
        for pattern in &module.patterns {
            let name = format!("{}.{}", module.name, pattern.name);
            let category = syntax_category(
                &pattern.category.0,
                module,
                pattern.category.1,
                "pattern categories",
            )?;
            let mut targets = Vec::new();
            let mut alternatives = Vec::new();
            for (captures, text) in &pattern.alternatives {
                let mut captures = crate::parser::captures_json(captures);
                for item in &mut captures {
                    capture(item, module)?;
                    let whole = text.trim() == format!("${}", item["name"].as_str().unwrap());
                    // Only a whole-alternative reference (plain or `..`) stays on its
                    // node; anywhere else it sits inside structure, a level down.
                    if let Some(target) = item["pattern"].as_str()
                        && whole
                    {
                        targets.push(target.to_owned());
                    }
                }
                alternatives.push(json!({"captures":captures,"pattern":text}));
            }
            edges.push((name.clone(), targets, (module.source, pattern.name_span)));
            patterns.push(json!({"name":name,"category":category,"alternatives":alternatives}));
        }
    }
    // Depth-first, in declaration order: 1 on the path, 2 done.
    let mut state: HashMap<&str, u8> = HashMap::new();
    let mut path: Vec<&str> = Vec::new();
    fn visit<'a>(
        name: &'a str,
        edges: &'a Edges,
        state: &mut HashMap<&'a str, u8>,
        path: &mut Vec<&'a str>,
    ) -> Option<(String, (usize, [usize; 2]))> {
        state.insert(name, 1);
        path.push(name);
        let (_, targets, at) = edges.iter().find(|(n, ..)| n == name)?;
        for target in targets {
            match state.get(target.as_str()) {
                Some(1) => {
                    let start = path.iter().position(|p| p == target).unwrap();
                    let mut cycle: Vec<&str> = path[start..].to_vec();
                    cycle.push(target);
                    return Some((cycle.join(" -> "), *at));
                }
                Some(_) => {}
                None => {
                    if let Some(found) = visit(target, edges, state, path) {
                        return Some(found);
                    }
                }
            }
        }
        path.pop();
        state.insert(name, 2);
        None
    }
    for (name, ..) in &edges {
        if state.contains_key(name.as_str()) {
            continue;
        }
        if let Some((cycle, (source, span))) = visit(name, &edges, &mut state, &mut path) {
            return Err(sources[source].coded(
                span,
                "NQP0008",
                format!("Named patterns refer to each other without consuming syntax: {cycle}."),
            ));
        }
    }
    Ok(patterns)
}
