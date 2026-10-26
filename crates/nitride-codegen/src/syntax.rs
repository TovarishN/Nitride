//! Structural syntax specialization. Ranking and storage stay in the runtime.
use super::*;

pub(super) fn generate(rules: &[Value], ids: &HashMap<&str, u16>) -> Result<String, String> {
    fn bounded(e: &Value, depth: usize, remaining: &mut usize, global: &mut usize) -> bool {
        if depth > 64 || *remaining == 0 || *global == 0 {
            return false;
        }
        *remaining -= 1;
        *global -= 1;
        match e["kind"].as_str().unwrap_or("") {
            "sequence" | "choice" => e["items"]
                .as_array()
                .is_some_and(|a| a.iter().all(|e| bounded(e, depth + 1, remaining, global))),
            "label" | "repeat" | "predicate" => bounded(&e["inner"], depth + 1, remaining, global),
            _ => true,
        }
    }
    let mut infixes = HashSet::new();
    for r in rules.iter().filter(|r| r["expr"]["kind"] == "pratt") {
        for name in strings(&r["expr"]["infixes"])? {
            infixes.insert(lookup(ids, name)? as usize);
        }
    }
    let mut bodies = vec!["None".to_owned(); rules.len()];
    let mut tails = bodies.clone();
    let mut output = String::new();
    let mut global = 1_000_000;
    for (id, r) in rules.iter().enumerate() {
        if r["lexical"] == true || r["expr"]["kind"] == "pratt" {
            continue;
        }
        let e = pair_surrogates(&r["expr"]);
        if !bounded(&e, 0, &mut 4096, &mut global) {
            continue;
        }
        let owner = if r["owner"].is_null() {
            None
        } else {
            Some(lookup(ids, text(&r["owner"])?)?)
        };
        if !matches!(
            e["kind"].as_str(),
            Some("choice" | "predicate" | "class" | "any")
        ) {
            let body = if owner.is_some() && e["kind"] == "sequence" {
                emit_parts(array(&e["items"])?, ids, owner, 0, &mut 0)?
            } else {
                emit(&e, ids, owner, 0, &mut 0)?
            };
            output += &function(&format!("nitride_syntax_{id}"), &body);
            bodies[id] = format!("Some(nitride_syntax_{id})");
        }
        if infixes.contains(&id) && e["kind"] == "sequence" {
            let parts = array(&e["items"])?;
            if parts.len() >= 2 {
                let body = emit_parts(&parts[1..], ids, owner, 0, &mut 0)?;
                output += &function(&format!("nitride_tail_{id}"), &body);
                tails[id] = format!("Some(nitride_tail_{id})");
            }
        }
    }
    output += &format!(
        "#[allow(dead_code)]\npub static PARSER: nitride_core::CompiledParser = nitride_core::CompiledParser::new(&LEXERS,{},{});\n",
        list(bodies),
        list(tails)
    );
    Ok(output)
}
fn as_result(body: &str) -> String {
    if body.starts_with("c.interpret(") {
        body.strip_suffix('?')
            .expect("fallback expression result")
            .to_owned()
    } else {
        format!("Ok({body})")
    }
}
fn function(name: &str, body: &str) -> String {
    let result = as_result(body);
    format!(
        "#[allow(dead_code, unused_variables)]\nfn {name}(c:&mut nitride_core::SyntaxContext<'_, '_>,_minimum:u32,d:u32)->Result<bool,nitride_core::Diagnostic>{{c.guard()?;{result}}}\n"
    )
}

fn emit_parts(
    parts: &[Value],
    ids: &HashMap<&str, u16>,
    owner: Option<u16>,
    depth: u32,
    serial: &mut usize,
) -> Result<String, String> {
    let mut output = Vec::new();
    for (index, e) in parts.iter().enumerate() {
        output.push(emit(
            e,
            ids,
            if index + 1 == parts.len() {
                owner
            } else {
                None
            },
            depth,
            serial,
        )?);
    }
    Ok(conjunction(output))
}
fn conjunction(parts: Vec<String>) -> String {
    match parts.len() {
        0 => "true".into(),
        1 => parts.into_iter().next().unwrap(),
        _ => parts
            .into_iter()
            .map(|p| format!("({p})"))
            .collect::<Vec<_>>()
            .join(" && "),
    }
}
fn emit(
    e: &Value,
    ids: &HashMap<&str, u16>,
    owner: Option<u16>,
    depth: u32,
    serial: &mut usize,
) -> Result<String, String> {
    let d = if depth == 0 {
        "d".to_owned()
    } else {
        format!("d+{depth}")
    };
    let next = depth + 1;
    let body = match text(&e["kind"])? {
        "literal" => format!("c.literal({})?", quoted(text(&e["text"])?)),
        "ref" => {
            let id = lookup(ids, text(&e["name"])?)?;
            format!(
                "c.reference({id},{},{d})?",
                if Some(id) == owner { "_minimum" } else { "0" }
            )
        }
        "sequence" => conjunction(
            array(&e["items"])?
                .iter()
                .map(|e| emit(e, ids, None, next, serial))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        "label" => format!(
            "{{let from=c.children_len();let matched={};c.label({},from,{d})?;matched}}",
            emit(&e["inner"], ids, owner, next, serial)?,
            quoted(text(&e["label"])?)
        ),
        "repeat" => {
            let min = e["min"].as_u64().ok_or("invalid repeat min")?;
            let max = e["max"].as_u64().ok_or("invalid repeat max")?;
            if max == 0 {
                "true".into()
            } else {
                let n = *serial;
                *serial += 1;
                let inner = emit(&e["inner"], ids, None, next, serial)?;
                let inner_result = if inner.starts_with("c.interpret(") {
                    as_result(&inner)
                } else {
                    format!(
                        "(||->Result<bool,nitride_core::Diagnostic>{{{}}})()",
                        as_result(&inner)
                    )
                };
                let result = if min == 0 {
                    "true".into()
                } else {
                    format!("count{n}>={min}u32")
                };
                format!(
                    "{{let mut count{n}=0u32;while count{n}<{max}u32{{let mark=c.begin_iteration({});let result={inner_result};let matched=c.end_iteration(mark,result,{})?;if !matched{{break;}}count{n}+=1;}}{result}}}",
                    max > 1,
                    max > 1
                )
            }
        }
        _ => {
            let owner = owner.map_or_else(|| "None".into(), |n| format!("Some({n})"));
            let minimum = if owner == "None" { "0" } else { "_minimum" };
            return Ok(format!(
                "c.interpret(&{},{owner},{minimum},{d})?",
                emit_expr(e, ids)?
            ));
        }
    };
    Ok(format!(
        "{{c.step({d})?;let matched={body};c.finish(matched)}}"
    ))
}
