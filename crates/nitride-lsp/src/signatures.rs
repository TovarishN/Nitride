//! Signature help for built-in calls. The call is found in the text, as
//! Nitrogen does, because recovery can drop a half-typed call's commas from
//! the tree: walking back from the cursor, characters inside token leaves
//! are skipped and the innermost unclosed `name(` wins.
use nitride_core::*;
use serde_json::{Value, json};

/// The callee and active parameter of the innermost unclosed call before
/// `offset`; `None` outside any call or past a `;`, `{` or `}`.
fn call_at(text: &str, offset: usize, leaves: &[[u32; 2]]) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    let inside = |i: usize| {
        let at = leaves.partition_point(|l| l[0] as usize <= i);
        at > 0 && i < leaves[at - 1][1] as usize
    };
    let (mut depth, mut commas) = (0usize, 0usize);
    let mut i = offset.min(bytes.len());
    while i > 0 {
        i -= 1;
        let c = bytes[i];
        if inside(i) && !matches!(c, b'(' | b')' | b',') {
            continue;
        }
        match c {
            b')' => depth += 1,
            b'(' if depth == 0 => {
                let end = text[..i].trim_end().len();
                let start = text[..end]
                    .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
                    .map_or(0, |p| p + 1);
                return (start < end).then(|| (text[start..end].to_owned(), commas));
            }
            b'(' => depth -= 1,
            b',' if depth == 0 => commas += 1,
            b';' | b'{' | b'}' => return None,
            _ => {}
        }
    }
    None
}

pub fn help(text: &str, offset: u32, language: &Language<'_>, leaves: &[[u32; 2]]) -> Value {
    let Some((name, active)) = call_at(text, offset as usize, leaves) else {
        return Value::Null;
    };
    let Some(builtin) = language
        .rules
        .iter()
        .flat_map(|r| match r.action {
            Action::Call { builtins, .. } => builtins,
            _ => &[],
        })
        .find(|b| b.name == name)
    else {
        return Value::Null;
    };
    let mut signatures = Vec::new();
    let mut arities = Vec::new();
    for &candidate in builtin.candidates {
        let Some(operation) = language.operations.get(candidate as usize) else {
            continue;
        };
        let mut label = format!("{name}(");
        let mut parameters = Vec::new();
        for (i, input) in operation.inputs.iter().enumerate() {
            if i > 0 {
                label.push_str(", ");
            }
            let start = label.encode_utf16().count();
            label.push_str(input);
            parameters.push(json!({"label": [start, label.encode_utf16().count()]}));
        }
        label.push_str(&format!(") : {}", operation.result));
        arities.push(operation.inputs.len());
        signatures.push(json!({"label": label, "parameters": parameters}));
    }
    if signatures.is_empty() {
        return Value::Null;
    }
    let chosen = arities.iter().position(|&n| n > active).unwrap_or(0);
    json!({"signatures": signatures, "activeSignature": chosen, "activeParameter": active})
}
