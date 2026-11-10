use serde_json::Value;
pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| values_equal(a, b))
        }
        (Value::Number(a), Value::Number(b)) => {
            let integer = |n: &serde_json::Number| {
                n.as_i64()
                    .map(i128::from)
                    .or_else(|| n.as_u64().map(i128::from))
            };
            match (integer(a), integer(b)) {
                (Some(a), Some(b)) => a == b,
                (Some(i), None) | (None, Some(i)) => {
                    let f = if integer(a).is_none() {
                        a.as_f64()
                    } else {
                        b.as_f64()
                    };
                    f.is_some_and(|f| {
                        f.is_finite()
                            && f.fract() == 0.0
                            && f >= i64::MIN as f64
                            && f < 18446744073709551616.0
                            && (f as i128) == i
                    })
                }
                _ => a.as_f64() == b.as_f64(),
            }
        }
        _ => a == b,
    }
}
