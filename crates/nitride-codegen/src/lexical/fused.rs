//! Exact first-byte traces. Unknown suffixes and executed rule calls decline.
use serde_json::Value;

#[derive(Clone, Copy, Default)]
pub(super) struct Decision {
    pub work: u16,
    pub depth: u8,
    pub observed: u32,
    pub matched: bool,
    pub advance: bool,
}
impl Decision {
    fn absorb(&mut self, other: Self) -> Option<()> {
        self.work = self.work.checked_add(other.work)?;
        self.depth = self.depth.max(other.depth);
        self.observed = self.observed.max(other.observed);
        Some(())
    }
}
pub(super) fn plan(expr: &Value) -> Option<[Decision; 128]> {
    let entries = std::array::from_fn(|byte| {
        trace(expr, byte as u8)
            .filter(|d| d.work >= 4)
            .unwrap_or_default()
    });
    entries.iter().any(|d| d.work != 0).then_some(entries)
}
/// Largest uniform group of successful single-byte traces. Rule calls and
/// observations beyond that byte are never batched.
pub(super) fn run_plan(expr: &Value) -> Option<([u64; 2], u16, u8)> {
    let mut groups = std::collections::BTreeMap::<(u16, u8), [u64; 2]>::new();
    for byte in 0..128u8 {
        if let Some(t) =
            trace(expr, byte).filter(|t| t.matched && t.advance && t.observed == 1 && t.work != 0)
        {
            let mask = groups.entry((t.work, t.depth)).or_default();
            mask[byte as usize / 64] |= 1u64 << (byte % 64);
        }
    }
    groups
        .into_iter()
        .max_by_key(|(_, m)| m[0].count_ones() + m[1].count_ones())
        .filter(|(_, m)| m[0].count_ones() + m[1].count_ones() >= 4)
        .map(|((work, depth), mask)| (mask, work, depth))
}
fn trace(expr: &Value, byte: u8) -> Option<Decision> {
    if !byte.is_ascii() {
        return None;
    }
    eval(expr, byte, 0, &mut 4096)
}
fn eval(expr: &Value, byte: u8, depth: u8, remaining: &mut usize) -> Option<Decision> {
    if depth > 64 || *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    let mut result = Decision {
        work: 1,
        depth,
        ..Default::default()
    };
    let next = depth.checked_add(1)?;
    match expr["kind"].as_str()? {
        "class" => {
            let mut hit = false;
            for range in expr["ranges"].as_array()? {
                let low = range.get(0)?.as_u64()?;
                let high = range.get(1)?.as_u64()?;
                hit |= low <= u64::from(byte) && u64::from(byte) <= high;
            }
            result.observed = 1;
            result.matched = hit != (expr["negated"] == true);
            result.advance = result.matched;
        }
        "any" => {
            result.observed = 1;
            result.matched = true;
            result.advance = true;
        }
        "literal" => {
            let text = expr["text"].as_str()?;
            result.observed = u32::try_from(text.len()).ok()?;
            if text.is_empty() {
                result.matched = true;
            } else if text.as_bytes()[0] == byte {
                if text.len() != 1 {
                    return None;
                }
                result.matched = true;
                result.advance = true;
            }
        }
        "sequence" => {
            result.matched = true;
            for part in expr["items"].as_array()? {
                if result.advance {
                    return None;
                }
                let child = eval(part, byte, next, remaining)?;
                result.absorb(child)?;
                result.advance = child.advance;
                if !child.matched {
                    result.matched = false;
                    break;
                }
            }
        }
        "choice" => {
            for part in expr["items"].as_array()? {
                let child = eval(part, byte, next, remaining)?;
                result.absorb(child)?;
                if child.matched {
                    result.matched = true;
                    result.advance = child.advance;
                    break;
                }
                if child.advance {
                    return None;
                }
            }
        }
        "label" => {
            let child = eval(&expr["inner"], byte, next, remaining)?;
            result.absorb(child)?;
            result.matched = child.matched;
            result.advance = child.advance;
        }
        "predicate" => {
            let child = eval(&expr["inner"], byte, next, remaining)?;
            result.absorb(child)?;
            result.matched = child.matched == (expr["positive"] == true);
        }
        "repeat" => {
            let min = expr["min"].as_u64()?;
            let max = expr["max"].as_u64()?;
            if min > max || max > u64::from(u32::MAX) {
                return None;
            }
            if max == 0 {
                result.matched = true;
            } else {
                let child = eval(&expr["inner"], byte, next, remaining)?;
                result.absorb(child)?;
                if !child.matched {
                    result.matched = min == 0;
                } else if max == 1 {
                    result.matched = true;
                    result.advance = child.advance;
                } else {
                    return None;
                }
            }
        }
        _ => return None,
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::{plan, run_plan, trace};
    use serde_json::json;
    fn class() -> serde_json::Value {
        json!({"kind":"class","ranges":[[97,97]],"negated":false})
    }
    #[test]
    fn runs_only_group_exact_advancing_traces_without_rule_calls() {
        let lower = json!({"kind":"class","ranges":[[97,122]],"negated":false});
        assert_eq!(
            run_plan(&lower),
            Some(([0, ((1u64 << 26) - 1) << 33], 1, 0))
        );
        let e = json!({"kind":"choice","items":[lower,{"kind":"ref","name":"Escape"}]});
        assert_eq!(run_plan(&e), Some(([0, ((1u64 << 26) - 1) << 33], 2, 1)));
        assert!(run_plan(&json!({"kind":"ref","name":"Lower"})).is_none());
        assert!(run_plan(&json!({"kind":"literal","text":""})).is_none());
        let e = json!({"kind":"sequence","items":[{"kind":"predicate","positive":false,"inner":{"kind":"literal","text":"ab"}}, {"kind":"any"}]});
        assert!(
            run_plan(&e).is_none(),
            "two-byte failed lookahead must not lose observations"
        );
    }
    #[test]
    fn ordered_decisions_preserve_work_depth_and_unknown_suffixes() {
        let e = json!({"kind":"choice","items":[class(),{"kind":"sequence","items":[{"kind":"literal","text":"b"},{"kind":"literal","text":"c"}]}]});
        let a = trace(&e, b'a').unwrap();
        assert_eq!(
            (a.work, a.depth, a.observed, a.matched, a.advance),
            (2, 1, 1, true, true)
        );
        let dot = trace(&e, b'.').unwrap();
        assert_eq!(
            (dot.work, dot.depth, dot.observed, dot.matched, dot.advance),
            (4, 2, 1, false, false)
        );
        assert!(trace(&e, b'b').is_none());
        let table = plan(&e).unwrap();
        assert_eq!(table[b'a' as usize].work, 0);
        assert_eq!(table[b'.' as usize].work, 4);
        assert_eq!(table[b'b' as usize].work, 0);
    }
    #[test]
    fn predicates_and_empty_success_keep_failed_literal_reach() {
        let e = json!({"kind":"choice","items":[{"kind":"literal","text":"😀x"},{"kind":"label","label":"x","inner":{"kind":"literal","text":""}}]});
        let t = trace(&e, b'a').unwrap();
        assert_eq!(
            (t.work, t.depth, t.observed, t.matched, t.advance),
            (4, 2, 5, true, false)
        );
        let e = json!({"kind":"sequence","items":[{"kind":"predicate","positive":false,"inner":{"kind":"literal","text":"?"}},{"kind":"predicate","positive":true,"inner":class()},class()]});
        let t = trace(&e, b'a').unwrap();
        assert_eq!(
            (t.work, t.depth, t.observed, t.matched, t.advance),
            (6, 2, 1, true, true)
        );
        let t = trace(&e, b'?').unwrap();
        assert_eq!(
            (t.work, t.depth, t.observed, t.matched, t.advance),
            (3, 2, 1, false, false)
        );
    }
    #[test]
    fn repeat_zero_optional_failed_and_unknown_iteration_follow_emitter() {
        let mut e = json!({"kind":"repeat","min":0,"max":0,"inner":{"kind":"literal","text":""}});
        assert_eq!(trace(&e, b'a').unwrap().work, 1);
        e["max"] = 1.into();
        let t = trace(&e, b'a').unwrap();
        assert_eq!((t.work, t.depth, t.matched, t.advance), (2, 1, true, false));
        e["max"] = 2.into();
        assert!(trace(&e, b'a').is_none());
        e["inner"] = class();
        let t = trace(&e, b'b').unwrap();
        assert_eq!((t.work, t.matched, t.advance), (2, true, false));
        assert!(trace(&e, b'a').is_none());
        e["max"] = 1.into();
        assert!(trace(&e, b'a').unwrap().advance);
        e["min"] = 1.into();
        assert!(!trace(&e, b'b').unwrap().matched);
    }
    #[test]
    fn only_executed_refs_decline_and_class_negation_is_exact() {
        let e = json!({"kind":"choice","items":[{"kind":"literal","text":"x"},{"kind":"ref","name":"Unknown"}]});
        assert!(trace(&e, b'x').unwrap().matched);
        assert!(trace(&e, b'y').is_none());
        let mut e = class();
        e["negated"] = true.into();
        assert!(!trace(&e, b'a').unwrap().matched);
        assert!(trace(&e, b'b').unwrap().matched);
    }
    #[test]
    fn resource_bounds_decline_without_approximation() {
        let mut deep = class();
        for _ in 0..65 {
            deep = json!({"kind":"label","label":"x","inner":deep});
        }
        assert!(trace(&deep, b'a').is_none());
        let huge = json!({"kind":"choice","items":vec![json!({"kind":"literal","text":"!"});5000]});
        assert!(trace(&huge, b'a').is_none());
        assert!(plan(&huge).is_none());
        assert!(trace(&json!({"kind":"literal","text":"ab"}), b'a').is_none());
    }
}
