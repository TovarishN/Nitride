//! Relative sidecar positions share unchanged blocks across shifted snapshots.
//! Borrowed diagnostic/repair slices are explicit lazy exports, not stored spans.
use super::{
    memory::{self, Class, Visitor},
    sequence::{Measured, Sequence},
};
use crate::Diagnostic;
use std::sync::{Arc, OnceLock};
#[derive(Clone, PartialEq, Eq)]
struct Entry<T> {
    gap: i64,
    width: u32,
    value: T,
}
impl<T: Clone> Measured for Entry<T> {
    fn width(&self) -> u64 {
        0
    }
}
pub(crate) struct Records<T> {
    items: Sequence<Entry<T>>,
}
impl<T: Clone + Eq> Records<T> {
    pub(crate) fn new(values: impl IntoIterator<Item = ([u32; 2], T)>, old: Option<&Self>) -> Self {
        let mut at = 0_i64;
        let entries: Vec<_> = values
            .into_iter()
            .map(|(span, value)| {
                let next = i64::from(span[0]);
                let entry = Entry {
                    gap: next - at,
                    width: span[1] - span[0],
                    value,
                };
                at = next;
                entry
            })
            .collect();
        let items = if let Some(old) = old {
            let prefix = entries
                .iter()
                .zip(old.items.iter())
                .take_while(|(a, b)| a == b)
                .count();
            let suffix = entries
                .iter()
                .rev()
                .zip(
                    (0..old.items.len())
                        .rev()
                        .map(|i| old.items.get(i).unwrap()),
                )
                .take(entries.len().min(old.items.len()) - prefix)
                .take_while(|(a, b)| a == b)
                .count();
            old.items.replace(
                prefix..old.items.len() - suffix,
                entries[prefix..entries.len() - suffix].to_vec(),
            )
        } else {
            Sequence::from_vec(entries)
        };
        Self { items }
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = ([u32; 2], &T)> {
        let mut at = 0_i64;
        self.items.iter().map(move |entry| {
            at += entry.gap;
            let start = at as u32;
            ([start, start + entry.width], &entry.value)
        })
    }
    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }
    pub(crate) fn visit(&self, visit: &mut Visitor<'_>) {
        self.items.visit_allocations(Class::Sidecar, visit)
    }
}
#[derive(Clone, PartialEq, Eq)]
struct DiagnosticValue {
    category: &'static str,
    code: Option<Box<str>>,
    message: Arc<str>,
}
pub(crate) struct DiagnosticStore {
    records: Records<DiagnosticValue>,
    flat: OnceLock<Box<[Diagnostic]>>,
}
impl DiagnosticStore {
    pub(crate) fn new(values: Vec<Diagnostic>, old: Option<&Self>) -> Self {
        Self {
            records: Records::new(
                values.into_iter().map(|d| {
                    (
                        d.span,
                        DiagnosticValue {
                            category: d.category,
                            code: d.code,
                            message: d.message.into(),
                        },
                    )
                }),
                old.map(|o| &o.records),
            ),
            flat: OnceLock::new(),
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }
    pub(crate) fn has_message(&self, message: &str) -> bool {
        self.records
            .iter()
            .any(|(_, d)| d.message.as_ref() == message)
    }
    /// Whether any diagnostic has `category` with a message other than `except`.
    pub(crate) fn has_category_except(&self, category: &str, except: &str) -> bool {
        self.records
            .iter()
            .any(|(_, d)| d.category == category && d.message.as_ref() != except)
    }
    pub(crate) fn as_slice(&self) -> &[Diagnostic] {
        self.flat.get_or_init(|| {
            self.records
                .iter()
                .map(|(span, d)| Diagnostic {
                    span,
                    category: d.category,
                    code: d.code.clone(),
                    message: d.message.to_string(),
                    severity: crate::Severity::Error,
                })
                .collect()
        })
    }
    pub(crate) fn visit(&self, visit: &mut Visitor<'_>) {
        self.records.visit(visit);
        for (_, d) in self.records.iter() {
            let (id, size) = memory::arc(&d.message);
            visit(Class::Sidecar, id, size);
            if let Some(code) = &d.code {
                if !code.is_empty() {
                    visit(Class::Sidecar, code.as_ptr() as usize, code.len());
                }
            }
        }
        if let Some(flat) = self.flat.get() {
            if !flat.is_empty() {
                visit(
                    Class::Sidecar,
                    flat.as_ptr() as usize,
                    std::mem::size_of_val(flat.as_ref()),
                );
            }
            for diagnostic in flat {
                if let Some(code) = &diagnostic.code {
                    if !code.is_empty() {
                        visit(Class::Sidecar, code.as_ptr() as usize, code.len());
                    }
                }
                if diagnostic.message.capacity() > 0 {
                    visit(
                        Class::Sidecar,
                        diagnostic.message.as_ptr() as usize,
                        diagnostic.message.capacity(),
                    );
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_diagnostic_codes_are_retained_exported_and_counted() {
        let mut diagnostic = crate::Diagnostic::new("syntax", [0, 1], "invalid");
        diagnostic.code = Some(String::from("E123").into_boxed_str());
        let store = DiagnosticStore::new(vec![diagnostic], None);
        let records: Vec<_> = store
            .records
            .iter()
            .filter_map(|(_, d)| d.code.as_ref().map(|s| (s.as_ptr() as usize, s.len())))
            .collect();
        let flat = store.as_slice();
        assert_eq!(flat[0].code.as_deref(), Some("E123"));
        let code = flat[0].code.as_ref().unwrap();
        let mut allocations = Vec::new();
        store.visit(&mut |_, id, size| {
            allocations.push((id, size));
            true
        });
        assert!(records.iter().all(|code| allocations.contains(code)));
        assert!(allocations.contains(&(code.as_ptr() as usize, code.len())));
    }
    #[test]
    fn shifted_metadata_shares_blocks_and_resolves_absolute_positions() {
        let old = Records::new((0..100_000).map(|n| ([n * 2, n * 2 + 1], 7_u16)), None);
        let new = Records::new(
            (0..100_000).map(|n| ([n * 2 + 3, n * 2 + 4], 7_u16)),
            Some(&old),
        );
        assert!(old.items.shared_leaves(&new.items) > 3000);
        assert_eq!(old.iter().last().unwrap().0, [199998, 199999]);
        assert_eq!(new.iter().last().unwrap().0, [200001, 200002]);
        let unordered = Records::new([([7, 9], 1), ([3, 4], 2)], None);
        assert_eq!(
            unordered.iter().map(|(s, v)| (s, *v)).collect::<Vec<_>>(),
            vec![([7, 9], 1), ([3, 4], 2)]
        );
    }
}
