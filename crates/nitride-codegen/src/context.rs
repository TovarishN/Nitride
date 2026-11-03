//! Scoped parse context, specialized away before generation.
//!
//! A grammar's `context` flags are Boolean and every parse starts with all of
//! them false. `with(Flag = v) e` parses `e` with one flag replaced and
//! `context(Flag)` tests it without consuming input. Both contribute no node.
//!
//! Instead of carrying the flags in parser state, each syntax rule is
//! instantiated once per environment it can be reached in, restricted to the
//! flags it (transitively) reads. The instance under all-false flags keeps the
//! rule's own index, so start rules, categories, checks and patterns are
//! unchanged; every other instance is appended as a clone whose `origin` names
//! the rule its nodes report. Memoization and incremental reuse key on rule
//! identity, so results computed under one environment can never answer a
//! lookup under another.
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

fn text(v: &Value) -> Result<&str, String> {
    v.as_str().ok_or_else(|| "expected string".into())
}

/// The prepared data with every `context` and `scope` expression removed.
/// Data without context flags is returned unchanged.
pub(crate) fn specialize(data: &Value) -> Result<Option<Value>, String> {
    let Some(flags) = data.get("contexts") else {
        return Ok(None);
    };
    let flags: Vec<&str> = flags
        .as_array()
        .ok_or("contexts must be a list")?
        .iter()
        .map(text)
        .collect::<Result<_, _>>()?;
    if flags.len() > 64 {
        return Err("more than 64 context flags".into());
    }
    let mut bits = HashMap::new();
    for (i, flag) in flags.iter().enumerate() {
        if bits.insert(*flag, 1u64 << i).is_some() {
            return Err("duplicate context flag".into());
        }
    }
    let rules = data["rules"].as_array().ok_or("rules must be a list")?;
    let mut ids = HashMap::new();
    for (i, rule) in rules.iter().enumerate() {
        ids.insert(text(&rule["name"])?, i);
    }
    let lookup = |name: &str| ids.get(name).copied().ok_or(format!("unknown rule {name}"));
    let flag = |e: &Value| -> Result<u64, String> {
        bits.get(text(&e["name"])?)
            .copied()
            .ok_or_else(|| format!("unknown context flag {}", e["name"]))
    };

    // The flags each rule reads before any scope of its own replaces them.
    fn reads(
        e: &Value,
        deps: &[u64],
        lookup: &impl Fn(&str) -> Result<usize, String>,
        flag: &impl Fn(&Value) -> Result<u64, String>,
    ) -> Result<u64, String> {
        Ok(match e["kind"].as_str() {
            Some("context") => flag(e)?,
            Some("scope") => reads(&e["inner"], deps, lookup, flag)? & !flag(e)?,
            Some("ref") => deps[lookup(text(&e["name"])?)?],
            Some("pratt") => {
                let mut found = 0;
                for key in ["prefixes", "infixes"] {
                    for name in e[key].as_array().ok_or("invalid pratt")? {
                        found |= deps[lookup(text(name)?)?];
                    }
                }
                found
            }
            _ => {
                let mut found = 0;
                for child in e["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .chain(e.get("inner"))
                {
                    found |= reads(child, deps, lookup, flag)?;
                }
                found
            }
        })
    }
    let mut deps = vec![0u64; rules.len()];
    loop {
        let mut changed = false;
        for (i, rule) in rules.iter().enumerate() {
            let mut found = reads(&rule["expr"], &deps, &lookup, &flag)?;
            // An alternative is instantiated with its point, so its operand
            // references and its owner always agree.
            if let Some(owner) = rule["owner"].as_str() {
                found |= deps[lookup(owner)?];
            }
            if found & !deps[i] != 0 {
                deps[i] |= found;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    if let Some(rule) = rules
        .iter()
        .zip(&deps)
        .find(|(r, d)| **d != 0 && r["lexical"] == true)
    {
        return Err(format!("token {} cannot use parse context", rule.0["name"]));
    }

    let mut instances = Instances {
        rules,
        deps: &deps,
        flags: &flags,
        ids: &ids,
        names: HashMap::new(),
        pending: (0..rules.len()).map(|i| (i, 0)).collect(),
    };
    let mut output: Vec<Value> = rules.to_vec();
    // Alias rules build no node. As in Nitrogen, a scope is seen through but a
    // context test is not a reference or literal, so this is decided before
    // folding and every instance keeps its rule's decision.
    let aliases: Vec<bool> = rules
        .iter()
        .map(|r| {
            let mut unscoped = r.clone();
            unscoped["expr"] = without_scopes(&r["expr"]);
            crate::alias_rule(&unscoped)
        })
        .collect();
    while let Some((rule, env)) = instances.pending.pop_front() {
        let source = &rules[rule];
        let mut out = source.clone();
        out["alias"] = json!(aliases[rule]);
        if source["expr"]["kind"] == "pratt" {
            for key in ["prefixes", "infixes"] {
                let mut kept = Vec::new();
                for name in source["expr"][key].as_array().ok_or("invalid pratt")? {
                    let alternative = &rules[lookup(text(name)?)?]["expr"];
                    // An alternative whose context test cannot hold is not offered.
                    if !matches!(
                        rewrite(alternative, env, &bits, &mut |n, _| Ok(n.into()))?,
                        Folded::Fail
                    ) {
                        kept.push(json!(instances.get(text(name)?, env)?));
                    }
                }
                out["expr"][key] = json!(kept);
            }
        } else {
            out["expr"] =
                rewrite(&source["expr"], env, &bits, &mut |n, e| instances.get(n, e))?.into_value();
        }
        if let Some(owner) = source["owner"].as_str() {
            out["owner"] = json!(instances.get(owner, env)?);
        }
        if env == 0 {
            output[rule] = out;
            continue;
        }
        out["origin"] = source["name"].clone();
        out["name"] = json!(instances.names[&(rule, env)]);
        // Semantics and checks belong to the rule the nodes report.
        out["clauses"] = json!([]);
        if let Some(map) = out.as_object_mut() {
            map.remove("checks");
        }
        output.push(out);
    }
    if output.len() > u16::MAX as usize {
        return Err("context instances exceed the rule limit".into());
    }
    let mut data = data.clone();
    data.as_object_mut().unwrap().remove("contexts");
    data["rules"] = json!(output);
    Ok(Some(data))
}

/// Rule instances by environment; the all-false instance is the rule itself.
struct Instances<'a> {
    rules: &'a [Value],
    deps: &'a [u64],
    flags: &'a [&'a str],
    ids: &'a HashMap<&'a str, usize>,
    names: HashMap<(usize, u64), String>,
    pending: VecDeque<(usize, u64)>,
}

impl Instances<'_> {
    /// The name of `name`'s instance under `env`, queued when new.
    fn get(&mut self, name: &str, env: u64) -> Result<String, String> {
        let rule = *self.ids.get(name).ok_or(format!("unknown rule {name}"))?;
        let env = env & self.deps[rule];
        if env == 0 {
            return Ok(name.into());
        }
        if let Some(found) = self.names.get(&(rule, env)) {
            return Ok(found.clone());
        }
        let set: Vec<_> = (self.flags.iter().enumerate())
            .filter(|(i, _)| env & (1 << i) != 0)
            .map(|(_, f)| *f)
            .collect();
        let clone = format!("{}@{}", text(&self.rules[rule]["name"])?, set.join("+"));
        self.names.insert((rule, env), clone.clone());
        self.pending.push_back((rule, env));
        Ok(clone)
    }
}

/// An expression after its context tests were decided.
enum Folded {
    /// Matches the empty string and builds nothing.
    Empty,
    /// Never matches.
    Fail,
    Expr(Value),
}

impl Folded {
    fn into_value(self) -> Value {
        match self {
            Folded::Empty => json!({"kind":"sequence","items":[]}),
            Folded::Fail => {
                json!({"kind":"predicate","positive":false,"inner":{"kind":"sequence","items":[]}})
            }
            Folded::Expr(v) => v,
        }
    }
}

fn rewrite(
    e: &Value,
    env: u64,
    bits: &HashMap<&str, u64>,
    reference: &mut dyn FnMut(&str, u64) -> Result<String, String>,
) -> Result<Folded, String> {
    let bit = |e: &Value| -> Result<u64, String> {
        bits.get(text(&e["name"])?)
            .copied()
            .ok_or_else(|| format!("unknown context flag {}", e["name"]))
    };
    Ok(match e["kind"].as_str() {
        Some("context") => {
            if env & bit(e)? != 0 {
                Folded::Empty
            } else {
                Folded::Fail
            }
        }
        Some("scope") => {
            let flag = bit(e)?;
            let inner = if e["value"] == true {
                env | flag
            } else {
                env & !flag
            };
            rewrite(&e["inner"], inner, bits, reference)?
        }
        Some("ref") => {
            let mut out = e.clone();
            out["name"] = json!(reference(text(&e["name"])?, env)?);
            Folded::Expr(out)
        }
        Some("sequence") => {
            let mut items = Vec::new();
            let mut dropped = false;
            for item in e["items"].as_array().unwrap() {
                match rewrite(item, env, bits, reference)? {
                    Folded::Fail => return Ok(Folded::Fail),
                    // A decided test leaves nothing; an authored `()` stays.
                    Folded::Empty if !authored_empty(item) => dropped = true,
                    folded => items.push(folded.into_value()),
                }
            }
            match items.len() {
                0 if dropped => Folded::Empty,
                1 if dropped => Folded::Expr(items.pop().unwrap()),
                _ => {
                    let mut out = e.clone();
                    out["items"] = json!(items);
                    Folded::Expr(out)
                }
            }
        }
        Some("choice") => {
            let mut items = Vec::new();
            for item in e["items"].as_array().unwrap() {
                match rewrite(item, env, bits, reference)? {
                    Folded::Fail => {}
                    folded => items.push(folded.into_value()),
                }
            }
            match items.len() {
                0 => Folded::Fail,
                1 => Folded::Expr(items.pop().unwrap()),
                _ => {
                    let mut out = e.clone();
                    out["items"] = json!(items);
                    Folded::Expr(out)
                }
            }
        }
        Some("predicate") => match rewrite(&e["inner"], env, bits, reference)? {
            Folded::Empty if e["positive"] == true => Folded::Empty,
            Folded::Empty => Folded::Fail,
            Folded::Fail if e["positive"] == true => Folded::Fail,
            Folded::Fail => Folded::Empty,
            Folded::Expr(inner) => {
                let mut out = e.clone();
                out["inner"] = inner;
                Folded::Expr(out)
            }
        },
        Some("label") => match rewrite(&e["inner"], env, bits, reference)? {
            Folded::Fail => Folded::Fail,
            folded => {
                let mut out = e.clone();
                out["inner"] = folded.into_value();
                Folded::Expr(out)
            }
        },
        Some("repeat") => match rewrite(&e["inner"], env, bits, reference)? {
            Folded::Fail if e["min"] == 0 => Folded::Empty,
            Folded::Fail => Folded::Fail,
            folded => {
                let mut out = e.clone();
                out["inner"] = folded.into_value();
                Folded::Expr(out)
            }
        },
        _ => Folded::Expr(e.clone()),
    })
}

/// An authored `()`, kept as written.
fn authored_empty(e: &Value) -> bool {
    e["kind"] == "sequence" && e["items"].as_array().is_some_and(Vec::is_empty)
}

/// The expression with every scope replaced by its body.
fn without_scopes(e: &Value) -> Value {
    if e["kind"] == "scope" {
        return without_scopes(&e["inner"]);
    }
    let mut out = e.clone();
    if let Some(items) = e["items"].as_array() {
        out["items"] = json!(items.iter().map(without_scopes).collect::<Vec<_>>());
    }
    if let Some(inner) = e.get("inner") {
        out["inner"] = without_scopes(inner);
    }
    out
}
