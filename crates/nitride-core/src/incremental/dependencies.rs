//! Bucketed persistent interval index. Edits share untouched subtrees and lazily
//! displace suffix positions; cold construction packs records without path copies.
use crate::SubtreeId;
use std::sync::Arc;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Key {
    pub rule: u16,
    pub minimum: u32,
    pub in_token: u8,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Hit {
    pub root: SubtreeId,
    pub root_offset: u32,
    pub end_offset: u32,
    pub observed_offset: u64,
    pub furthest_offset: u32,
    pub depth: u32,
}
#[derive(Clone)]
struct Entry {
    at: i64,
    key: Key,
    hit: Hit,
}
#[derive(Clone)]
struct SmallEntry {
    // Two naturally aligned halves preserve every handle bit without an
    // eight-byte alignment requirement on each record.
    root_chunk: u32,
    root_slot: u32,
    at: u16,
    end: u16,
    root_offset: u16,
    observed_extra: u16,
    furthest_extra: i16,
    key_depth: u16,
}
#[derive(Clone)]
struct PackedLeaf {
    base: i64,
    keys: Vec<Key>,
    entries: Vec<SmallEntry>,
}
impl PackedLeaf {
    fn new(entries: &[Entry]) -> Option<Self> {
        let base = entries.first()?.at;
        let mut keys = Vec::new();
        let mut packed = Vec::with_capacity(entries.len());
        for entry in entries {
            let hit = entry.hit;
            if hit.depth > 2047 {
                return None;
            }
            let key = if let Some(i) = keys.iter().position(|k| *k == entry.key) {
                i
            } else {
                if keys.len() >= 32 {
                    return None;
                }
                keys.push(entry.key);
                keys.len() - 1
            };
            packed.push(SmallEntry {
                root_chunk: hit.root.chunk(),
                root_slot: hit.root.slot() as u32,
                at: u16::try_from(entry.at - base).ok()?,
                end: u16::try_from(hit.end_offset).ok()?,
                root_offset: u16::try_from(hit.root_offset).ok()?,
                observed_extra: u16::try_from(
                    hit.observed_offset.checked_sub(u64::from(hit.end_offset))?,
                )
                .ok()?,
                furthest_extra: i16::try_from(
                    i64::from(hit.furthest_offset) - i64::from(hit.end_offset),
                )
                .ok()?,
                key_depth: (key as u16) << 11 | hit.depth as u16,
            });
        }
        keys.shrink_to_fit();
        Some(Self {
            base,
            keys,
            entries: packed,
        })
    }
    fn entry(&self, e: &SmallEntry) -> Entry {
        Entry {
            at: self.base + i64::from(e.at),
            key: self.keys[(e.key_depth >> 11) as usize],
            hit: Hit {
                root: SubtreeId::new(e.root_chunk, e.root_slot),
                root_offset: u32::from(e.root_offset),
                end_offset: u32::from(e.end),
                observed_offset: u64::from(e.end) + u64::from(e.observed_extra),
                furthest_offset: (i64::from(e.end) + i64::from(e.furthest_extra)) as u32,
                depth: u32::from(e.key_depth & 2047),
            },
        }
    }
}
#[derive(Clone)]
enum LeafStore {
    Wide(Vec<Entry>),
    Packed(PackedLeaf),
}
impl LeafStore {
    fn new(entries: Vec<Entry>) -> Self {
        PackedLeaf::new(&entries).map_or_else(|| Self::Wide(entries), Self::Packed)
    }
    fn entries(&self) -> Vec<Entry> {
        match self {
            Self::Wide(entries) => entries.clone(),
            Self::Packed(leaf) => leaf.entries.iter().map(|e| leaf.entry(e)).collect(),
        }
    }
    fn shift(&mut self, delta: i64) {
        match self {
            Self::Wide(entries) => {
                for e in entries {
                    e.at += delta;
                }
            }
            Self::Packed(leaf) => leaf.base += delta,
        }
    }
    fn get(&self, at: i64, key: Key) -> Option<Hit> {
        match self {
            Self::Wide(entries) => entries
                .binary_search_by_key(&(at, key), |e| (e.at, e.key))
                .ok()
                .map(|i| entries[i].hit),
            Self::Packed(leaf) => {
                let at = u32::try_from(at - leaf.base).ok()?;
                leaf.entries
                    .binary_search_by_key(&(at, key), |e| {
                        (u32::from(e.at), leaf.keys[(e.key_depth >> 11) as usize])
                    })
                    .ok()
                    .map(|i| leaf.entry(&leaf.entries[i]).hit)
            }
        }
    }
    fn visit_allocations(&self, visit: &mut crate::syntax::memory::Visitor<'_>) {
        use crate::syntax::memory::{self, Class};
        match self {
            Self::Wide(entries) => {
                if let Some((id, size)) = memory::vector(entries) {
                    visit(Class::Index, id, size);
                }
            }
            Self::Packed(leaf) => {
                if let Some((id, size)) = memory::vector(&leaf.keys) {
                    visit(Class::Index, id, size);
                }
                if let Some((id, size)) = memory::vector(&leaf.entries) {
                    visit(Class::Index, id, size);
                }
            }
        }
    }
}
#[derive(Clone)]
enum Kind {
    Leaf(Box<LeafStore>),
    Branch(Arc<Node>, Arc<Node>),
}
#[derive(Clone)]
struct Node {
    kind: Kind,
    min_at: i64,
    max_at: i64,
    max_dependency: i64,
    height: u16,
    lazy: i64,
}
type Link = Option<Arc<Node>>;
#[derive(Clone, Default)]
pub(crate) struct Index {
    root: Link,
    edit_map: Option<([u32; 2], i64)>,
}
const BUCKET: usize = 32;
fn leaf(mut entries: Vec<Entry>) -> Link {
    entries.shrink_to_fit();
    if entries.is_empty() {
        return None;
    }
    Some(Arc::new(Node {
        min_at: entries[0].at,
        max_at: entries.last().unwrap().at,
        max_dependency: entries
            .iter()
            .map(|e| e.at + e.hit.observed_offset as i64)
            .max()
            .unwrap(),
        height: 1,
        lazy: 0,
        kind: Kind::Leaf(Box::new(LeafStore::new(entries))),
    }))
}
fn branch(left: Arc<Node>, right: Arc<Node>) -> Arc<Node> {
    Arc::new(Node {
        min_at: left.min_at,
        max_at: right.max_at,
        max_dependency: left.max_dependency.max(right.max_dependency),
        height: 1 + left.height.max(right.height),
        lazy: 0,
        kind: Kind::Branch(left, right),
    })
}
fn shift(node: &Arc<Node>, delta: i64) -> Arc<Node> {
    let mut n = (**node).clone();
    n.min_at += delta;
    n.max_at += delta;
    n.max_dependency += delta;
    n.lazy += delta;
    Arc::new(n)
}
fn push(node: &Arc<Node>) -> Node {
    let mut n = (**node).clone();
    if n.lazy == 0 {
        return n;
    }
    n.kind = match n.kind {
        Kind::Leaf(mut store) => {
            store.shift(n.lazy);
            Kind::Leaf(store)
        }
        Kind::Branch(a, b) => Kind::Branch(shift(&a, n.lazy), shift(&b, n.lazy)),
    };
    n.lazy = 0;
    n
}
fn balance(left: Arc<Node>, right: Arc<Node>) -> Arc<Node> {
    if left.height > right.height + 1 {
        let Kind::Branch(a, b) = push(&left).kind else {
            unreachable!()
        };
        if a.height >= b.height {
            return branch(a, branch(b, right));
        }
        let Kind::Branch(c, d) = push(&b).kind else {
            unreachable!()
        };
        return branch(branch(a, c), branch(d, right));
    }
    if right.height > left.height + 1 {
        let Kind::Branch(a, b) = push(&right).kind else {
            unreachable!()
        };
        if b.height >= a.height {
            return branch(branch(left, a), b);
        }
        let Kind::Branch(c, d) = push(&a).kind else {
            unreachable!()
        };
        return branch(branch(left, c), branch(d, b));
    }
    branch(left, right)
}
fn join(left: Link, right: Link) -> Link {
    let (a, b) = match (left, right) {
        (None, b) => return b,
        (a, None) => return a,
        (Some(a), Some(b)) => (a, b),
    };
    if a.height > b.height + 1 {
        let Kind::Branch(c, d) = push(&a).kind else {
            unreachable!()
        };
        return Some(balance(c, join(Some(d), Some(b)).unwrap()));
    }
    if b.height > a.height + 1 {
        let Kind::Branch(c, d) = push(&b).kind else {
            unreachable!()
        };
        return Some(balance(join(Some(a), Some(c)).unwrap(), d));
    }
    Some(branch(a, b))
}
fn pack(entries: Vec<Entry>) -> Link {
    if entries.len() <= BUCKET {
        return leaf(entries);
    }
    pack_slice(&entries)
}
/// Splits sorted entries without moving them; only leaves copy theirs.
fn pack_slice(entries: &[Entry]) -> Link {
    pack_by(entries, |e| e.at, Entry::clone)
}
/// Splits entries sorted by `at` into a balanced tree, converting only the
/// entries of each leaf.
fn pack_by<T>(
    entries: &[T],
    at: impl Fn(&T) -> i64 + Copy,
    entry: impl Fn(&T) -> Entry + Copy,
) -> Link {
    if entries.len() <= BUCKET {
        return leaf(entries.iter().map(entry).collect());
    }
    let mut middle = entries.len() / 2;
    while middle < entries.len() && at(&entries[middle - 1]) == at(&entries[middle]) {
        middle += 1;
    }
    if middle == entries.len() {
        middle = entries.len() / 2;
        while middle > 0 && at(&entries[middle - 1]) == at(&entries[middle]) {
            middle -= 1;
        }
    }
    if middle == 0 {
        return leaf(entries.iter().map(entry).collect());
    }
    let (left, right) = entries.split_at(middle);
    join(pack_by(left, at, entry), pack_by(right, at, entry))
}
fn insert(node: &Link, entry: Entry) -> Link {
    let Some(node) = node else {
        return leaf(vec![entry]);
    };
    let n = push(node);
    match n.kind {
        Kind::Leaf(store) => {
            let mut entries = store.entries();
            match entries.binary_search_by_key(&(entry.at, entry.key), |e| (e.at, e.key)) {
                Ok(i) => entries[i] = entry,
                Err(i) => entries.insert(i, entry),
            }
            pack(entries)
        }
        Kind::Branch(a, b) => {
            if entry.at <= a.max_at {
                join(insert(&Some(a), entry), Some(b))
            } else {
                join(Some(a), insert(&Some(b), entry))
            }
        }
    }
}
fn edit(node: &Link, range: [u32; 2], delta: i64, visits: &mut usize) -> Link {
    let node = node.as_ref()?;
    *visits += 1;
    if node.max_dependency < i64::from(range[0]) {
        return Some(node.clone());
    }
    if node.min_at > i64::from(range[1]) {
        return Some(shift(node, delta));
    }
    match push(node).kind {
        Kind::Leaf(store) => leaf(
            store
                .entries()
                .into_iter()
                .filter_map(|mut e| {
                    if e.at > i64::from(range[1]) {
                        e.at += delta;
                        Some(e)
                    } else if e.at + (e.hit.observed_offset as i64) < i64::from(range[0]) {
                        Some(e)
                    } else {
                        None
                    }
                })
                .collect(),
        ),
        Kind::Branch(a, b) => join(
            edit(&Some(a), range, delta, visits),
            edit(&Some(b), range, delta, visits),
        ),
    }
}
impl Index {
    pub(crate) fn visit_allocations(&self, visit: &mut crate::syntax::memory::Visitor<'_>) {
        use crate::syntax::memory::{self, Class};
        let mut pending = Vec::new();
        if let Some(node) = &self.root {
            pending.push(node);
        }
        while let Some(node) = pending.pop() {
            let (id, size) = memory::arc(node);
            if !visit(Class::Index, id, size) {
                continue;
            }
            match &node.kind {
                Kind::Leaf(store) => {
                    if visit(
                        Class::Index,
                        store.as_ref() as *const LeafStore as usize,
                        std::mem::size_of::<LeafStore>(),
                    ) {
                        store.visit_allocations(visit);
                    }
                }
                Kind::Branch(a, b) => {
                    pending.push(a);
                    pending.push(b);
                }
            }
        }
    }
    pub(crate) fn from_entries(entries: impl IntoIterator<Item = (u32, Key, Hit)>) -> Self {
        let mut entries: Vec<_> = entries
            .into_iter()
            .map(|(at, key, hit)| Entry {
                at: i64::from(at),
                key,
                hit,
            })
            .collect();
        // Positions and keys are unique, so stability is irrelevant.
        entries.sort_unstable_by_key(|e| (e.at, e.key));
        Self {
            root: pack(entries),
            edit_map: None,
        }
    }
    /// [`Index::from_entries`] for facts in any order at positions up to
    /// `text_len`: a counting sort into 16-byte blocks of positions, then a
    /// sort within each block's few facts, so building takes linear time.
    pub(crate) fn from_unsorted(mut entries: Vec<(u32, Key, Hit)>, text_len: u32) -> Self {
        const BLOCK: u32 = 4;
        // Each entry's place in block order, then the permutation applied in
        // place by following its cycles, so no second copy is made.
        let mut starts = vec![0_u32; (text_len >> BLOCK) as usize + 2];
        for &(at, ..) in &entries {
            starts[(at >> BLOCK) as usize + 1] += 1;
        }
        for i in 1..starts.len() {
            starts[i] += starts[i - 1];
        }
        let mut places: Vec<u32> = entries
            .iter()
            .map(|&(at, ..)| {
                let slot = &mut starts[(at >> BLOCK) as usize];
                *slot += 1;
                *slot - 1
            })
            .collect();
        drop(starts);
        for i in 0..entries.len() {
            while places[i] as usize != i {
                let to = places[i] as usize;
                entries.swap(i, to);
                places.swap(i, to);
            }
        }
        drop(places);
        let mut from = 0;
        while from < entries.len() {
            let block = entries[from].0 >> BLOCK;
            let to = from
                + entries[from..]
                    .iter()
                    .take_while(|e| e.0 >> BLOCK == block)
                    .count();
            entries[from..to].sort_unstable_by_key(|e| (e.0, e.1));
            from = to;
        }
        Self {
            root: pack_by(
                &entries,
                |e| i64::from(e.0),
                |&(at, key, hit)| Entry {
                    at: i64::from(at),
                    key,
                    hit,
                },
            ),
            edit_map: None,
        }
    }
    /// This index with `entries` inserted, built in one pass; a new entry
    /// replaces an existing one at the same position and key, as `insert` does.
    pub(crate) fn merged(&self, entries: impl IntoIterator<Item = (u32, Key, Hit)>) -> Self {
        fn collect(node: &Link, displacement: i64, out: &mut Vec<Entry>) {
            let Some(node) = node else {
                return;
            };
            let displacement = displacement + node.lazy;
            match &node.kind {
                Kind::Leaf(store) => out.extend(store.entries().into_iter().map(|mut e| {
                    e.at += displacement;
                    e
                })),
                Kind::Branch(a, b) => {
                    collect(&Some(a.clone()), displacement, out);
                    collect(&Some(b.clone()), displacement, out);
                }
            }
        }
        let mut all: Vec<_> = entries
            .into_iter()
            .map(|(at, key, hit)| Entry {
                at: i64::from(at),
                key,
                hit,
            })
            .collect();
        let added = all.len();
        collect(&self.root, 0, &mut all);
        // A stable sort keeps each new entry ahead of the existing one it replaces.
        all[..added].sort_unstable_by_key(|e| (e.at, e.key));
        all.sort_by_key(|e| (e.at, e.key));
        all.dedup_by_key(|e| (e.at, e.key));
        Self {
            root: pack(all),
            edit_map: self.edit_map,
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.root.is_none()
    }
    pub(crate) fn previous_position(&self, at: u32) -> Option<u32> {
        let (range, delta) = self.edit_map?;
        let value = i64::from(at);
        if value >= i64::from(range[1]) + delta {
            u32::try_from(value - delta).ok()
        } else if at <= range[0] {
            Some(at)
        } else {
            None
        }
    }
    /// Approximate old point for construction hints only. This does not validate
    /// any recognition or authorize reuse within the edited interval.
    pub(crate) fn construction_position(&self, at: u32) -> Option<u32> {
        self.previous_position(at).or_else(|| {
            let (range, _) = self.edit_map?;
            Some(range[0] + at.saturating_sub(range[0]).min(range[1] - range[0]))
        })
    }
    pub(crate) fn current_position(&self, at: u32) -> Option<u32> {
        let (range, delta) = self.edit_map?;
        if at >= range[1] {
            u32::try_from(i64::from(at) + delta).ok()
        } else if at <= range[0] {
            Some(at)
        } else {
            None
        }
    }
    pub(crate) fn get(&self, at: u32, key: Key) -> Option<Hit> {
        let mut node = self.root.as_deref()?;
        // The root's bounds are absolute: most lookups past the facts end here.
        if !(node.min_at..=node.max_at).contains(&i64::from(at)) {
            return None;
        }
        let mut displacement = 0;
        loop {
            displacement += node.lazy;
            match &node.kind {
                Kind::Leaf(store) => return store.get(i64::from(at) - displacement, key),
                Kind::Branch(a, b) => {
                    node = if i64::from(at) <= a.max_at + displacement {
                        a
                    } else {
                        b
                    };
                }
            }
        }
    }
    pub(crate) fn insert(&mut self, at: u32, key: Key, hit: Hit) {
        if self.get(at, key) != Some(hit) {
            self.root = insert(
                &self.root,
                Entry {
                    at: i64::from(at),
                    key,
                    hit,
                },
            );
        }
    }
    pub(crate) fn edit(&self, range: [u32; 2], delta: i64) -> (Self, usize) {
        let mut visits = 0;
        let root = edit(&self.root, range, delta, &mut visits);
        (
            Self {
                root,
                edit_map: Some((range, delta)),
            },
            visits,
        )
    }
    pub(crate) fn retain_mapped(&self, map: &impl Fn(SubtreeId) -> Option<SubtreeId>) -> Self {
        fn visit(node: &Link, map: &impl Fn(SubtreeId) -> Option<SubtreeId>) -> Link {
            match push(node.as_ref()?).kind {
                Kind::Leaf(store) => leaf(
                    store
                        .entries()
                        .into_iter()
                        .filter_map(|mut e| {
                            e.hit.root = map(e.hit.root)?;
                            Some(e)
                        })
                        .collect(),
                ),
                Kind::Branch(a, b) => join(visit(&Some(a), map), visit(&Some(b), map)),
            }
        }
        Self {
            root: visit(&self.root, map),
            edit_map: self.edit_map,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key() -> Key {
        Key {
            rule: 1,
            minimum: 0,
            in_token: 0,
        }
    }
    fn hit(observed: u32) -> Hit {
        Hit {
            root: crate::SubtreeId::NONE,
            root_offset: 0,
            end_offset: 2,
            observed_offset: u64::from(observed),
            furthest_offset: 2,
            depth: 0,
        }
    }
    #[test]
    fn edit_invalidates_lookahead_and_shifts_suffix_without_touching_old_index() {
        let mut index = Index::default();
        index.insert(0, key(), hit(20));
        index.insert(30, key(), hit(2));
        let (edited, _) = index.edit([10, 10], 3);
        assert!(edited.get(0, key()).is_none());
        assert!(edited.get(30, key()).is_none());
        assert!(edited.get(33, key()).is_some());
        assert!(index.get(30, key()).is_some());
    }
    #[test]
    fn eof_and_zero_width_boundaries_invalidate() {
        let mut index = Index::default();
        index.insert(0, key(), hit(3));
        assert!(index.edit([3, 3], 1).0.get(0, key()).is_none());
    }
    #[test]
    fn local_edit_does_not_scan_the_whole_index() {
        let mut index = Index::default();
        for at in (0..100_000).map(|i| i * 10) {
            index.insert(at, key(), hit(2));
        }
        let (edited, visits) = index.edit([500_001, 500_002], 5);
        assert!(visits < 200, "visited {visits}");
        assert!(edited.get(500_015, key()).is_some());
        assert!(edited.get(500_000, key()).is_none());
        assert!(index.get(500_010, key()).is_some());
    }
    #[test]
    fn regular_checkpoint_metadata_stays_compact() {
        let index = Index::from_entries((0..100_000).map(|i| (i * 10, key(), hit(2))));
        let mut counter = crate::syntax::memory::Counter::default();
        index.visit_allocations(&mut |kind, id, bytes| counter.add(kind, id, bytes));
        assert!(
            counter.total() < 100_000 * 32,
            "{} bytes for regular checkpoints",
            counter.total()
        );
    }
    #[test]
    fn compact_storage_preserves_wide_observations() {
        let mut values = Vec::new();
        for i in 0..64_u32 {
            let mut value = hit(2);
            match i % 5 {
                0 => value.depth = 100_000,
                1 => {
                    value.root_offset = 70_000;
                    value.end_offset = 100_000;
                    value.observed_offset = 100_001;
                }
                2 => value.observed_offset = u64::from(u32::MAX) + 1,
                3 => {
                    value.end_offset = 100_000;
                    value.observed_offset = 100_000;
                    value.furthest_offset = 0;
                }
                _ => {}
            }
            values.push((i * 10, key(), value));
        }
        let index = Index::from_entries(values.iter().copied());
        for (at, key, value) in values {
            assert_eq!(index.get(at, key).map(|h| h == value), Some(true));
        }
    }
    #[test]
    fn from_unsorted_matches_from_entries() {
        let mut state = 0x9e37_79b9_u64;
        let mut next = move |bound: u32| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % u64::from(bound)) as u32
        };
        let mut entries = Vec::new();
        for at in 0..500 {
            if next(3) == 0 {
                continue;
            }
            for rule in 0..=next(3) as u16 {
                let key = Key {
                    rule,
                    minimum: next(2),
                    in_token: 0,
                };
                if !entries.iter().any(|&(a, k, _)| a == at && k == key) {
                    entries.push((at, key, hit(at + 2 + next(20))));
                }
            }
        }
        for i in (1..entries.len()).rev() {
            entries.swap(i, next(i as u32 + 1) as usize);
        }
        let eager = Index::from_entries(entries.iter().copied());
        let deferred = Index::from_unsorted(entries.clone(), 500);
        for &(at, key, value) in &entries {
            assert!(deferred.get(at, key) == Some(value), "{at}");
            assert!(eager.get(at, key) == Some(value), "{at}");
        }
        for _ in 0..100 {
            let (at, rule) = (next(520), next(6) as u16);
            let key = Key {
                rule,
                minimum: 0,
                in_token: 0,
            };
            assert!(deferred.get(at, key) == eager.get(at, key), "{at}");
        }
    }
    #[test]
    fn compact_and_wide_leaf_boundaries_preserve_all_handle_bits_and_offsets() {
        let mut values = Vec::new();
        for (i, width) in [65535, 65536, u32::MAX].into_iter().enumerate() {
            let mut value = hit(2);
            value.root = SubtreeId::new(u32::MAX - 1, u32::MAX);
            value.end_offset = width;
            value.observed_offset = u64::from(width) + 1;
            value.furthest_offset = width;
            values.push((i as u32 * 70000, key(), value));
        }
        let index = Index::from_entries(values.iter().copied());
        let (edited, _) = index.edit([35000, 35001], 7);
        for (at, key, value) in values {
            assert!(index.get(at, key) == Some(value));
            if at == 0 {
                // The first fact observes across the edit and must be invalidated.
                assert!(edited.get(at, key).is_none());
            } else {
                assert!(edited.get(at + 7, key) == Some(value));
            }
        }
    }
}
