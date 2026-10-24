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
            list(inputs.into_iter().map(quoted)),
            quoted(result)
        ));
        let handler = text(&op["handler"])?;
        let arity = match handler {
            "zero" => 0,
            "identity" | "neg" => 1,
            "add" | "sub" | "mul" | "div" | "pow" => 2,
            _ => return Err("unknown fixture host handler".into()),
        };
        if array(&op["inputs"])?.len() != arity {
            return Err("fixture host handler arity mismatch".into());
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
            if !strings(&clauses[0]["symbol_kinds"])?
                .iter()
                .any(|kind| builtins.iter().any(|(k, _)| k == kind))
            {
                return Err("call has no builtin namespace".into());
            }
        }
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
            || !groups.is_empty())
    {
        return Err("template profile cannot mix builtin calls or overload policies".into());
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
                    if !type_set.contains(ty) {
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
                            "Action::Reference {{field:{},kinds:{}}}",
                            field(text(&clause["field"])?)?,
                            list(kinds.into_iter().map(quoted))
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
                    if let Some(candidates) = groups.get(target) {
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
                        if args.len() != array(&operations[id as usize]["inputs"])?.len() {
                            return Err("operation arity mismatch".into());
                        }
                        format!("Action::Operation {{id:{id},fields:{}}}", list(fields))
                    }
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
        output_rules.push(format!("Rule {{name:{},lexical:{},expr:{},except:{},precedence:{precedence},right_associative:{},owner:{owner},action:{action}}}",
            quoted(name), rule["lexical"].as_bool().ok_or("invalid lexical flag")?, emit_expr(&rule["expr"], &ids)?,
            list(strings(&rule["except"])?.into_iter().map(quoted)), rule["associativity"] == "Right"));
    }

    fn bare(mut expr: &Value) -> &Value {
        while expr["kind"] == "label" {
            expr = &expr["inner"];
        }
        expr
    }
    for point in rules.iter().filter(|r| r["expr"]["kind"] == "pratt") {
        let mut operators = HashSet::new();
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
            let operator = bare(&parts[1]);
            if operator["kind"] != "literal" {
                return Err("infix must start with a literal operator".into());
            }
            if !operators.insert(text(&operator["text"])?) {
                return Err("conflicting extension operator".into());
            }
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
    Ok(format!(
        "use nitride_core::{{Language, Rule, Expr, Action, Profile}};\npub static LANGUAGE: Language = Language {{rules:{},start:{start},types:{},operations:{},handlers:{},source_hash:{},profile:Profile::{}}};\n",
        list(output_rules),
        list(types.into_iter().map(quoted)),
        list(signatures),
        list(handlers),
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

fn emit_expr(e: &Value, ids: &HashMap<&str, u16>) -> Result<String, String> {
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
                "Expr::Pratt {{prefixes:{},infixes:{}}}",
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
