mod lexical;
mod syntax;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

fn text(v: &Value) -> Result<&str, String> {
    v.as_str().ok_or_else(|| "expected string".into())
}
fn array(v: &Value) -> Result<&Vec<Value>, String> {
    v.as_array().ok_or_else(|| "expected array".into())
}
fn strings(v: &Value) -> Result<Vec<&str>, String> {
    array(v)?.iter().map(text).collect()
}
const TEXT: &str = "Core.Text";

fn quoted(s: &str) -> String {
    format!("{s:?}")
}
fn list(items: impl IntoIterator<Item = String>) -> String {
    format!("&[{}]", items.into_iter().collect::<Vec<_>>().join(","))
}

pub fn generate(input: &str) -> Result<String, String> {
    let data: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    if data["version"] != 1 {
        return Err("unsupported format version".into());
    }
    for key in ["source_hash", "catalog_hash"] {
        let hash = text(&data[key])?;
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid source/catalog identity".into());
        }
    }
    let syntax_only = match data["mode"].as_str().unwrap_or("typed") {
        "typed" => false,
        "syntax" => true,
        _ => return Err("unknown admission mode".into()),
    };
    let rules = array(&data["rules"])?;
    if syntax_only
        && (data["catalog"]
            .get("overloads")
            .is_some_and(|v| v.as_object().is_none_or(|m| !m.is_empty()))
            || !array(&data["catalog"]["operations"])?.is_empty()
            || rules
                .iter()
                .any(|r| r["clauses"].as_array().is_none_or(|a| !a.is_empty()))
            || data["catalog"]["statements"]
                .as_object()
                .is_some_and(|m| !m.is_empty()))
    {
        return Err("syntax-only profile contains semantic actions".into());
    }
    if rules.is_empty() || rules.len() > u16::MAX as usize {
        return Err("invalid rule count".into());
    }
    let mut ids = HashMap::new();
    for (i, rule) in rules.iter().enumerate() {
        if ids.insert(text(&rule["name"])?, i as u16).is_some() {
            return Err("duplicate rule".into());
        }
    }
    let start = lookup(&ids, text(&data["start"])?)?;
    let types = strings(&data["catalog"]["types"])?;
    let type_set: HashSet<_> = types.iter().copied().collect();
    if (!syntax_only && types.is_empty()) || types.len() != type_set.len() {
        return Err("duplicate/empty types".into());
    }
    let operations = array(&data["catalog"]["operations"])?;
    if operations.len() > u16::MAX as usize {
        return Err("too many operations".into());
    }
    let mut op_ids = HashMap::new();
    let mut signatures = Vec::new();
    let mut handlers = Vec::new();
    for (i, op) in operations.iter().enumerate() {
        let id = text(&op["id"])?;
        if op_ids.insert(id, i as u16).is_some() {
            return Err("duplicate operation".into());
        }
        let inputs = strings(&op["inputs"])?;
        let result = text(&op["result"])?;
        if !type_set.contains(result) || inputs.iter().any(|t| !type_set.contains(t)) {
            return Err("unknown operation type".into());
        }
        signatures.push(format!(
            "nitride_core::Signature {{id:{}, inputs:{}, result:{}}}",
            quoted(id),
            list(inputs.iter().copied().map(quoted)),
            quoted(result)
        ));
        let handler = text(&op["handler"])?;
        let arity = match handler {
            "zero" => 0,
            "identity" | "neg" | "iso-date" => 1,
            "add" | "sub" | "mul" | "div" | "pow" => 2,
            _ => return Err("unknown fixture host handler".into()),
        };
        if array(&op["inputs"])?.len() != arity {
            return Err("fixture host handler arity mismatch".into());
        }
        // Text is only an operation input fed directly by source spelling.
        if result == TEXT || (handler == "iso-date") != inputs.contains(&TEXT) {
            return Err("text operations require a text host handler".into());
        }
        handlers.push(quoted(handler));
    }
    let mut groups = HashMap::<&str, Vec<u16>>::new();
    if let Some(overloads) = data["catalog"].get("overloads") {
        for (name, ids) in overloads.as_object().ok_or("overloads must be an object")? {
            if name.is_empty() || op_ids.contains_key(name.as_str()) {
                return Err("conflicting overload group identity".into());
            }
            let ids = strings(ids)?;
            if ids.is_empty() {
                return Err("empty overload group".into());
            }
            let mut inputs = HashSet::new();
            let mut candidates = Vec::new();
            for name in ids {
                let id = lookup(&op_ids, name)?;
                let signature = strings(&operations[id as usize]["inputs"])?;
                if signature.contains(&TEXT) {
                    return Err("overloads cannot take text arguments".into());
                }
                if !inputs.insert(signature) {
                    return Err("ambiguous overload input signature".into());
                }
                candidates.push(id);
            }
            groups.insert(name, candidates);
        }
    }
    let inventory = data.get("symbol_kinds").map(strings).transpose()?;
    if let Some(kinds) = &inventory {
        let unique: HashSet<_> = kinds.iter().copied().collect();
        if kinds.len() != unique.len() || kinds.iter().any(|s| s.is_empty()) {
            return Err("invalid symbol namespace inventory".into());
        }
    }
    let mut builtin_declarations = HashSet::new();
    if let Some(declarations) = data.get("builtins") {
        for builtin in array(declarations)? {
            let kind = text(&builtin["kind"])?;
            let name = text(&builtin["name"])?;
            if name.is_empty()
                || inventory.as_ref().is_none_or(|k| !k.contains(&kind))
                || !builtin_declarations.insert((kind, name))
            {
                return Err("invalid builtin inventory".into());
            }
        }
    }
    let mut builtins = Vec::new();
    let mut mapped_builtins = HashSet::new();
    if let Some(mapping) = data["catalog"].get("builtins") {
        for (kind, names) in mapping
            .as_object()
            .ok_or("builtin catalog must be an object")?
        {
            if inventory
                .as_ref()
                .is_none_or(|k| !k.contains(&kind.as_str()))
            {
                return Err("unknown builtin catalog namespace".into());
            }
            for (name, operations_for_name) in
                names.as_object().ok_or("builtin names must be an object")?
            {
                if !builtin_declarations.contains(&(kind.as_str(), name.as_str())) {
                    return Err("undeclared builtin mapping".into());
                }
                let mut inputs = HashSet::new();
                let mut candidates = Vec::new();
                for name in strings(operations_for_name)? {
                    let id = lookup(&op_ids, name)?;
                    if !inputs.insert(strings(&operations[id as usize]["inputs"])?) {
                        return Err("ambiguous builtin input signature".into());
                    }
                    candidates.push(id.to_string());
                }
                if candidates.is_empty() {
                    return Err("empty builtin signatures".into());
                }
                mapped_builtins.insert((kind.as_str(), name.as_str()));
                builtins.push((
                    kind.as_str(),
                    format!(
                        "nitride_core::Builtin {{kind:{},name:{},candidates:{}}}",
                        quoted(kind),
                        quoted(name),
                        list(candidates)
                    ),
                ));
            }
        }
    }
    let mut values = Vec::new();
    if let Some(mapping) = data["catalog"].get("values") {
        for (kind, names) in mapping
            .as_object()
            .ok_or("value catalog must be an object")?
        {
            if inventory
                .as_ref()
                .is_none_or(|k| !k.contains(&kind.as_str()))
            {
                return Err("unknown builtin value namespace".into());
            }
            for (name, ty) in names.as_object().ok_or("value names must be an object")? {
                let ty = text(ty)?;
                if !builtin_declarations.contains(&(kind.as_str(), name.as_str())) {
                    return Err("undeclared builtin value mapping".into());
                }
                if !type_set.contains(ty) || ty == TEXT {
                    return Err("unknown builtin value type".into());
                }
                if !mapped_builtins.insert((kind.as_str(), name.as_str())) {
                    return Err("builtin is both callable and a value".into());
                }
                values.push((
                    kind.as_str(),
                    format!(
                        "nitride_core::BuiltinValue {{kind:{},name:{},ty:{}}}",
                        quoted(kind),
                        quoted(name),
                        quoted(ty)
                    ),
                ));
            }
        }
    }
    if !syntax_only && mapped_builtins != builtin_declarations {
        return Err("missing builtin mapping".into());
    }
    let calls = data["catalog"]
        .get("calls")
        .map(|v| v.as_object().ok_or("calls must be an object"))
        .transpose()?;
    if let Some(calls) = calls {
        for (name, policy) in calls {
            let rule = &rules[lookup(&ids, name)? as usize];
            let clauses = array(&rule["clauses"])?;
            let args = strings(&policy["args"])?;
            if clauses.len() != 1
                || clauses[0]["kind"] != "References"
                || rule["lexical"] != false
                || args.iter().copied().collect::<HashSet<_>>().len() != args.len()
            {
                return Err(
                    "call mapping requires one reference clause and unique arguments".into(),
                );
            }
            let kinds = strings(&clauses[0]["symbol_kinds"])?;
            if !kinds
                .iter()
                .any(|kind| builtins.iter().any(|(k, _)| k == kind))
            {
                return Err("call has no builtin namespace".into());
            }
            if kinds
                .iter()
                .any(|kind| values.iter().any(|(k, _)| k == kind))
            {
                return Err("calls cannot reach builtin values".into());
            }
        }
    }
    let checks = data["catalog"]
        .get("checks")
        .map(|v| v.as_object().ok_or("checks must be an object"))
        .transpose()?;
    let mut check_tables = Vec::new();
    for (name, list) in checks.into_iter().flatten() {
        let id = lookup(&ids, name)?;
        let rule = &rules[id as usize];
        let mut labels = HashSet::new();
        collect_labels(&rule["expr"], &mut labels);
        if rule["lexical"] != false {
            return Err("checks require syntax rules".into());
        }
        for check in array(list)? {
            let (code, field, message) = (
                text(&check["code"])?,
                text(&check["field"])?,
                text(&check["message"])?,
            );
            if code.is_empty() || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
                return Err("invalid check code".into());
            }
            if message.is_empty() || !labels.contains(field) {
                return Err("check requires a rule field and message".into());
            }
            let predicate = match text(&check["predicate"])? {
                "iso-date" => "IsoDate",
                _ => return Err("unknown check predicate".into()),
            };
            check_tables.push(format!(
                "nitride_core::Check {{rule:{id},field:{},predicate:nitride_core::Predicate::{predicate},code:{},message:{}}}",
                quoted(field),
                quoted(code),
                quoted(message)
            ));
        }
    }
    if syntax_only && (!values.is_empty() || !check_tables.is_empty()) {
        return Err("syntax-only value or check semantics".into());
    }
    if syntax_only && (!builtins.is_empty() || calls.is_some_and(|m| !m.is_empty())) {
        return Err("syntax-only call semantics".into());
    }
    let template_mode = rules.iter().any(|r| {
        r["clauses"]
            .as_array()
            .is_some_and(|c| c.iter().any(|c| c["kind"] == "LowersTemplate"))
    });
    if template_mode
        && (!builtin_declarations.is_empty()
            || calls.is_some_and(|m| !m.is_empty())
            || !groups.is_empty()
            || !check_tables.is_empty()
            || operations.iter().any(|op| op["handler"] == "iso-date"))
    {
        return Err("template profile cannot mix builtin calls or overload policies".into());
    }
    // Quote helpers. Templates are parsed by the generated language at compile
    // time; here categories, names and arities are validated.
    let syntax_category = |name: &str| -> Result<u16, String> {
        let id = lookup(&ids, name)?;
        if rules[id as usize]["lexical"] != false {
            return Err("helper categories must be syntax rules".into());
        }
        Ok(id)
    };
    let hole = |p: &Value| -> Result<(String, u16), String> {
        Ok((
            text(&p["name"])?.to_owned(),
            syntax_category(text(&p["category"])?)?,
        ))
    };
    let emit_holes = |holes: &[(String, u16)]| {
        list(holes.iter().map(|(name, category)| {
            format!(
                "nitride_core::Hole {{name:{},category:{category}}}",
                quoted(name)
            )
        }))
    };
    let mut helper_ids = HashMap::new();
    let mut helper_signatures = Vec::new();
    let mut helper_tables = Vec::new();
    for helper in data
        .get("helpers")
        .map(array)
        .transpose()?
        .into_iter()
        .flatten()
    {
        let name = text(&helper["name"])?;
        let parameters = array(&helper["parameters"])?
            .iter()
            .map(hole)
            .collect::<Result<Vec<_>, _>>()?;
        let result = syntax_category(text(&helper["result"])?)?;
        let mut names: HashSet<_> = HashSet::new();
        if parameters.iter().any(|(n, _)| !names.insert(n.clone())) {
            return Err("duplicate helper parameter".into());
        }
        let (selector, arms) = if helper["match"].is_null() {
            ("None".to_owned(), Vec::new())
        } else {
            let selector = text(&helper["match"]["selector"])?;
            let index = parameters
                .iter()
                .position(|(n, _)| n == selector)
                .ok_or("match selector must be a parameter")?;
            let mut arms = Vec::new();
            for arm in array(&helper["match"]["arms"])? {
                let captures = array(&arm["captures"])?
                    .iter()
                    .map(hole)
                    .collect::<Result<Vec<_>, _>>()?;
                let mut taken = names.clone();
                if captures.iter().any(|(n, _)| !taken.insert(n.clone())) {
                    return Err("duplicate match capture".into());
                }
                arms.push(format!(
                    "nitride_core::Arm {{captures:{},pattern:{},result:{}}}",
                    emit_holes(&captures),
                    quoted(text(&arm["pattern"])?),
                    quoted(text(&arm["result"])?)
                ));
            }
            (format!("Some({index})"), arms)
        };
        if helper_ids
            .insert(name.to_owned(), helper_tables.len() as u16)
            .is_some()
        {
            return Err("duplicate helper".into());
        }
        helper_signatures.push((parameters.clone(), result));
        helper_tables.push(format!(
            "nitride_core::Helper {{name:{},parameters:{},result:{result},body:{},selector:{selector},arms:{}}}",
            quoted(name),
            emit_holes(&parameters),
            quoted(text(&helper["body"])?),
            list(arms)
        ));
    }
    if (syntax_only || template_mode) && !helper_tables.is_empty() {
        return Err("quote helpers require an ordinary typed profile".into());
    }
    let mut legacy_kind = None;
    for rule in rules {
        for clause in array(&rule["clauses"])? {
            if clause["kind"] == "Declares" || clause["kind"] == "References" {
                let kinds = strings(&clause["symbol_kinds"])?;
                if kinds.is_empty() || (clause["kind"] == "Declares" && kinds.len() != 1) {
                    return Err("invalid symbol kind cardinality".into());
                }
                if let Some(inventory) = &inventory {
                    if kinds.iter().any(|kind| !inventory.contains(kind)) {
                        return Err("unknown symbol namespace".into());
                    }
                } else {
                    // Legacy v1 exports lacked a declared-kind inventory. Keep
                    // their single-kind admission rather than infer namespaces.
                    if kinds.len() != 1 || legacy_kind.is_some_and(|kind| kind != kinds[0]) {
                        return Err("multiple or incompatible legacy symbol namespaces".into());
                    }
                    legacy_kind = Some(kinds[0]);
                }
            }
        }
    }
    let mut output_rules = Vec::new();
    for rule in rules {
        let name = text(&rule["name"])?;
        let mut labels = HashSet::new();
        collect_labels(&rule["expr"], &mut labels);
        let field = |s: &str| -> Result<String, String> {
            if !labels.contains(s) {
                return Err(format!("unknown field {name}.{s}"));
            }
            Ok(quoted(s))
        };
        let clauses = array(&rule["clauses"])?;
        if template_mode && rule["lexical"] == true && !clauses.is_empty() {
            return Err("template semantic actions require syntax rules".into());
        }
        let special = if template_mode {
            template_action(clauses, &field)?
        } else {
            None
        };
        let ordinary = if special.is_some() {
            &[][..]
        } else {
            clauses.as_slice()
        };
        let mut action = special.unwrap_or_else(|| "Action::None".to_owned());
        for clause in ordinary {
            if clause["export"] == true {
                return Err("unsupported exported declaration".into());
            }
            if action != "Action::None" {
                return Err("multiple semantic actions unsupported".into());
            }
            action = match text(&clause["kind"])? {
                "LowersLiteral" => {
                    let ty = text(&clause["target"])?;
                    if !type_set.contains(ty) || ty == TEXT {
                        return Err("unknown literal type".into());
                    }
                    format!(
                        "Action::Literal {{ty:{},field:{}}}",
                        quoted(ty),
                        field(text(&clause["field"])?)?
                    )
                }
                "References" => {
                    let kinds = strings(&clause["symbol_kinds"])?;
                    let admitted: Vec<_> = builtins
                        .iter()
                        .filter(|(kind, _)| kinds.contains(kind))
                        .map(|(_, code)| code.clone())
                        .collect();
                    let visible_values: Vec<_> = values
                        .iter()
                        .filter(|(kind, _)| kinds.contains(kind))
                        .map(|(_, code)| code.clone())
                        .collect();
                    if let Some(policy) = calls.and_then(|m| m.get(name)) {
                        format!(
                            "Action::Call {{field:{},kinds:{},fields:{},builtins:{}}}",
                            field(text(&clause["field"])?)?,
                            list(kinds.into_iter().map(quoted)),
                            list(
                                strings(&policy["args"])?
                                    .into_iter()
                                    .map(field)
                                    .collect::<Result<Vec<_>, _>>()?
                            ),
                            list(admitted)
                        )
                    } else {
                        if !admitted.is_empty() {
                            return Err("builtin references require a call mapping".into());
                        }
                        format!(
                            "Action::Reference {{field:{},kinds:{},values:{}}}",
                            field(text(&clause["field"])?)?,
                            list(kinds.into_iter().map(quoted)),
                            list(visible_values)
                        )
                    }
                }
                "Lowers" => {
                    let target = text(&clause["target"])?;
                    let args = strings(&clause["args"])?;
                    let fields = args
                        .iter()
                        .map(|arg| field(arg))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut texts = vec![false; args.len()];
                    if let Some(positions) = clause.get("text_args") {
                        for position in array(positions)? {
                            let index = position
                                .as_u64()
                                .filter(|i| (*i as usize) < args.len())
                                .ok_or("invalid text argument position")?;
                            if std::mem::replace(&mut texts[index as usize], true) {
                                return Err("duplicate text argument position".into());
                            }
                        }
                        if !texts.contains(&true) {
                            return Err("empty text argument positions".into());
                        }
                    }
                    if let Some(candidates) = groups.get(target) {
                        if texts.contains(&true) {
                            return Err("overloads cannot take text arguments".into());
                        }
                        if !candidates.iter().any(|id| {
                            operations[*id as usize]["inputs"]
                                .as_array()
                                .is_some_and(|a| a.len() == args.len())
                        }) {
                            return Err("no overload has the required arity".into());
                        }
                        format!(
                            "Action::Overload {{candidates:{},fields:{}}}",
                            list(candidates.iter().map(u16::to_string)),
                            list(fields)
                        )
                    } else {
                        let id = lookup(&op_ids, target)?;
                        let inputs = strings(&operations[id as usize]["inputs"])?;
                        if args.len() != inputs.len() {
                            return Err("operation arity mismatch".into());
                        }
                        if inputs
                            .iter()
                            .zip(&texts)
                            .any(|(ty, text)| (*ty == TEXT) != *text)
                        {
                            return Err("text arguments must match Core.Text inputs".into());
                        }
                        format!(
                            "Action::Operation {{id:{id},fields:{},texts:{}}}",
                            list(fields),
                            if texts.contains(&true) {
                                list(texts.iter().map(bool::to_string))
                            } else {
                                "&[]".into()
                            }
                        )
                    }
                }
                "Rewrites" => {
                    if syntax_only || template_mode {
                        return Err("rewrites require an ordinary typed profile".into());
                    }
                    let owner = rule["owner"]
                        .as_str()
                        .ok_or("rewrites require an extensible alternative")?;
                    let owner = lookup(&ids, owner)?;
                    // A labeled field's category: its direct reference to a syntax rule.
                    let category_of = |label: &str| -> Option<u16> {
                        fn find<'a>(e: &'a Value, label: &str) -> Option<&'a Value> {
                            if e["kind"] == "label" && e["label"] == label {
                                return Some(&e["inner"]);
                            }
                            e["items"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .chain(e.get("inner"))
                                .find_map(|c| find(c, label))
                        }
                        let inner = find(&rule["expr"], label)?;
                        let id = *ids.get(text(&inner["name"]).ok()?)?;
                        (inner["kind"] == "ref" && rules[id as usize]["lexical"] == false)
                            .then_some(id)
                    };
                    let (helper, fields) = if clause["target"].is_null() {
                        let mut labels: Vec<_> = labels.iter().copied().collect();
                        labels.sort_unstable();
                        let holes: Vec<_> = labels
                            .into_iter()
                            .filter_map(|l| category_of(l).map(|c| (l.to_owned(), c)))
                            .collect();
                        let fields = holes.iter().map(|(l, _)| quoted(l)).collect::<Vec<_>>();
                        helper_signatures.push((holes.clone(), owner));
                        helper_tables.push(format!(
                            "nitride_core::Helper {{name:{},parameters:{},result:{owner},body:{},selector:None,arms:&[]}}",
                            quoted(name),
                            emit_holes(&holes),
                            quoted(text(&clause["template"])?)
                        ));
                        (helper_tables.len() as u16 - 1, fields)
                    } else {
                        let helper = *helper_ids
                            .get(text(&clause["target"])?)
                            .ok_or("unknown rewrite helper")?;
                        let (parameters, result) = &helper_signatures[helper as usize];
                        let args = strings(&clause["args"])?;
                        if *result != owner
                            || args.len() != parameters.len()
                            || args
                                .iter()
                                .zip(parameters)
                                .any(|(a, (_, c))| category_of(a) != Some(*c))
                        {
                            return Err("rewrite helper categories must match exactly".into());
                        }
                        (
                            helper,
                            args.into_iter().map(field).collect::<Result<_, _>>()?,
                        )
                    };
                    format!(
                        "Action::Rewrite {{helper:{helper},fields:{},name:{}}}",
                        list(fields),
                        quoted(name.rsplit('.').next().unwrap())
                    )
                }
                "Declares" => {
                    if template_mode {
                        return Err("sequential globals are outside template profile".into());
                    }
                    if clause["sequential"] != true {
                        return Err("non-sequential declaration unsupported".into());
                    }
                    let statement = &data["catalog"]["statements"][name];
                    if statement["kind"] != "let" {
                        return Err("missing declaration initializer mapping".into());
                    }
                    let infer = statement
                        .get("infer")
                        .map(|v| v.as_bool().ok_or("infer policy must be boolean"))
                        .transpose()?
                        .unwrap_or(false);
                    let annotation = if clause["target"].is_null() {
                        if !infer {
                            return Err(
                                "untyped declaration requires explicit infer:true policy".into()
                            );
                        }
                        "None".to_owned()
                    } else {
                        if infer {
                            return Err("explicit annotation conflicts with infer policy".into());
                        }
                        format!("Some({})", field(text(&clause["target"])?)?)
                    };
                    format!(
                        "Action::Declare {{kind:{},field:{},annotation:{},value:{}}}",
                        quoted(text(&clause["symbol_kinds"][0])?),
                        field(text(&clause["field"])?)?,
                        annotation,
                        field(text(&statement["value"])?)?
                    )
                }
                _ => return Err("unsupported semantic clause".into()),
            };
        }
        if data["catalog"]["statements"][name]["kind"] == "show" {
            if action != "Action::None" {
                return Err("conflicting statement action".into());
            }
            action = format!(
                "Action::Show {{field:{}}}",
                field(text(&data["catalog"]["statements"][name]["value"])?)?
            );
        }
        let owner = if rule["owner"].is_null() {
            "None".into()
        } else {
            format!("Some({})", lookup(&ids, text(&rule["owner"])?)?)
        };
        let precedence = rule["precedence"].as_u64().unwrap_or(0);
        if precedence > u32::MAX as u64 - 1 {
            return Err("precedence out of range".into());
        }
        let alias = syntax_only && name != data["start"] && alias_rule(rule);
        output_rules.push(format!("Rule {{name:{},lexical:{},expr:{},except:{},precedence:{precedence},right_associative:{},transparent:{},alias:{alias},owner:{owner},action:{action}}}",
            quoted(name), rule["lexical"].as_bool().ok_or("invalid lexical flag")?, emit_expr_with(&pair_surrogates(&rule["expr"]), &ids, rule["expr"]["kind"] == "pratt" && memoize_point(&rule["expr"], rules, &ids))?,
            list(strings(&rule["except"])?.into_iter().map(quoted)), rule["associativity"] == "Right", rule["transparent"] == true));
    }

    fn bare(mut expr: &Value) -> &Value {
        while expr["kind"] == "label" {
            expr = &expr["inner"];
        }
        expr
    }
    for point in rules.iter().filter(|r| r["expr"]["kind"] == "pratt") {
        for name in strings(&point["expr"]["infixes"])? {
            let candidate = &rules[lookup(&ids, name)? as usize];
            let parts = array(&candidate["expr"]["items"])?;
            if parts.len() < 2
                || bare(&parts[0])["kind"] != "ref"
                || bare(&parts[0])["name"] != point["name"]
                || candidate["owner"] != point["name"]
            {
                return Err("invalid infix target".into());
            }
            // Alternatives may share an operator: the parser ranks them by longest
            // match, then precedence, as Nitrogen does.
        }
    }
    validate_progress(rules, &ids)?;
    if let Some(statements) = data["catalog"]["statements"].as_object() {
        for (name, statement) in statements {
            if template_mode && statement["kind"] != "show" {
                return Err("unsupported statement policy in template profile".into());
            }
            let rule = &rules[lookup(&ids, name)? as usize];
            if let Some(infer) = statement.get("infer") {
                let infer = infer.as_bool().ok_or("infer policy must be boolean")?;
                if infer
                    && (statement["kind"] != "let"
                        || !array(&rule["clauses"])?
                            .iter()
                            .any(|c| c["kind"] == "Declares" && c["target"].is_null()))
                {
                    return Err("infer policy requires an unannotated declaration".into());
                }
            }
            if statement["kind"] != "let" && statement["kind"] != "show" {
                return Err("unsupported statement mapping".into());
            }
        }
    }
    let lexers = lexical::generate(rules, &ids)?;
    let parser = syntax::generate(rules, &ids)?;
    Ok(format!(
        "use nitride_core::{{Language, Rule, Expr, Action, Profile}};\npub static LANGUAGE: Language = Language {{rules:{},start:{start},types:{},operations:{},handlers:{},checks:{},helpers:{},source_hash:{},profile:Profile::{}}};\n{lexers}{parser}",
        list(output_rules),
        list(types.into_iter().map(quoted)),
        list(signatures),
        list(handlers),
        list(check_tables),
        list(helper_tables),
        quoted(text(&data["source_hash"])?),
        if syntax_only { "SyntaxOnly" } else { "Typed" }
    ))
}

fn template_action(
    clauses: &[Value],
    field: &impl Fn(&str) -> Result<String, String>,
) -> Result<Option<String>, String> {
    for clause in clauses {
        let kind = text(&clause["kind"])?;
        if matches!(
            kind,
            "Scope" | "LowersTemplate" | "LowersExpand" | "References"
        ) && (!clause["target"].is_null()
            || clause["sequential"] != false
            || clause["export"] == true)
        {
            return Err("invalid template clause flags".into());
        }
        if kind == "Scope"
            && (clause["field"] != ""
                || !array(&clause["args"])?.is_empty()
                || !array(&clause["symbol_kinds"])?.is_empty())
        {
            return Err("invalid scope clause".into());
        }
        if matches!(kind, "LowersTemplate" | "LowersExpand")
            && !array(&clause["symbol_kinds"])?.is_empty()
        {
            return Err("invalid lowering kinds".into());
        }
    }
    let find = |kind: &str| clauses.iter().find(|c| c["kind"] == kind);
    if let Some(template) = find("LowersTemplate") {
        let declaration = find("Declares").ok_or("template needs declaration")?;
        if clauses.len() != 3
            || find("Scope").is_none()
            || declaration["sequential"] != false
            || declaration["export"] != true
            || !declaration["target"].is_null()
            || strings(&declaration["symbol_kinds"])?.len() != 1
            || strings(&template["args"])?.len() != 1
        {
            return Err("invalid template definition clauses".into());
        }
        return Ok(Some(format!(
            "Action::Template {{kind:{},field:{},body:{},parameters:{}}}",
            quoted(text(&declaration["symbol_kinds"][0])?),
            field(text(&declaration["field"])?)?,
            field(text(&template["field"])?)?,
            field(text(&template["args"][0])?)?
        )));
    }
    if let Some(expand) = find("LowersExpand") {
        let reference = find("References").ok_or("expansion needs reference")?;
        if clauses.len() != 2
            || !array(&reference["args"])?.is_empty()
            || expand["field"] != reference["field"]
            || strings(&expand["args"])?.len() != 1
        {
            return Err("invalid expansion clauses".into());
        }
        return Ok(Some(format!(
            "Action::Expand {{kinds:{},field:{},arguments:{}}}",
            list(strings(&reference["symbol_kinds"])?.into_iter().map(quoted)),
            field(text(&reference["field"])?)?,
            field(text(&expand["args"][0])?)?
        )));
    }
    if let Some(declaration) = find("Declares").filter(|c| c["sequential"] == false) {
        if clauses.len() != 1 || strings(&declaration["symbol_kinds"])?.len() != 1 {
            return Err("invalid template-profile declaration".into());
        }
        let kind = quoted(text(&declaration["symbol_kinds"][0])?);
        let name = field(text(&declaration["field"])?)?;
        return if declaration["target"].is_null() {
            if declaration["export"] != true {
                return Err("alias needs export".into());
            }
            Ok(Some(format!("Action::Alias {{kind:{kind},field:{name}}}")))
        } else {
            if declaration["export"] == true {
                return Err("parameter cannot export".into());
            }
            Ok(Some(format!(
                "Action::Parameter {{kind:{kind},field:{name},annotation:{}}}",
                field(text(&declaration["target"])?)?
            )))
        };
    }
    Ok(None)
}

fn lookup(ids: &HashMap<&str, u16>, name: &str) -> Result<u16, String> {
    ids.get(name)
        .copied()
        .ok_or_else(|| format!("unknown identity {name}"))
}

fn collect_labels<'a>(e: &'a Value, labels: &mut HashSet<&'a str>) {
    if let Some(label) = e["label"].as_str() {
        labels.insert(label);
    }
    if let Some(items) = e["items"].as_array() {
        for item in items {
            collect_labels(item, labels);
        }
    }
    if e.get("inner").is_some() {
        collect_labels(&e["inner"], labels);
    }
}

const HIGH: [u32; 2] = [0xD800, 0xDBFF];
const LOW: [u32; 2] = [0xDC00, 0xDFFF];

fn class_ranges(e: &Value) -> Option<Vec<[u32; 2]>> {
    if e["kind"] != "class" || e["negated"] == true {
        return None;
    }
    e["ranges"]
        .as_array()?
        .iter()
        .map(|r| Some([r[0].as_u64()? as u32, r[1].as_u64()? as u32]))
        .collect()
}

fn clip(ranges: &[[u32; 2]], bounds: [u32; 2]) -> Vec<[u32; 2]> {
    ranges
        .iter()
        .filter_map(|[a, b]| {
            let (a, b) = ((*a).max(bounds[0]), (*b).min(bounds[1]));
            (a <= b).then_some([a, b])
        })
        .collect()
}

/// Gives Nitrogen's UTF-16 classes their meaning over UTF-8 text. In a
/// sequence, a class with high surrogates followed by a class of only low
/// surrogates matches the supplementary code points of every pair (a BMP
/// character cannot be followed by a lone low surrogate in valid text).
/// Any other surrogate unit can never occur in UTF-8 text and is removed; a
/// class left empty matches nothing.
pub fn pair_surrogates(e: &Value) -> Value {
    let mut e = e.clone();
    let sequence = e["kind"] == "sequence";
    if let Some(items) = e.get_mut("items").and_then(Value::as_array_mut) {
        let paired: Vec<Value> = items.iter().map(pair_surrogates).collect();
        let mut out = Vec::with_capacity(paired.len());
        let mut index = 0;
        while index < paired.len() {
            if sequence && index + 1 < paired.len() {
                if let (Some(first), Some(second)) =
                    (class_ranges(&items[index]), class_ranges(&items[index + 1]))
                {
                    let high = clip(&first, HIGH);
                    let low = clip(&second, LOW);
                    if !high.is_empty()
                        && !low.is_empty()
                        && clip(&second, [0, 0xDBFF]).is_empty()
                        && clip(&second, [0xE000, u32::MAX]).is_empty()
                    {
                        let mut ranges = Vec::new();
                        for [a, b] in &high {
                            for h in *a..=*b {
                                let base = 0x10000 + (h - 0xD800) * 0x400;
                                for [l, m] in &low {
                                    ranges.push(serde_json::json!([
                                        base + l - 0xDC00,
                                        base + m - 0xDC00
                                    ]));
                                }
                            }
                        }
                        out.push(
                            serde_json::json!({"kind":"class","negated":false,"ranges":ranges}),
                        );
                        index += 2;
                        continue;
                    }
                }
            }
            out.push(paired[index].clone());
            index += 1;
        }
        *items = out;
        return e;
    }
    if let Some(inner) = e.get("inner") {
        e["inner"] = pair_surrogates(inner);
    }
    if e["kind"] == "class" {
        let ranges: Vec<Value> = e["ranges"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|r| {
                let (a, b) = (
                    r[0].as_u64().unwrap_or(0) as u32,
                    r[1].as_u64().unwrap_or(0) as u32,
                );
                [[a, b.min(0xD7FF)], [a.max(0xE000), b]]
                    .into_iter()
                    .filter(|[x, y]| x <= y)
                    .map(|[x, y]| serde_json::json!([x, y]))
                    .collect::<Vec<_>>()
            })
            .collect();
        e["ranges"] = Value::Array(ranges);
    }
    e
}

/// How an expression can begin: a whole leading literal or a character range.
#[derive(Clone)]
enum Start {
    Text(String),
    Range(u32, u32),
}

type Chars = Vec<Start>;

fn starts_overlap(a: &Start, b: &Start) -> bool {
    let first = |t: &str| t.chars().next().map_or(0, |c| c as u32);
    match (a, b) {
        (Start::Text(a), Start::Text(b)) => a.starts_with(b.as_str()) || b.starts_with(a.as_str()),
        (Start::Text(t), Start::Range(x, y)) | (Start::Range(x, y), Start::Text(t)) => {
            (*x..=*y).contains(&first(t))
        }
        (Start::Range(x, y), Start::Range(p, q)) => x <= q && p <= y,
    }
}

/// How an expression can begin; None when unknown, nullable, negated or
/// predicate-led (conservatively overlapping everything).
fn first_chars(
    e: &Value,
    rules: &[Value],
    ids: &HashMap<&str, u16>,
    visiting: &mut HashSet<u16>,
) -> Option<Chars> {
    match e["kind"].as_str()? {
        "literal" => {
            let text = e["text"].as_str()?;
            if text.is_empty() {
                return None;
            }
            Some(vec![Start::Text(text.to_owned())])
        }
        "class" if e["negated"] != true => Some(
            e["ranges"]
                .as_array()?
                .iter()
                .map(|r| Some(Start::Range(r[0].as_u64()? as u32, r[1].as_u64()? as u32)))
                .collect::<Option<_>>()?,
        ),
        "label" => first_chars(&e["inner"], rules, ids, visiting),
        "repeat" if e["min"].as_u64()? >= 1 => first_chars(&e["inner"], rules, ids, visiting),
        "sequence" => first_chars(e["items"].as_array()?.first()?, rules, ids, visiting),
        "choice" => {
            let mut all = Vec::new();
            for item in e["items"].as_array()? {
                all.extend(first_chars(item, rules, ids, visiting)?);
            }
            Some(all)
        }
        "ref" => {
            let id = *ids.get(e["name"].as_str()?)?;
            if !visiting.insert(id) {
                return None;
            }
            let result = first_chars(&rules[id as usize]["expr"], rules, ids, visiting);
            visiting.remove(&id);
            result
        }
        "pratt" => {
            let mut all = Vec::new();
            for name in e["prefixes"].as_array()? {
                let id = *ids.get(name.as_str()?)?;
                if !visiting.insert(id) {
                    return None;
                }
                let part = first_chars(&rules[id as usize]["expr"], rules, ids, visiting);
                visiting.remove(&id);
                all.extend(part?);
            }
            Some(all)
        }
        _ => None,
    }
}

/// Whether `e` can reach an extension point through references.
fn reaches_point(
    e: &Value,
    rules: &[Value],
    ids: &HashMap<&str, u16>,
    seen: &mut HashSet<u16>,
) -> bool {
    match e {
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some("ref") {
                let Some(&id) = map
                    .get("name")
                    .and_then(Value::as_str)
                    .and_then(|n| ids.get(n))
                else {
                    return false;
                };
                let expr = &rules[id as usize]["expr"];
                return expr["kind"] == "pratt"
                    || (seen.insert(id) && reaches_point(expr, rules, ids, seen));
            }
            map.values().any(|v| reaches_point(v, rules, ids, seen))
        }
        Value::Array(items) => items.iter().any(|v| reaches_point(v, rules, ids, seen)),
        _ => false,
    }
}

/// Whether two alternatives of an extension point can begin alike while both
/// parse a nested expression (reach any extension point): prefixes by how they start, infixes by their
/// operators. Only then can a failed alternative's operand be reparsed by the
/// next (exponentially, when nested), so only then is memoization worth its cost.
fn memoize_point(e: &Value, rules: &[Value], ids: &HashMap<&str, u16>) -> bool {
    let sets = |key: &str, element: usize| -> Vec<Option<Chars>> {
        e[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|name| {
                let Some(&id) = name.as_str().and_then(|n| ids.get(n)) else {
                    return Some(None);
                };
                let rule = &rules[id as usize];
                // An infix always parses its operands; a prefix may not.
                if element == 0 && !reaches_point(&rule["expr"], rules, ids, &mut HashSet::new()) {
                    return None;
                }
                let expr = if element == 0 {
                    &rule["expr"]
                } else {
                    &rule["expr"]["items"][element]
                };
                Some(first_chars(expr, rules, ids, &mut HashSet::new()))
            })
            .collect()
    };
    let overlap = |group: Vec<Option<Chars>>| {
        group.iter().enumerate().any(|(i, a)| {
            group[i + 1..].iter().any(|b| match (a, b) {
                (Some(a), Some(b)) => a.iter().any(|x| b.iter().any(|y| starts_overlap(x, y))),
                _ => true,
            })
        })
    };
    overlap(sets("prefixes", 0)) || overlap(sets("infixes", 1))
}

/// Nitrogen's EmitModel.IsAlias: a syntax rule that is a reference (possibly
/// labeled) or a choice of them, or a plain rule of literals and references,
/// builds no node. Each matches at most one child node.
fn alias_rule(rule: &Value) -> bool {
    fn reference(e: &Value) -> bool {
        match e["kind"].as_str() {
            Some("ref") => true,
            Some("label") => reference(&e["inner"]),
            Some("choice") => e["items"]
                .as_array()
                .is_some_and(|items| !items.is_empty() && items.iter().all(reference)),
            _ => false,
        }
    }
    fn plain(e: &Value) -> bool {
        let leaf = |e: &Value| matches!(e["kind"].as_str(), Some("ref" | "literal"));
        leaf(e)
            || (e["kind"] == "choice"
                && e["items"]
                    .as_array()
                    .is_some_and(|items| items.iter().all(leaf)))
    }
    rule["lexical"] != true
        && rule["transparent"] != true
        && (reference(&rule["expr"]) || (rule["owner"].is_null() && plain(&rule["expr"])))
}

fn emit_expr(e: &Value, ids: &HashMap<&str, u16>) -> Result<String, String> {
    emit_expr_with(e, ids, false)
}

fn emit_expr_with(e: &Value, ids: &HashMap<&str, u16>, memoize: bool) -> Result<String, String> {
    Ok(match text(&e["kind"])? {
        "literal" => format!("Expr::Literal({})", quoted(text(&e["text"])?)),
        "any" => "Expr::Any".into(),
        "predicate" => format!(
            "Expr::Predicate {{positive:{},inner:&{}}}",
            e["positive"].as_bool().ok_or("invalid predicate mode")?,
            emit_expr(&e["inner"], ids)?
        ),
        "ref" => format!("Expr::Ref({})", lookup(ids, text(&e["name"])?)?),
        "class" => {
            let mut ranges = Vec::new();
            for range in array(&e["ranges"])? {
                let pair = array(range)?;
                if pair.len() != 2 {
                    return Err("invalid character range".into());
                }
                let a = pair[0]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .and_then(char::from_u32)
                    .ok_or("invalid character")?;
                let b = pair[1]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .and_then(char::from_u32)
                    .ok_or("invalid character")?;
                if a > b {
                    return Err("inverted character range".into());
                }
                ranges.push(format!("({a:?},{b:?})"));
            }
            format!(
                "Expr::{}({})",
                if e["negated"] == true {
                    "NotClass"
                } else {
                    "Class"
                },
                list(ranges)
            )
        }
        "sequence" | "choice" => {
            let items = array(&e["items"])?
                .iter()
                .map(|i| emit_expr(i, ids))
                .collect::<Result<Vec<_>, _>>()?;
            let form = if e["kind"] == "sequence" {
                "Sequence"
            } else {
                "Choice"
            };
            format!("Expr::{form}({})", list(items))
        }
        "label" => format!(
            "Expr::Label({},&{})",
            quoted(text(&e["label"])?),
            emit_expr(&e["inner"], ids)?
        ),
        "repeat" => {
            let min = e["min"].as_u64().ok_or("invalid repeat min")?;
            let max = e["max"].as_u64().ok_or("invalid repeat max")?;
            if min > max || max > u32::MAX as u64 {
                return Err("invalid repetition bounds".into());
            }
            format!(
                "Expr::Repeat {{inner:&{},min:{min},max:{max}}}",
                emit_expr(&e["inner"], ids)?
            )
        }
        "pratt" => {
            let refs = |key: &str| -> Result<String, String> {
                Ok(list(
                    strings(&e[key])?
                        .into_iter()
                        .map(|n| lookup(ids, n).map(|id| id.to_string()))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            };
            format!(
                "Expr::Pratt {{prefixes:{},infixes:{},memoize:{memoize}}}",
                refs("prefixes")?,
                refs("infixes")?
            )
        }
        _ => return Err("unsupported grammar expression".into()),
    })
}

fn nullable(e: &Value, empty: &[bool], ids: &HashMap<&str, u16>) -> bool {
    match e["kind"].as_str().unwrap_or("") {
        "literal" => e["text"] == "",
        "predicate" => true,
        "ref" => ids
            .get(e["name"].as_str().unwrap_or(""))
            .is_some_and(|i| empty[*i as usize]),
        "label" => nullable(&e["inner"], empty, ids),
        "repeat" => e["min"] == 0 || nullable(&e["inner"], empty, ids),
        "sequence" => e["items"]
            .as_array()
            .is_some_and(|a| a.iter().all(|i| nullable(i, empty, ids))),
        "choice" => e["items"]
            .as_array()
            .is_some_and(|a| a.iter().any(|i| nullable(i, empty, ids))),
        "pratt" => e["prefixes"].as_array().is_some_and(|a| {
            a.iter().any(|i| {
                ids.get(i.as_str().unwrap_or(""))
                    .is_some_and(|id| empty[*id as usize])
            })
        }),
        _ => false,
    }
}

fn validate_progress(rules: &[Value], ids: &HashMap<&str, u16>) -> Result<(), String> {
    let mut empty = vec![false; rules.len()];
    loop {
        let next: Vec<_> = rules
            .iter()
            .map(|r| nullable(&r["expr"], &empty, ids))
            .collect();
        if next == empty {
            break;
        }
        empty = next;
    }
    for (index, rule) in rules.iter().enumerate() {
        if rule["lexical"] == true && empty[index] {
            return Err(format!(
                "nullable token {} is outside admission",
                text(&rule["name"])?
            ));
        }
        if rule["expr"]["kind"] == "pratt" {
            for name in strings(&rule["expr"]["prefixes"])? {
                if empty[lookup(ids, name)? as usize] {
                    return Err(format!("nullable extensible prefix {name}"));
                }
            }
        }
    }
    fn edges(
        e: &Value,
        empty: &[bool],
        ids: &HashMap<&str, u16>,
        out: &mut Vec<u16>,
    ) -> Result<(), String> {
        match text(&e["kind"])? {
            "ref" => out.push(lookup(ids, text(&e["name"])?)?),
            "label" | "repeat" | "predicate" => {
                if e["kind"] == "repeat" && e["max"] != 1 && nullable(&e["inner"], empty, ids) {
                    return Err("zero-width repetition unsupported".into());
                }
                edges(&e["inner"], empty, ids, out)?;
            }
            "sequence" => {
                for item in array(&e["items"])? {
                    edges(item, empty, ids, out)?;
                    if !nullable(item, empty, ids) {
                        break;
                    }
                }
            }
            "choice" => {
                for item in array(&e["items"])? {
                    edges(item, empty, ids, out)?;
                }
            }
            "pratt" => {
                for name in strings(&e["prefixes"])? {
                    out.push(lookup(ids, name)?);
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut graph = Vec::new();
    fn check_repeats(
        e: &Value,
        empty: &[bool],
        ids: &HashMap<&str, u16>,
        lexical: bool,
    ) -> Result<(), String> {
        if e["kind"] == "repeat" && e["max"] != 1 && nullable(&e["inner"], empty, ids) {
            return Err("zero-width repetition unsupported".into());
        }
        if let Some(items) = e["items"].as_array() {
            if e["kind"] == "choice" {
                for (index, later) in items.iter().enumerate().skip(1) {
                    for earlier in &items[..index] {
                        let mut shadows = nullable(earlier, empty, ids);
                        if earlier["kind"] == "literal" && later["kind"] == "literal" {
                            let a = text(&earlier["text"])?;
                            let b = text(&later["text"])?;
                            let keyword = a
                                .bytes()
                                .next()
                                .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
                                && a.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_');
                            let boundary = !lexical
                                && keyword
                                && b.len() > a.len()
                                && b.as_bytes()
                                    .get(a.len())
                                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_');
                            shadows |= b.starts_with(a) && !boundary;
                        }
                        if shadows {
                            return Err("unreachable ordered choice alternative".into());
                        }
                    }
                }
            }
            for item in items {
                check_repeats(item, empty, ids, lexical)?;
            }
        }
        if let Some(inner) = e.get("inner") {
            check_repeats(inner, empty, ids, lexical)?;
        }
        Ok(())
    }
    for rule in rules {
        check_repeats(&rule["expr"], &empty, ids, rule["lexical"] == true)?;
        let mut row = Vec::new();
        edges(&rule["expr"], &empty, ids, &mut row)?;
        graph.push(row);
    }
    fn visit(n: usize, graph: &[Vec<u16>], state: &mut [u8]) -> Result<(), String> {
        if state[n] == 1 {
            return Err("left recursion unsupported".into());
        }
        if state[n] == 2 {
            return Ok(());
        }
        state[n] = 1;
        for next in &graph[n] {
            visit(*next as usize, graph, state)?;
        }
        state[n] = 2;
        Ok(())
    }
    let mut state = vec![0; rules.len()];
    for n in 0..rules.len() {
        visit(n, &graph, &mut state)?;
    }
    Ok(())
}
