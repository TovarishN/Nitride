//! Transactional LSP changes and byte edits for debounced syntax updates.
use crate::lines::LineIndex;
use nitride_core::TextEdit;
use serde_json::Value;

pub struct Document {
    pub text: String,
    pub version: i32,
}
impl Document {
    /// Apply all changes against each preceding result, or leave everything intact.
    pub fn apply(&mut self, version: i32, changes: &Value) -> bool {
        if version <= self.version { return false; }
        let Some(changes) = changes.as_array().filter(|c| !c.is_empty()) else { return false; };
        let mut text = self.text.clone();
        for change in changes {
            let Some(replacement) = change["text"].as_str() else { return false; };
            if let Some(range) = change.get("range") {
                let lines = LineIndex::new(&text);
                let position = |key: &str| {
                    let line = u32::try_from(range[key]["line"].as_u64()?).ok()?;
                    let column = u32::try_from(range[key]["character"].as_u64()?).ok()?;
                    lines.checked_offset(line, column)
                };
                let (Some(start), Some(end)) = (position("start"), position("end")) else { return false; };
                if start > end { return false; }
                text.replace_range(start as usize..end as usize, replacement);
            } else {
                text = replacement.to_owned();
            }
        }
        self.text = text;
        self.version = version;
        true
    }
}

/// One exact edit encompassing the changes since the last debounced analysis.
/// Common byte prefixes/suffixes are shortened to valid UTF-8 boundaries.
pub(crate) fn difference(before: &str, after: &str) -> Option<TextEdit> {
    if before == after { return None; }
    let mut start = before.bytes().zip(after.bytes()).take_while(|(a,b)| a == b).count();
    while !before.is_char_boundary(start) || !after.is_char_boundary(start) { start -= 1; }
    let suffix = before.as_bytes()[start..].iter().rev()
        .zip(after.as_bytes()[start..].iter().rev()).take_while(|(a,b)| a == b).count();
    let (mut old_end, mut new_end) = (before.len() - suffix, after.len() - suffix);
    while !before.is_char_boundary(old_end) || !after.is_char_boundary(new_end) {
        old_end += 1;
        new_end += 1;
    }
    Some(TextEdit { range: [start as u32, old_end as u32], replacement: after[start..new_end].into() })
}
