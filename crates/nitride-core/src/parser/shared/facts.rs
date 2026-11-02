//! Recognition facts recorded during one parse.
use super::hash::FastMap;
use crate::incremental::dependencies::{Hit, Key};

/// A map from position and rule to a fact, kept as a flat list so that a
/// cold snapshot can take the list as is and sort it by position only when
/// it is first edited. The map holds only each fact's place in the list.
#[derive(Clone, Default)]
pub(crate) struct FactLog {
    entries: Vec<(u32, Key, Hit)>,
    slots: FastMap<(u32, Key), u32>,
    text_len: u32,
}
impl FactLog {
    /// A log for a parse of `text_len` bytes, which grows to the number of
    /// facts the text so far predicts instead of doubling.
    pub(crate) fn for_text(text_len: u32) -> Self {
        Self {
            text_len,
            ..Default::default()
        }
    }
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn get(&self, key: &(u32, Key)) -> Option<&Hit> {
        self.slots.get(key).map(|&i| &self.entries[i as usize].2)
    }
    pub(crate) fn contains_key(&self, key: &(u32, Key)) -> bool {
        self.slots.contains_key(key)
    }
    /// Records `hit`, replacing and returning a fact at the same position and rule.
    pub(crate) fn insert(&mut self, key: (u32, Key), hit: Hit) -> Option<Hit> {
        let next = self.entries.len() as u32;
        if self.entries.len() == self.entries.capacity() && key.0 > 4096 {
            // Facts per byte so far, over the whole text, with a margin.
            let predicted = (self.entries.len() as u64 * u64::from(self.text_len)
                / u64::from(key.0))
            .saturating_mul(9)
                / 8;
            let extra = (predicted as usize)
                .saturating_sub(self.entries.len())
                .max(1024);
            self.entries.reserve_exact(extra);
            self.slots.reserve(extra);
        }
        match self.slots.entry(key) {
            std::collections::hash_map::Entry::Occupied(slot) => Some(std::mem::replace(
                &mut self.entries[*slot.get() as usize].2,
                hit,
            )),
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(next);
                self.entries.push((key.0, key.1, hit));
                None
            }
        }
    }
    pub(crate) fn remove(&mut self, key: &(u32, Key)) -> Option<Hit> {
        let i = self.slots.remove(key)? as usize;
        let (_, _, hit) = self.entries.swap_remove(i);
        if let Some(&(at, key, _)) = self.entries.get(i) {
            self.slots.insert((at, key), i as u32);
        }
        Some(hit)
    }
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&(u32, Key), &mut Hit) -> bool) {
        let before = self.entries.len();
        self.entries
            .retain_mut(|(at, key, hit)| keep(&(*at, *key), hit));
        if self.entries.len() != before {
            self.slots.clear();
            for (i, &(at, key, _)) in self.entries.iter().enumerate() {
                self.slots.insert((at, key), i as u32);
            }
        }
    }
    pub(crate) fn values(&self) -> impl Iterator<Item = &Hit> {
        self.entries.iter().map(|(_, _, hit)| hit)
    }
    /// Empties the log and keeps its capacity for the next parse.
    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.slots.clear();
    }
    pub(crate) fn into_entries(self) -> Vec<(u32, Key, Hit)> {
        self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SubtreeId;
    use std::collections::HashMap;
    fn hit(end: u32) -> Hit {
        Hit {
            root: SubtreeId::NONE,
            root_offset: 0,
            end_offset: end,
            observed_offset: u64::from(end),
            furthest_offset: end,
            depth: 0,
        }
    }
    #[test]
    fn fact_log_matches_hash_map_model() {
        let mut state = 0x2545_f491_u64;
        let mut next = move |bound: u32| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % u64::from(bound)) as u32
        };
        let mut log = FactLog::default();
        let mut model: HashMap<(u32, Key), Hit> = HashMap::new();
        for step in 0..10_000 {
            let key = (
                next(200),
                Key {
                    rule: next(3) as u16,
                    minimum: 0,
                    in_token: 0,
                },
            );
            match next(10) {
                0..=4 => assert!(log.insert(key, hit(step)) == model.insert(key, hit(step))),
                5..=7 => assert!(log.remove(&key) == model.remove(&key)),
                8 if step % 500 == 0 => {
                    log.retain(|(at, _), _| at % 2 == 0);
                    model.retain(|(at, _), _| at % 2 == 0);
                }
                _ => {
                    assert!(log.get(&key) == model.get(&key));
                    assert_eq!(log.contains_key(&key), model.contains_key(&key));
                }
            }
            assert_eq!(log.len(), model.len());
        }
        let mut entries = log.clone().into_entries();
        entries.sort_unstable_by_key(|&(at, key, _)| (at, key));
        let mut expected: Vec<_> = model.into_iter().map(|((a, k), h)| (a, k, h)).collect();
        expected.sort_unstable_by_key(|&(at, key, _)| (at, key));
        assert!(entries == expected);
        assert_eq!(log.values().count(), entries.len());
        log.clear();
        assert_eq!(log.len(), 0);
        assert!(log.get(&(0, entries[0].1)).is_none());
    }
}
