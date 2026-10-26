//! Specialize bounded, acyclic lexical graphs. The ordinary Expr data stays intact.
use super::*;
mod fused;

pub(super) fn generate(rules: &[Value], ids: &HashMap<&str, u16>) -> Result<String, String> {
    let rules: Vec<_> = rules
        .iter()
        .map(|r| {
            let mut r = r.clone();
            r["expr"] = pair_surrogates(&r["expr"]);
            r
        })
        .collect();
    fn rule(
        id: usize,
        rules: &[Value],
        ids: &HashMap<&str, u16>,
        state: &mut [u8],
        depth: usize,
        budget: &mut usize,
    ) -> bool {
        if depth > 64 || *budget == 0 {
            return false;
        }
        if state[id] != 0 {
            return state[id] == 2;
        }
        state[id] = 1;
        let ok = rules[id]["lexical"] == true
            && rules[id]["owner"].is_null()
            && expr(&rules[id]["expr"], rules, ids, state, depth + 1, budget);
        state[id] = if ok { 2 } else { 3 };
        ok
    }
    fn expr(
        e: &Value,
        rules: &[Value],
        ids: &HashMap<&str, u16>,
        state: &mut [u8],
        depth: usize,
        budget: &mut usize,
    ) -> bool {
        if depth > 64 || *budget == 0 {
            return false;
        }
        *budget -= 1;
        match e["kind"].as_str().unwrap_or("") {
            "literal" | "class" | "any" => true,
            "ref" => ids
                .get(e["name"].as_str().unwrap_or(""))
                .is_some_and(|id| rule(*id as usize, rules, ids, state, depth + 1, budget)),
            "label" | "predicate" | "repeat" => {
                expr(&e["inner"], rules, ids, state, depth + 1, budget)
            }
            "choice" | "sequence" => e["items"].as_array().is_some_and(|items| {
                items
                    .iter()
                    .all(|e| expr(e, rules, ids, state, depth + 1, budget))
            }),
            _ => false,
        }
    }
    let mut state = vec![0; rules.len()];
    let mut budget = 1_000_000;
    for id in 0..rules.len() {
        rule(id, &rules, ids, &mut state, 0, &mut budget);
    }
    // Recheck graph height independently of cached eligibility. A shared suffix
    // admitted shallowly must not make a later deep caller exceed the bound.
    fn height(
        e: &Value,
        rules: &[Value],
        ids: &HashMap<&str, u16>,
        depth: usize,
        budget: &mut usize,
    ) -> bool {
        if depth > 64 || *budget == 0 {
            return false;
        }
        *budget -= 1;
        match e["kind"].as_str().unwrap_or("") {
            "ref" => height(
                &rules[ids[e["name"].as_str().unwrap()] as usize]["expr"],
                rules,
                ids,
                depth + 2,
                budget,
            ),
            "label" | "predicate" | "repeat" => height(&e["inner"], rules, ids, depth + 1, budget),
            "choice" | "sequence" => e["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| height(e, rules, ids, depth + 1, budget)),
            _ => true,
        }
    }
    for id in 0..rules.len() {
        if state[id] == 2 && !height(&rules[id]["expr"], &rules, ids, 0, &mut budget) {
            state[id] = 3;
        }
    }
    // Every emitted callback must call another emitted callback.
    loop {
        let mut changed = false;
        for id in 0..rules.len() {
            if state[id] != 2 {
                continue;
            }
            let mut references = HashSet::new();
            collect_refs(&rules[id]["expr"], &mut references);
            if references.iter().any(|name| state[ids[name] as usize] != 2) {
                state[id] = 3;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut output = String::new();
    for (id, r) in rules.iter().enumerate().filter(|(id, _)| state[*id] == 2) {
        let mut body = emit(&r["expr"], ids, 1, &mut 0)?;
        if let Some(entries) = fused::plan(&r["expr"]) {
            let table = format!("NITRIDE_ASCII_{id}");
            let values = entries
                .iter()
                .map(|entry| {
                    if entry.work == 0 {
                        "nitride_core::lexical::AsciiDecision::UNKNOWN".to_owned()
                    } else {
                        format!(
                            "nitride_core::lexical::AsciiDecision::new({},{},{},{},{})",
                            entry.work, entry.depth, entry.observed, entry.matched, entry.advance
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            output += &format!(
                "#[allow(dead_code)]\nstatic {table}:[nitride_core::lexical::AsciiDecision;128]=[{values}];\n"
            );
            body = format!(
                "if let Some(matched)=c.ascii_decision(&{table},d+1){{matched}}else{{{body}}}"
            );
        }
        output += &format!(
            "#[allow(dead_code)]\nfn nitride_lex_{id}(c: &mut nitride_core::lexical::LexicalContext<'_>, d:u32)->Option<bool>{{c.enter(d)?;let start=c.position();let matched={body};if !matched || c.excluded(start,{}) {{c.restore(start);Some(false)}} else {{Some(true)}}}}\n",
            list(strings(&r["except"])?.into_iter().map(quoted))
        );
    }
    output += &format!(
        "#[allow(dead_code)]\npub static LEXERS: nitride_core::lexical::CompiledLexers = nitride_core::lexical::CompiledLexers::new(&LANGUAGE,{});\n",
        list(state.iter().enumerate().map(|(id, s)| if *s == 2 {
            format!("Some(nitride_lex_{id})")
        } else {
            "None".into()
        }))
    );
    Ok(output)
}

fn emit(
    e: &Value,
    ids: &HashMap<&str, u16>,
    depth: u32,
    serial: &mut usize,
) -> Result<String, String> {
    let d = format!("d+{depth}");
    let next = depth + 1;
    let inner = match text(&e["kind"])? {
        "literal" => format!("c.literal({})", quoted(text(&e["text"])?)),
        "any" => "c.any()".into(),
        "ref" => format!("nitride_lex_{}(c,{d}+1)?", lookup(ids, text(&e["name"])?)?),
        "class" => {
            let mut ranges: Vec<(u32, u32)> = array(&e["ranges"])?
                .iter()
                .map(|r| (r[0].as_u64().unwrap() as u32, r[1].as_u64().unwrap() as u32))
                .collect();
            ranges.sort_unstable();
            let mut merged: Vec<(u32, u32)> = Vec::new();
            for (a, b) in ranges {
                if let Some(last) = merged
                    .last_mut()
                    .filter(|last| a <= last.1.saturating_add(1))
                {
                    last.1 = last.1.max(b);
                } else {
                    merged.push((a, b));
                }
            }
            let mut ascii = [0u64; 2];
            for b in 0..128 {
                if merged.iter().any(|(a, z)| *a <= b && b <= *z) {
                    ascii[b as usize / 64] |= 1u64 << (b % 64);
                }
            }
            format!(
                "c.class([{}u64,{}u64],{}, {})",
                ascii[0],
                ascii[1],
                list(
                    merged
                        .into_iter()
                        .filter(|(_, b)| *b >= 128)
                        .map(|(a, b)| format!("({a}u32,{b}u32)"))
                ),
                e["negated"] == true
            )
        }
        "label" => emit(&e["inner"], ids, next, serial)?,
        "sequence" => {
            let parts = array(&e["items"])?
                .iter()
                .map(|e| emit(e, ids, next, serial))
                .collect::<Result<Vec<_>, _>>()?;
            if parts.is_empty() {
                "true".into()
            } else if parts.len() == 1 {
                parts.into_iter().next().unwrap()
            } else {
                parts
                    .into_iter()
                    .map(|p| format!("({p})"))
                    .collect::<Vec<_>>()
                    .join(" && ")
            }
        }
        "choice" => {
            let n = *serial;
            *serial += 1;
            let mut body = format!("'choice{n}:{{let mark=c.position();");
            for e in array(&e["items"])? {
                body += &format!(
                    "let matched={};if matched {{break 'choice{n} true;}} c.restore(mark);\n",
                    emit(e, ids, next, serial)?
                );
            }
            body + "false}"
        }
        "predicate" => format!(
            "{{let mark=c.position();let matched={};c.restore(mark);{}}}",
            emit(&e["inner"], ids, next, serial)?,
            if e["positive"] == true {
                "matched"
            } else {
                "!matched"
            }
        ),
        "repeat" => {
            let min = e["min"].as_u64().unwrap();
            let max = e["max"].as_u64().unwrap();
            if max == 0 {
                "true".into()
            } else {
                let zero = if max > 1 {
                    "if c.position()==mark{return None;}"
                } else {
                    ""
                };
                let result = if min == 0 {
                    "true".into()
                } else {
                    format!("count>={min}u32")
                };
                format!(
                    "{{let mut count=0u32;while count<{max}u32{{let mark=c.position();let matched={};if !matched{{c.restore(mark);break;}}\n{zero}count+=1;}}{result}}}",
                    emit(&e["inner"], ids, next, serial)?
                )
            }
        }
        _ => return Err("unsupported native lexical expression".into()),
    };
    Ok(format!("{{c.step({d})?;{inner}}}"))
}

fn collect_refs<'a>(e: &'a Value, refs: &mut HashSet<&'a str>) {
    match e["kind"].as_str().unwrap_or("") {
        "ref" => {
            refs.insert(e["name"].as_str().unwrap());
        }
        "repeat" | "predicate" | "label" => collect_refs(&e["inner"], refs),
        "choice" | "sequence" => {
            for e in e["items"].as_array().unwrap() {
                collect_refs(e, refs);
            }
        }
        _ => {}
    }
}
