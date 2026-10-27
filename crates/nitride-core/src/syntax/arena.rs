//! Stable chunk/slot handles; snapshots share a balanced ownership directory.
//! Chunks contain raw IDs, never owning back-references, so destruction has no
//! dependency-chain recursion or ownership cycles.
use super::sequence::{Measured, Sequence};
use crate::Diagnostic;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubtreeId(u64);
impl SubtreeId {
    pub(crate) const NONE: Self = Self(u64::MAX);
    fn new(chunk: u32, slot: u32) -> Self {
        Self((u64::from(chunk) << 32) | u64::from(slot))
    }
    fn chunk(self) -> u32 {
        (self.0 >> 32) as u32
    }
    fn slot(self) -> usize {
        self.0 as u32 as usize
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Child {
    pub node: SubtreeId,
    pub extent: u32,
    pub label: u16,
}
impl Measured for Child {
    fn width(&self) -> u64 {
        u64::from(self.extent)
    }
}
#[derive(Clone, Copy)]
pub(crate) struct StoredNode {
    pub rule: u16,
    pub width: u32,
    first: u32,
    count: u32,
}
impl StoredNode {
    pub(crate) fn child_count(self) -> u32 {
        self.count & !LARGE
    }
}
const LARGE: u32 = 1 << 31;
const CHUNK_BYTES: usize = 64 * 1024;
static NEXT_CHUNK: AtomicU32 = AtomicU32::new(1);
fn next_chunk_id(ids: &AtomicU32) -> u32 {
    let mut id = ids.load(Ordering::Relaxed);
    loop {
        if id == u32::MAX {
            return id;
        }
        match ids.compare_exchange_weak(id, id + 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return id,
            Err(current) => id = current,
        }
    }
}

struct Chunk {
    id: u32,
    nodes: Vec<StoredNode>,
    edges: Vec<Child>,
    large: Vec<Sequence<Child>>,
}
impl Measured for Arc<Chunk> {
    fn key(&self) -> u32 {
        self.id
    }
    fn width(&self) -> u64 {
        self.payload() as u64
    }
}
impl Chunk {
    fn new() -> Self {
        let id = next_chunk_id(&NEXT_CHUNK);
        Self {
            id,
            nodes: Vec::new(),
            edges: Vec::new(),
            large: Vec::new(),
        }
    }
    fn payload(&self) -> usize {
        self.nodes.len() * std::mem::size_of::<StoredNode>()
            + self.edges.len() * std::mem::size_of::<Child>()
            + self
                .large
                .iter()
                .map(|s| s.len() * std::mem::size_of::<Child>())
                .sum::<usize>()
    }
    fn children(&self, node: StoredNode) -> Children<'_> {
        if node.count & LARGE != 0 {
            Children::Large(Box::new(self.large[node.first as usize].iter().copied()))
        } else {
            Children::Small(
                self.edges[node.first as usize..(node.first + node.count) as usize]
                    .iter()
                    .copied(),
            )
        }
    }
}
pub(crate) enum Children<'a> {
    Small(std::iter::Copied<std::slice::Iter<'a, Child>>),
    Large(Box<dyn Iterator<Item = Child> + 'a>),
}
impl Iterator for Children<'_> {
    type Item = Child;
    fn next(&mut self) -> Option<Child> {
        match self {
            Self::Small(iter) => iter.next(),
            Self::Large(iter) => iter.next(),
        }
    }
}
pub(crate) struct Relocation {
    ids: Vec<u32>,
    index: ChunkIndex,
    maps: Vec<Vec<u32>>,
    live: Vec<Vec<bool>>,
    pub removed_bytes: usize,
    pub copied_bytes: usize,
    evacuated: Option<std::collections::HashMap<SubtreeId, SubtreeId>>,
}
impl Relocation {
    pub(crate) fn get(&self, id: SubtreeId) -> Option<SubtreeId> {
        if let Some(map) = &self.evacuated {
            return map.get(&id).copied();
        }
        let chunk = self.index.get(&self.ids, id.chunk())?;
        if !*self.live[chunk].get(id.slot())? {
            return None;
        }
        Some(SubtreeId::new(id.chunk(), self.maps[chunk][id.slot()]))
    }
}
struct Evacuated {
    arena: PublishedArena,
    mapping: std::collections::HashMap<SubtreeId, SubtreeId>,
    copied: usize,
}
#[derive(Clone)]
pub(crate) struct PublishedArena {
    chunks: Sequence<Arc<Chunk>>,
    pub labels: Arc<[&'static str]>,
}
impl PublishedArena {
    fn chunk(&self, id: SubtreeId) -> Option<&Chunk> {
        self.chunks.find_key(id.chunk()).map(Arc::as_ref)
    }
    pub(crate) fn node(&self, id: SubtreeId) -> Option<StoredNode> {
        self.chunk(id)?.nodes.get(id.slot()).copied()
    }
    pub(crate) fn children(&self, id: SubtreeId) -> Children<'_> {
        let chunk = self.chunk(id).expect("owned subtree chunk");
        chunk.children(chunk.nodes[id.slot()])
    }
    /// Reclaim unreachable chunks; slide records only in exclusively owned chunks.
    /// Shared ownership requires evacuation into a new graph at maintenance time.
    /// Old readers retain their original IDs and immutable chunk contents.
    pub(crate) fn compact(
        mut self,
        roots: &mut [SubtreeId],
    ) -> Result<(Self, Relocation), Diagnostic> {
        let chunks: Vec<_> = self.chunks.iter().cloned().collect();
        self.chunks = Sequence::default();
        let ids: Vec<_> = chunks.iter().map(|c| c.id).collect();
        let index = ChunkIndex::new(&ids);
        let mut live: Vec<Vec<bool>> = chunks.iter().map(|c| vec![false; c.nodes.len()]).collect();
        let mut pending = roots.to_vec();
        while let Some(id) = pending.pop() {
            let c = index.get(&ids, id.chunk()).expect("owned reachable chunk");
            if live[c][id.slot()] {
                continue;
            }
            live[c][id.slot()] = true;
            pending.extend(
                chunks[c]
                    .children(chunks[c].nodes[id.slot()])
                    .map(|e| e.node),
            );
        }
        let exclusive: Vec<_> = chunks.iter().map(|c| Arc::strong_count(c) == 1).collect();
        if exclusive.iter().any(|v| !v) {
            // Maintenance may change the latest snapshot's IDs; every old reader
            // retains its immutable representation. Rebuild the reachable graph
            // rather than pinning sparse chunks (including their obsolete lists).
            self.chunks = Sequence::from_vec(chunks);
            let before = self.payload_bytes();
            let rebuilt = self.evacuate(roots, ArenaBuilder::new(self.labels.clone()))?;
            let mapping = rebuilt.mapping;
            let copied = rebuilt.copied;
            for root in roots {
                *root = mapping[root];
            }
            let next = rebuilt.arena;
            let removed_bytes = before.saturating_sub(next.payload_bytes());
            return Ok((
                next,
                Relocation {
                    index,
                    ids,
                    maps: Vec::new(),
                    live,
                    removed_bytes,
                    copied_bytes: copied,
                    evacuated: Some(mapping),
                },
            ));
        }
        let maps: Vec<Vec<u32>> = chunks
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut next = 0;
                live[i]
                    .iter()
                    .enumerate()
                    .map(|(slot, alive)| {
                        if !exclusive[i] {
                            slot as u32
                        } else if *alive {
                            let result = next;
                            next += 1;
                            result
                        } else {
                            u32::MAX
                        }
                    })
                    .take(c.nodes.len())
                    .collect()
            })
            .collect();
        let relocated = |id: SubtreeId| {
            let c = index.get(&ids, id.chunk()).unwrap();
            let slot = maps[c][id.slot()];
            assert_ne!(slot, u32::MAX);
            SubtreeId::new(id.chunk(), slot)
        };
        for root in roots {
            *root = relocated(*root);
        }
        let mut kept = Vec::new();
        let mut removed = 0;
        for (i, chunk) in chunks.into_iter().enumerate() {
            if !live[i].iter().any(|v| *v) {
                removed += chunk.payload();
                continue;
            }
            if !exclusive[i] {
                kept.push(chunk);
                continue;
            }
            let mut chunk =
                Arc::try_unwrap(chunk).unwrap_or_else(|_| unreachable!("exclusive chunk"));
            let before = chunk.payload();
            let old_edges = std::mem::take(&mut chunk.edges);
            let mut old_large = std::mem::take(&mut chunk.large);
            let mut nodes = 0;
            let mut edges = 0;
            let mut large = 0;
            for (old, alive) in live[i].iter().copied().enumerate() {
                if !alive {
                    continue;
                }
                let mut node = chunk.nodes[old];
                if node.count & LARGE != 0 {
                    let previous = std::mem::take(&mut old_large[node.first as usize]);
                    let items = previous
                        .iter()
                        .map(|edge| Child {
                            node: relocated(edge.node),
                            ..*edge
                        })
                        .collect();
                    node.first = large as u32;
                    chunk.large.push(Sequence::from_vec(items));
                    large += 1;
                } else {
                    let first = node.first as usize;
                    node.first = edges as u32;
                    for offset in 0..node.count as usize {
                        let old = old_edges[first + offset];
                        chunk.edges.push(Child {
                            node: relocated(old.node),
                            ..old
                        });
                        edges += 1;
                    }
                }
                chunk.nodes[nodes] = node;
                nodes += 1;
            }
            chunk.nodes.truncate(nodes);
            chunk.edges.truncate(edges);
            chunk.large.truncate(large);
            chunk.nodes.shrink_to_fit();
            chunk.edges.shrink_to_fit();
            chunk.large.shrink_to_fit();
            removed += before - chunk.payload();
            kept.push(Arc::new(chunk));
        }
        self.chunks = Sequence::from_vec(kept);
        Ok((
            self,
            Relocation {
                index,
                ids,
                maps,
                live,
                removed_bytes: removed,
                copied_bytes: 0,
                evacuated: None,
            },
        ))
    }
    fn evacuate(
        &self,
        roots: &[SubtreeId],
        mut builder: ArenaBuilder,
    ) -> Result<Evacuated, Diagnostic> {
        let mut visited = std::collections::HashSet::new();
        let mut order = Vec::new();
        let mut stack: Vec<_> = roots.iter().map(|id| (*id, false)).collect();
        while let Some((id, expanded)) = stack.pop() {
            if expanded {
                order.push(id);
                continue;
            }
            if !visited.insert(id) {
                continue;
            }
            stack.push((id, true));
            stack.extend(self.children(id).map(|child| (child.node, false)));
        }
        let mut mapping = std::collections::HashMap::with_capacity(order.len());
        let mut copied = 0;
        for id in order {
            let node = self.node(id).unwrap();
            let children: Vec<_> = self
                .children(id)
                .map(|edge| Child {
                    node: mapping[&edge.node],
                    ..edge
                })
                .collect();
            copied +=
                std::mem::size_of::<StoredNode>() + children.len() * std::mem::size_of::<Child>();
            let next = builder.add_reusing(node.rule, node.width, children.into_iter(), None)?;
            mapping.insert(id, next);
        }
        Ok(Evacuated {
            arena: builder.finish(),
            mapping,
            copied,
        })
    }
    pub(crate) fn sequence(&self, id: SubtreeId) -> Option<Sequence<Child>> {
        let chunk = self.chunk(id)?;
        let node = chunk.nodes[id.slot()];
        (node.count & LARGE != 0).then(|| chunk.large[node.first as usize].clone())
    }
    pub(crate) fn visit_allocations(&self, visit: &mut super::memory::Visitor<'_>) {
        use super::memory::{self, Class};
        self.chunks.visit_allocations(Class::Index, visit);
        let (id, size) = memory::arc(&self.labels);
        visit(Class::Syntax, id, size);
        for chunk in self.chunks.iter() {
            let (id, size) = memory::arc(chunk);
            if !visit(Class::Syntax, id, size) {
                continue;
            }
            if let Some((id, size)) = memory::vector(&chunk.nodes) {
                visit(Class::Syntax, id, size);
            }
            if let Some((id, size)) = memory::vector(&chunk.edges) {
                visit(Class::Syntax, id, size);
            }
            if let Some((id, size)) = memory::vector(&chunk.large) {
                visit(Class::Syntax, id, size);
            }
            for sequence in &chunk.large {
                sequence.visit_allocations(Class::Syntax, visit);
            }
        }
    }
    pub(crate) fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
    pub(crate) fn payload_bytes(&self) -> usize {
        self.chunks.width() as usize
    }
}
// Most unpublished nodes have only a few construction owners. Promote the
// whole private chunk on the exceptional wide count, preserving the u64 limit.
enum ReferenceCounts {
    Small(Vec<u32>),
    Large(Vec<u64>),
}
impl Default for ReferenceCounts {
    fn default() -> Self {
        Self::Small(Vec::new())
    }
}
impl ReferenceCounts {
    fn get(&self, slot: usize) -> Option<u64> {
        match self {
            Self::Small(values) => values.get(slot).map(|v| u64::from(*v)),
            Self::Large(values) => values.get(slot).copied(),
        }
    }
    fn set(&mut self, slot: usize, count: u64) {
        match self {
            Self::Small(values) if count <= u64::from(u32::MAX) => values[slot] = count as u32,
            Self::Small(values) => {
                let mut large: Vec<_> = values.iter().map(|v| u64::from(*v)).collect();
                large[slot] = count;
                *self = Self::Large(large);
            }
            Self::Large(values) => values[slot] = count,
        }
    }
    fn push(&mut self, count: u32) {
        match self {
            Self::Small(values) => values.push(count),
            Self::Large(values) => values.push(u64::from(count)),
        }
    }
    fn shrink_to_fit(&mut self) {
        match self {
            Self::Small(values) => values.shrink_to_fit(),
            Self::Large(values) => values.shrink_to_fit(),
        }
    }
}
struct MutableChunk {
    data: Chunk,
    references: ReferenceCounts,
    cache_references: Vec<u32>,
    weights: Vec<u32>,
    alive: usize,
    free_edges: [Vec<u32>; 33],
    free_large: Vec<u32>,
    sequence_charges: Vec<usize>,
}
impl MutableChunk {
    fn new() -> Self {
        Self {
            data: Chunk::new(),
            references: ReferenceCounts::default(),
            cache_references: Vec::new(),
            weights: Vec::new(),
            alive: 0,
            free_edges: std::array::from_fn(|_| Vec::new()),
            free_large: Vec::new(),
            sequence_charges: Vec::new(),
        }
    }
    fn shrink(&mut self) {
        self.data.nodes.shrink_to_fit();
        self.data.edges.shrink_to_fit();
        self.data.large.shrink_to_fit();
        self.references.shrink_to_fit();
        self.cache_references.shrink_to_fit();
        self.weights.shrink_to_fit();
    }
}
pub(crate) struct ArenaBuilder {
    published: PublishedArena,
    chunks: Vec<Option<MutableChunk>>,
    by_id: crate::parser::shared::hash::FastMap<u32, usize>,
    current: usize,
    first_id: u32,
    free_chunks: Vec<usize>,
    free_nodes: Vec<SubtreeId>,
    trail: Vec<SubtreeId>,
    /// Reused by `free_released` across calls.
    release_stack: Vec<SubtreeId>,
    copied_existing_bytes: u64,
    cache_only_bytes: u64,
}
impl ArenaBuilder {
    pub(crate) fn new(labels: Arc<[&'static str]>) -> Self {
        Self::from_published(&PublishedArena {
            chunks: Sequence::default(),
            labels,
        })
    }
    pub(crate) fn from_published(published: &PublishedArena) -> Self {
        let chunk = MutableChunk::new();
        let first_id = chunk.data.id;
        Self {
            published: published.clone(),
            chunks: vec![Some(chunk)],
            by_id: [(first_id, 0)].into_iter().collect(),
            current: 0,
            first_id,
            free_chunks: Vec::new(),
            free_nodes: Vec::new(),
            trail: Vec::new(),
            release_stack: Vec::new(),
            copied_existing_bytes: 0,
            cache_only_bytes: 0,
        }
    }
    #[inline]
    fn current_chunk_index(&self, id: SubtreeId) -> Option<usize> {
        self.chunks
            .get(self.current)
            .and_then(Option::as_ref)
            .filter(|chunk| chunk.data.id == id.chunk())
            .map(|_| self.current)
    }
    fn find_new(&self, id: SubtreeId) -> Option<usize> {
        // Chunk ids only increase, so every unpublished chunk is at least
        // `first_id`; anything older belongs to the published directory.
        let index = id.chunk().checked_sub(self.first_id)? as usize;
        if self
            .chunks
            .get(index)
            .and_then(Option::as_ref)
            .is_some_and(|c| c.data.id == id.chunk())
        {
            return Some(index);
        }
        if let Some(index) = self.current_chunk_index(id) {
            return Some(index);
        }
        self.by_id.get(&id.chunk()).copied()
    }
    pub(crate) fn node(&self, id: SubtreeId) -> Option<StoredNode> {
        if let Some(index) = self.find_new(id) {
            let c = self.chunks[index].as_ref()?;
            if c.references.get(id.slot())? == 0 {
                return None;
            }
            return c.data.nodes.get(id.slot()).copied();
        }
        self.published.node(id)
    }
    pub(crate) fn children(&self, id: SubtreeId) -> Children<'_> {
        if let Some(index) = self.find_new(id) {
            let c = self.chunks[index].as_ref().unwrap();
            assert_ne!(c.references.get(id.slot()).unwrap(), 0);
            return c.data.children(c.data.nodes[id.slot()]);
        }
        self.published.children(id)
    }
    pub(crate) fn sequence(&self, id: SubtreeId) -> Option<Sequence<Child>> {
        if let Some(index) = self.find_new(id) {
            let c = self.chunks[index].as_ref()?;
            let node = self.node(id)?;
            return (node.count & LARGE != 0).then(|| c.data.large[node.first as usize].clone());
        }
        self.published.sequence(id)
    }
    pub(crate) fn weight(&self, id: SubtreeId) -> u32 {
        self.find_new(id)
            .map_or(0, |i| self.chunks[i].as_ref().unwrap().weights[id.slot()])
    }
    pub(crate) fn mark(&self) -> usize {
        self.trail.len()
    }
    pub(crate) fn restore(&mut self, mark: usize) {
        while self.trail.len() > mark {
            let id = self.trail.pop().unwrap();
            self.release(id);
        }
    }
    pub(crate) fn adopt(&mut self, id: SubtreeId) {
        if id != SubtreeId::NONE && self.find_new(id).is_some() {
            self.trail.push(id);
        }
    }
    pub(crate) fn commit(&mut self, mark: usize, root: SubtreeId) {
        if root == SubtreeId::NONE {
            return;
        }
        if self.find_new(root).is_none() {
            self.restore(mark);
            return;
        }
        assert_eq!(self.trail.pop(), Some(root));
        self.restore(mark);
        self.trail.push(root);
    }
    pub(crate) fn clone_temporary(&mut self, id: SubtreeId) {
        self.retain(id);
        self.adopt(id);
    }
    pub(crate) fn replace_last_temporary(&mut self, old: SubtreeId, new: SubtreeId) {
        if self.find_new(old).is_none() {
            return;
        }
        assert_eq!(self.trail.pop(), Some(old));
        self.retain(new);
        self.adopt(new);
        self.release(old);
    }
    fn adjust(&mut self, id: SubtreeId, strong: i64, cache: i32) -> Option<(usize, bool)> {
        if id == SubtreeId::NONE {
            return None;
        }
        let index = self.find_new(id)?;
        let c = self.chunks[index].as_mut().unwrap();
        let slot = id.slot();
        // Without cache pins before or after, no bytes are charged either way.
        if cache == 0 && c.cache_references[slot] == 0 {
            if let ReferenceCounts::Small(values) = &mut c.references {
                let references = values[slot];
                assert!(references > 0, "reference to released unpublished node");
                let next = i64::from(references) + strong;
                if let Ok(next) = u32::try_from(next) {
                    values[slot] = next;
                    return Some((index, next == 0));
                }
            }
        }
        let charged = |refs: u64, pins: u32| {
            if pins != 0 && refs == u64::from(pins) {
                u64::from(c.weights[slot])
            } else {
                0
            }
        };
        let references = c.references.get(slot).unwrap();
        let before = charged(references, c.cache_references[slot]);
        assert!(references > 0, "reference to released unpublished node");
        let references = if strong >= 0 {
            references.checked_add(strong as u64).unwrap()
        } else {
            references.checked_sub((-strong) as u64).unwrap()
        };
        c.references.set(slot, references);
        c.cache_references[slot] = if cache >= 0 {
            c.cache_references[slot].checked_add(cache as u32).unwrap()
        } else {
            c.cache_references[slot]
                .checked_sub((-cache) as u32)
                .unwrap()
        };
        let after = charged(references, c.cache_references[slot]);
        self.cache_only_bytes = self.cache_only_bytes - before + after;
        Some((index, references == 0))
    }
    pub(crate) fn retain(&mut self, id: SubtreeId) {
        self.adjust(id, 1, 0);
    }
    pub(crate) fn retain_cache(&mut self, id: SubtreeId) {
        self.adjust(id, 1, 1);
    }
    pub(crate) fn release(&mut self, id: SubtreeId) {
        self.release_kind(id, false);
    }
    pub(crate) fn release_cache(&mut self, id: SubtreeId) {
        self.release_kind(id, true);
    }
    #[inline]
    fn release_kind(&mut self, id: SubtreeId, cache: bool) {
        // Most releases leave the node alive; only a free walks the subtree.
        if let Some((index, true)) = self.adjust(id, -1, if cache { -1 } else { 0 }) {
            self.free_released(id, index);
        }
    }
    /// Frees `id`, whose last reference was just released, and releases its
    /// unpublished children, freeing those that reach zero in turn.
    fn free_released(&mut self, id: SubtreeId, index: usize) {
        let mut stack = std::mem::take(&mut self.release_stack);
        let mut freed = Some((id, index));
        loop {
            if let Some((id, index)) = freed.take() {
                let first_new = self.first_id;
                let c = self.chunks[index].as_mut().unwrap();
                let node = c.data.nodes[id.slot()];
                // Published dependencies already belong to the shared directory.
                // They need neither temporary owners nor release-stack entries.
                stack.extend(
                    c.data
                        .children(node)
                        .map(|child| child.node)
                        .filter(|child| child.chunk() >= first_new),
                );
                if node.count & LARGE != 0 {
                    c.data.large[node.first as usize] = Sequence::default();
                    c.sequence_charges[node.first as usize] = 0;
                    c.free_large.push(node.first);
                } else if node.count != 0 {
                    c.free_edges[node.count as usize].push(node.first);
                }
                c.alive -= 1;
                self.free_nodes.push(id);
                if c.alive == 0 {
                    let chunk_id = c.data.id;
                    self.chunks[index] = None;
                    self.by_id.remove(&chunk_id);
                    self.free_chunks.push(index);
                }
            }
            let Some(child) = stack.pop() else {
                break;
            };
            if let Some((index, true)) = self.adjust(child, -1, 0) {
                freed = Some((child, index));
            }
        }
        self.release_stack = stack;
    }
    pub(crate) fn is_cache_only(&self, id: SubtreeId) -> bool {
        let Some(index) = self.find_new(id) else {
            return false;
        };
        let Some(c) = self.chunks[index].as_ref() else {
            return false;
        };
        let pins = c.cache_references[id.slot()];
        pins != 0 && c.references.get(id.slot()) == Some(u64::from(pins))
    }
    pub(crate) fn cache_only_bytes(&self) -> u64 {
        self.cache_only_bytes
    }
    pub(crate) fn trim_dead_ids(&mut self) {
        let by_id = &self.by_id;
        self.free_nodes.retain(|id| by_id.contains_key(&id.chunk()));
        if self.free_nodes.capacity() > self.free_nodes.len().saturating_mul(2) + 8192 {
            self.free_nodes.shrink_to_fit();
        }
    }
    #[cfg(test)]
    pub(crate) fn unpublished_payload_bytes(&self) -> usize {
        self.chunks.iter().flatten().map(|c| c.data.payload()).sum()
    }
    fn start_chunk(&mut self) {
        let chunk = MutableChunk::new();
        let id = chunk.data.id;
        let index = if let Some(index) = self.free_chunks.pop() {
            self.chunks[index] = Some(chunk);
            index
        } else {
            self.chunks.push(Some(chunk));
            self.chunks.len() - 1
        };
        self.by_id.insert(id, index);
        self.current = index;
    }
    pub(crate) fn seal(&mut self) {
        if let Some(c) = self.chunks.get_mut(self.current).and_then(Option::as_mut) {
            if c.data.nodes.is_empty() {
                return;
            }
            c.shrink();
        }
        self.start_chunk();
    }
    #[cfg(test)]
    pub(crate) fn add(
        &mut self,
        rule: u16,
        width: u32,
        children: Vec<Child>,
    ) -> Result<SubtreeId, Diagnostic> {
        self.add_reusing(rule, width, children.into_iter(), None)
    }
    pub(crate) fn add_reusing(
        &mut self,
        rule: u16,
        width: u32,
        children: impl ExactSizeIterator<Item = Child> + DoubleEndedIterator + Clone,
        prior: Option<SubtreeId>,
    ) -> Result<SubtreeId, Diagnostic> {
        self.add_prepared(rule, width, children, prior, None)
    }
    pub(crate) fn add_prepared(
        &mut self,
        rule: u16,
        width: u32,
        children: impl ExactSizeIterator<Item = Child> + DoubleEndedIterator + Clone,
        prior: Option<SubtreeId>,
        prepared: Option<(Sequence<Child>, super::sequence::Costs)>,
    ) -> Result<SubtreeId, Diagnostic> {
        if children.len() >= LARGE as usize {
            return Err(Diagnostic::new(
                "limit",
                [0, 0],
                "syntax storage capacity exceeded",
            ));
        }
        let prior = prior.and_then(|id| self.sequence(id));
        let mut weight = 32_u32.saturating_add((children.len() as u32).saturating_mul(24));
        for child in children.clone() {
            weight = weight.saturating_add(self.weight(child.node));
            self.retain(child.node);
        }
        let reuse = loop {
            match self.free_nodes.pop() {
                Some(id) if self.find_new(id).is_some() => break Some(id),
                Some(_) => continue,
                None => break None,
            }
        };
        let (index, slot) = if let Some(id) = reuse {
            (self.find_new(id).unwrap(), id.slot())
        } else {
            if self
                .chunks
                .get(self.current)
                .and_then(Option::as_ref)
                .is_none_or(|c| c.data.payload() >= CHUNK_BYTES)
            {
                self.seal();
            }
            let c = self.chunks[self.current].as_ref().unwrap();
            if c.data.id == u32::MAX || c.data.nodes.len() >= u32::MAX as usize {
                return Err(Diagnostic::new(
                    "limit",
                    [0, 0],
                    "syntax storage capacity exceeded",
                ));
            }
            (self.current, c.data.nodes.len())
        };
        let mut sequence_charge = 0;
        let sequence = if children.len() > 32 {
            Some(if let Some((sequence, costs)) = prepared {
                sequence_charge = costs.allocated;
                self.copied_existing_bytes += (costs.copies * std::mem::size_of::<Child>()) as u64;
                sequence
            } else if let Some(previous) = prior {
                let len = children.len();
                let prefix = children
                    .clone()
                    .zip(previous.iter())
                    .take_while(|(a, b)| a == *b)
                    .count();
                let suffix = children
                    .clone()
                    .rev()
                    .zip((0..previous.len()).rev().map(|i| previous.get(i).unwrap()))
                    .take(len.min(previous.len()) - prefix)
                    .take_while(|(a, b)| a == *b)
                    .count();
                let (sequence, costs) = previous.replace_costed(
                    prefix..previous.len() - suffix,
                    children
                        .clone()
                        .skip(prefix)
                        .take(len - prefix - suffix)
                        .collect(),
                );
                sequence_charge = costs.allocated;
                self.copied_existing_bytes += (costs.copies * std::mem::size_of::<Child>()) as u64;
                sequence
            } else {
                let (sequence, costs) = Sequence::from_vec_costed(children.clone().collect());
                sequence_charge = costs.allocated;
                sequence
            })
        } else {
            None
        };
        let c = self.chunks[index].as_mut().unwrap();
        let (first, count) = if let Some(sequence) = sequence {
            let len = children.len() as u32 | LARGE;
            let first = if let Some(first) = c.free_large.pop() {
                c.data.large[first as usize] = sequence;
                c.sequence_charges[first as usize] = sequence_charge;
                first
            } else {
                let first = c.data.large.len() as u32;
                c.data.large.push(sequence);
                c.sequence_charges.push(sequence_charge);
                first
            };
            (first, len)
        } else {
            let count = children.len();
            let first = c.free_edges[count]
                .pop()
                .unwrap_or(c.data.edges.len() as u32);
            for (offset, child) in children.enumerate() {
                let at = first as usize + offset;
                if at < c.data.edges.len() {
                    c.data.edges[at] = child;
                } else {
                    c.data.edges.push(child);
                }
            }
            (first, count as u32)
        };
        let node = StoredNode {
            rule,
            width,
            first,
            count,
        };
        if slot == c.data.nodes.len() {
            c.data.nodes.push(node);
            c.references.push(1);
            c.cache_references.push(0);
            c.weights.push(weight);
        } else {
            c.data.nodes[slot] = node;
            c.references.set(slot, 1);
            c.cache_references[slot] = 0;
            c.weights[slot] = weight;
        }
        c.alive += 1;
        let id = SubtreeId::new(c.data.id, slot as u32);
        self.trail.push(id);
        Ok(id)
    }
    /// Upper bound on newly published allocation payload, excluding reused
    /// sequence blocks. Scratch allocations that die before publication vanish.
    pub(crate) fn publication_allocation_bound(&self) -> usize {
        let mut payload = 0;
        let mut chunks = 0;
        for c in self.chunks.iter().flatten().filter(|c| c.alive != 0) {
            chunks += 1;
            payload += c.data.nodes.len() * std::mem::size_of::<StoredNode>()
                + c.data.edges.len() * std::mem::size_of::<Child>()
                + c.data.large.len() * std::mem::size_of::<Sequence<Child>>()
                + c.sequence_charges.iter().sum::<usize>()
                + std::mem::size_of::<Chunk>()
                + 16;
        }
        // finish() concatenates the new directory as ONE sorted batch. Charge
        // its leaf payload and rebuilt paths once, not once per new chunk.
        if chunks != 0 {
            payload += self
                .published
                .chunks
                .replacement_allocation_bound(chunks, 64);
        }
        payload
    }
    pub(crate) fn copied_bytes(&self) -> u64 {
        self.copied_existing_bytes
    }
    /// Live nodes and node slots in unpublished chunks.
    pub(crate) fn occupancy(&self) -> (usize, usize) {
        self.chunks
            .iter()
            .flatten()
            .fold((0, 0), |(alive, slots), c| {
                (alive + c.alive, slots + c.data.nodes.len())
            })
    }
    pub(crate) fn finish(self) -> PublishedArena {
        let mut published = self.published;
        let mut chunks: Vec<_> = self
            .chunks
            .into_iter()
            .flatten()
            .filter(|c| c.alive != 0)
            .map(|mut c| {
                c.data.nodes.shrink_to_fit();
                c.data.edges.shrink_to_fit();
                c.data.large.shrink_to_fit();
                Arc::new(c.data)
            })
            .collect();
        chunks.sort_by_key(|c| c.id);
        published.chunks = published.chunks.concat(Sequence::from_vec(chunks));
        published
    }
}

/// Positions of sorted chunk IDs, by direct table when they are dense
/// (chunks of one build are mostly consecutive), else by binary search.
struct ChunkIndex {
    first: u32,
    table: Vec<u32>,
}
impl ChunkIndex {
    fn new(ids: &[u32]) -> Self {
        let (Some(&first), Some(&last)) = (ids.first(), ids.last()) else {
            return Self {
                first: 0,
                table: Vec::new(),
            };
        };
        let span = (last - first) as usize + 1;
        if span > ids.len().saturating_mul(4).max(64) {
            return Self {
                first,
                table: Vec::new(),
            };
        }
        let mut table = vec![u32::MAX; span];
        for (index, id) in ids.iter().enumerate() {
            table[(id - first) as usize] = index as u32;
        }
        Self { first, table }
    }
    #[inline]
    fn get(&self, ids: &[u32], id: u32) -> Option<usize> {
        if self.table.is_empty() {
            return ids.binary_search(&id).ok();
        }
        let slot = *self.table.get(id.checked_sub(self.first)? as usize)?;
        (slot != u32::MAX).then_some(slot as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_chunk_lookup_rejects_recycled_and_freed_identities() {
        let mut arena = ArenaBuilder::new(vec![""].into());
        let old = arena.add(1, 1, vec![]).unwrap();
        let old_index = arena.current;
        assert_eq!(arena.current_chunk_index(old), Some(old_index));
        arena.seal();
        arena.release(old);
        arena.start_chunk();
        assert_eq!(arena.current, old_index);
        let new = arena.add(2, 1, vec![]).unwrap();
        assert_ne!(old.chunk(), new.chunk());
        assert_eq!(arena.current_chunk_index(old), None);
        assert_eq!(arena.find_new(old), None);
        assert!(arena.node(old).is_none());
        assert_eq!(arena.current_chunk_index(new), Some(old_index));
        assert_eq!(arena.find_new(new), Some(old_index));
        assert_eq!(arena.node(new).unwrap().rule, 2);
        arena.release(new);
        assert_eq!(arena.current_chunk_index(new), None);
        assert_eq!(arena.find_new(new), None);
        assert!(arena.node(new).is_none());
    }

    #[test]
    fn current_chunk_lookup_keeps_other_live_chunk_resolution() {
        let mut arena = ArenaBuilder::new(vec![""].into());
        let earlier = arena.add(1, 1, vec![]).unwrap();
        let earlier_index = arena.current;
        arena.seal();
        let current = arena.add(2, 1, vec![]).unwrap();
        assert_eq!(arena.current_chunk_index(earlier), None);
        assert_eq!(arena.find_new(earlier), Some(earlier_index));
        assert_eq!(arena.current_chunk_index(current), Some(arena.current));
        assert_eq!(arena.node(earlier).unwrap().rule, 1);
        assert_eq!(arena.node(current).unwrap().rule, 2);
    }

    #[test]
    fn current_chunk_lookup_keeps_published_and_independent_builders_separate() {
        let mut original = ArenaBuilder::new(vec![""].into());
        let published_node = original.add(1, 1, vec![]).unwrap();
        let published = original.finish();
        let mut arena = ArenaBuilder::from_published(&published);
        let new = arena.add(2, 1, vec![]).unwrap();
        let mut independent = ArenaBuilder::new(vec![""].into());
        let unrelated = independent.add(3, 1, vec![]).unwrap();
        assert_eq!(arena.current_chunk_index(published_node), None);
        assert_eq!(arena.find_new(published_node), None);
        assert_eq!(arena.node(published_node).unwrap().rule, 1);
        assert_eq!(arena.current_chunk_index(unrelated), None);
        assert!(arena.node(unrelated).is_none());
        assert_eq!(arena.current_chunk_index(new), Some(arena.current));
        assert_eq!(published.node(published_node).unwrap().rule, 1);
    }

    #[test]
    fn batched_publication_charges_directory_paths_once() {
        let chunks = (0..4096)
            .map(|_| {
                let mut chunk = Chunk::new();
                chunk.nodes.push(StoredNode {
                    rule: 0,
                    width: 1,
                    first: 0,
                    count: 0,
                });
                Arc::new(chunk)
            })
            .collect();
        let base = PublishedArena {
            chunks: Sequence::from_vec(chunks),
            labels: vec![""].into(),
        };
        let mut builder = ArenaBuilder::from_published(&base);
        for _ in 0..100 {
            builder.add(0, 1, vec![]).unwrap();
            builder.seal();
        }
        assert!(
            builder.publication_allocation_bound() < 128 * 1024,
            "batched directory publication charged {} bytes",
            builder.publication_allocation_bound()
        );
    }
    #[test]
    fn construction_reference_counts_promote_without_truncation() {
        let mut counts = ReferenceCounts::default();
        counts.push(1);
        counts.set(0, u64::from(u32::MAX));
        assert_eq!(counts.get(0), Some(u64::from(u32::MAX)));
        counts.set(0, u64::from(u32::MAX) + 1);
        assert_eq!(counts.get(0), Some(u64::from(u32::MAX) + 1));
        counts.push(2);
        assert_eq!(counts.get(1), Some(2));
        counts.set(0, 0);
        counts.shrink_to_fit();
        assert_eq!(counts.get(0), Some(0));
    }
    #[test]
    fn evacuation_propagates_storage_capacity_limits() {
        let mut build = ArenaBuilder::new(vec![""].into());
        let root = build.add(1, 1, vec![]).unwrap();
        let original = build.finish();
        let reader = original.clone();
        let mut exhausted = ArenaBuilder::new(vec![""].into());
        exhausted.chunks[0].as_mut().unwrap().data.id = u32::MAX;
        exhausted.first_id = u32::MAX;
        exhausted.by_id.clear();
        exhausted.by_id.insert(u32::MAX, 0);
        let result = original.evacuate(&[root], exhausted);
        assert!(result.is_err());
        let error = result.err().unwrap();
        assert_eq!(error.category, "limit");
        assert_eq!(reader.node(root).unwrap().rule, 1);
    }
    #[test]
    fn exhausted_chunk_ids_never_wrap_to_live_identities() {
        let ids = AtomicU32::new(u32::MAX - 1);
        assert_eq!(next_chunk_id(&ids), u32::MAX - 1);
        assert_eq!(next_chunk_id(&ids), u32::MAX);
        assert_eq!(next_chunk_id(&ids), u32::MAX);
        assert_eq!(ids.load(Ordering::Relaxed), u32::MAX);
    }
    #[test]
    fn completed_parent_consumes_child_temporaries() {
        let mut arena = ArenaBuilder::new(vec![""].into());
        let leaf = arena.add(1, 1, vec![]).unwrap();
        let mut root = leaf;
        for _ in 0..5000 {
            root = arena
                .add(
                    2,
                    1,
                    vec![Child {
                        node: root,
                        extent: 1,
                        label: 0,
                    }],
                )
                .unwrap();
            arena.commit(0, root);
            assert_eq!(arena.mark(), 1);
        }
        assert!(arena.node(leaf).is_some());
        arena.restore(0);
        assert!(arena.node(leaf).is_none());
    }
    #[test]
    fn rolled_back_unpublished_nodes_reuse_storage() {
        let mut arena = ArenaBuilder::new(vec![""].into());
        let root = arena.add(1, 1, vec![]).unwrap();
        let mark = arena.mark();
        for _ in 0..100_000 {
            let child = arena.add(2, 1, vec![]).unwrap();
            arena
                .add(
                    3,
                    2,
                    vec![
                        Child {
                            node: root,
                            extent: 1,
                            label: 0,
                        },
                        Child {
                            node: child,
                            extent: 1,
                            label: 0,
                        },
                    ],
                )
                .unwrap();
            arena.restore(mark);
        }
        assert!(arena.unpublished_payload_bytes() < 2 * CHUNK_BYTES);
        assert_eq!(arena.node(root).unwrap().rule, 1);
    }
    #[test]
    fn pinned_winner_survives_rollback_until_transferred() {
        let mut arena = ArenaBuilder::new(vec![""].into());
        let mark = arena.mark();
        let leaf = arena.add(1, 1, vec![]).unwrap();
        let winner = arena
            .add(
                2,
                1,
                vec![Child {
                    node: leaf,
                    extent: 1,
                    label: 0,
                }],
            )
            .unwrap();
        arena.retain(winner);
        arena.restore(mark);
        assert!(arena.node(leaf).is_some());
        arena.adopt(winner);
        let final_root = arena
            .add(
                3,
                1,
                vec![Child {
                    node: winner,
                    extent: 1,
                    label: 0,
                }],
            )
            .unwrap();
        arena.retain(final_root);
        arena.restore(0);
        assert!(arena.node(leaf).is_some());
        arena.release(final_root);
        assert!(arena.node(leaf).is_none());
    }
    #[test]
    fn eviction_preserves_published_handles() {
        let mut build = ArenaBuilder::new(vec!["", "Name"].into());
        let leaf = build.add(1, 3, vec![]).unwrap();
        let root = build
            .add(
                2,
                5,
                vec![Child {
                    node: leaf,
                    extent: 4,
                    label: 1,
                }],
            )
            .unwrap();
        let published = build.finish();
        let mut edit = ArenaBuilder::from_published(&published);
        let newer = edit
            .add(
                3,
                6,
                vec![Child {
                    node: root,
                    extent: 5,
                    label: 0,
                }],
            )
            .unwrap();
        let edited = edit.finish();
        drop(published);
        assert_eq!(edited.node(leaf).unwrap().width, 3);
        assert_eq!(edited.children(newer).next().unwrap().node, root);
        assert_eq!(edited.children(root).next().unwrap().label, 1);
    }
    #[test]
    fn wide_parent_reuses_child_sequence_blocks() {
        let mut build = ArenaBuilder::new(vec![""].into());
        let items: Vec<_> = (0..10_000)
            .map(|_| Child {
                node: build.add(1, 1, vec![]).unwrap(),
                extent: 1,
                label: 0,
            })
            .collect();
        let root = build.add(2, 10_000, items.clone()).unwrap();
        let original = build.finish();
        let mut edit = ArenaBuilder::from_published(&original);
        let replacement = edit.add(1, 1, vec![]).unwrap();
        let old_child = items[5000].node;
        let mut changed = items;
        changed[5000].node = replacement;
        let new = edit
            .add_reusing(2, 10_000, changed.iter().copied(), Some(root))
            .unwrap();
        let edited = edit.finish();
        let a = original.sequence(root).unwrap();
        let b = edited.sequence(new).unwrap();
        assert!(a.shared_leaves(&b) > 250);
        assert_eq!(original.children(root).nth(5000).unwrap().node, old_child);
    }
    #[test]
    fn sparse_chunks_release_unowned_dependencies() {
        let mut build = ArenaBuilder::new(vec![""].into());
        for _ in 0..100 {
            build.add(1, 1, vec![]).unwrap();
            build.seal();
        }
        let root = build.add(2, 1, vec![]).unwrap();
        let original = build.finish();
        let old_reader = original.clone();
        let mut roots = vec![root];
        let (compacted, _) = original.compact(&mut roots).unwrap();
        assert_eq!(compacted.chunk_count(), 1);
        assert_eq!(compacted.node(roots[0]).unwrap().rule, 2);
        assert_eq!(old_reader.chunk_count(), 101);
        assert_eq!(old_reader.node(root).unwrap().rule, 2);
    }
    #[test]
    fn wide_parent_publication_charges_only_new_sequence_paths() {
        let mut build = ArenaBuilder::new(vec![""].into());
        let leaf = build.add(1, 1, vec![]).unwrap();
        let children = vec![
            Child {
                node: leaf,
                extent: 1,
                label: 0
            };
            100_000
        ];
        let mut root = build.add(2, 100_000, children.clone()).unwrap();
        let mut arena = build.finish();
        let mut charge = 0;
        for _ in 0..100 {
            let mut build = ArenaBuilder::from_published(&arena);
            root = build
                .add_reusing(2, 100_000, children.iter().copied(), Some(root))
                .unwrap();
            charge += build.publication_allocation_bound();
            arena = build.finish();
        }
        assert!(
            charge < 8 * 1024 * 1024,
            "shared lists charged as whole copies: {charge}"
        );
    }
    #[test]
    fn sparse_shared_chunk_evacuates_without_changing_old_reader() {
        let mut build = ArenaBuilder::new(vec![""].into());
        let leaf = build.add(1, 1, vec![]).unwrap();
        let old_root = build
            .add(
                2,
                1,
                vec![
                    Child {
                        node: leaf,
                        extent: 1,
                        label: 0
                    };
                    100_000
                ],
            )
            .unwrap();
        let original = build.finish();
        let old_reader = original.clone();
        let mut roots = [leaf];
        let (latest, _) = original.compact(&mut roots).unwrap();
        assert!(
            latest.payload_bytes() < CHUNK_BYTES,
            "tiny live subtree pins obsolete wide root"
        );
        assert_eq!(latest.node(roots[0]).unwrap().rule, 1);
        assert_eq!(old_reader.children(old_root).count(), 100_000);
        assert_eq!(old_reader.node(leaf).unwrap().rule, 1);
    }
    #[test]
    fn cold_compaction_preserves_postorder_references() {
        let mut build = ArenaBuilder::new(vec![""].into());
        build.add(9, 1, vec![]).unwrap();
        let leaf = build.add(1, 1, vec![]).unwrap();
        let root = build
            .add(
                2,
                1,
                vec![Child {
                    node: leaf,
                    extent: 1,
                    label: 0,
                }],
            )
            .unwrap();
        let mut roots = vec![root];
        let (compacted, _) = build.finish().compact(&mut roots).unwrap();
        assert_eq!(
            compacted
                .children(roots[0])
                .next()
                .map(|e| compacted.node(e.node).unwrap().rule),
            Some(1)
        );
        assert_eq!(
            compacted.payload_bytes(),
            2 * std::mem::size_of::<StoredNode>() + std::mem::size_of::<Child>()
        );
    }
    #[test]
    fn deep_chunk_drop_is_iterative() {
        let mut build = ArenaBuilder::new(vec![""].into());
        let mut root = build.add(1, 1, vec![]).unwrap();
        for _ in 0..20_000 {
            build.seal();
            root = build
                .add(
                    1,
                    1,
                    vec![Child {
                        node: root,
                        extent: 1,
                        label: 0,
                    }],
                )
                .unwrap();
        }
        let published = build.finish();
        assert_eq!(published.node(root).unwrap().width, 1);
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(move || drop(published))
            .unwrap()
            .join()
            .unwrap();
    }
}
