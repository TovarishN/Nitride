//! Persistent weighted sequences. Only boundary leaves and balanced paths copy.
use std::{ops::Range, sync::Arc};

pub(crate) trait Measured: Clone {
    fn width(&self) -> u64;
    fn key(&self) -> u32 {
        0
    }
}
const LEAF: usize = 32;

enum Tree<T> {
    Leaf {
        items: Vec<T>,
        width: u64,
        high_key: u32,
    },
    Branch {
        left: Arc<Tree<T>>,
        right: Arc<Tree<T>>,
        len: usize,
        width: u64,
        height: u16,
        high_key: u32,
    },
}
impl<T> Tree<T> {
    fn len(&self) -> usize {
        match self {
            Self::Leaf { items, .. } => items.len(),
            Self::Branch { len, .. } => *len,
        }
    }
    fn width(&self) -> u64 {
        match self {
            Self::Leaf { width, .. } | Self::Branch { width, .. } => *width,
        }
    }
    fn high_key(&self) -> u32 {
        match self {
            Self::Leaf { high_key, .. } | Self::Branch { high_key, .. } => *high_key,
        }
    }
    fn height(&self) -> u16 {
        match self {
            Self::Leaf { .. } => 1,
            Self::Branch { height, .. } => *height,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Sequence<T> {
    root: Option<Arc<Tree<T>>>,
}
impl<T> Default for Sequence<T> {
    fn default() -> Self {
        Self { root: None }
    }
}

#[derive(Default, Clone, Copy)]
pub(crate) struct Costs {
    pub(crate) copies: usize,
    pub(crate) allocated: usize,
}
impl Costs {
    pub(crate) fn include(&mut self, other: Self) {
        self.copies += other.copies;
        self.allocated += other.allocated;
    }
    fn node<T>(&mut self) {
        self.allocated += std::mem::size_of::<Tree<T>>() + 2 * std::mem::size_of::<usize>();
    }
}
fn leaf<T: Measured>(items: Vec<T>, costs: &mut Costs) -> Arc<Tree<T>> {
    costs.node::<T>();
    costs.allocated += items.capacity() * std::mem::size_of::<T>();
    let width = items.iter().map(Measured::width).sum();
    let high_key = items.last().map_or(0, Measured::key);
    Arc::new(Tree::Leaf {
        items,
        width,
        high_key,
    })
}
fn branch<T>(left: Arc<Tree<T>>, right: Arc<Tree<T>>, costs: &mut Costs) -> Arc<Tree<T>> {
    costs.node::<T>();
    Arc::new(Tree::Branch {
        len: left.len() + right.len(),
        width: left.width() + right.width(),
        height: 1 + left.height().max(right.height()),
        high_key: right.high_key(),
        left,
        right,
    })
}
fn balance<T>(left: Arc<Tree<T>>, right: Arc<Tree<T>>, costs: &mut Costs) -> Arc<Tree<T>> {
    if left.height() > right.height() + 1 {
        let Tree::Branch {
            left: a, right: b, ..
        } = left.as_ref()
        else {
            unreachable!()
        };
        if a.height() >= b.height() {
            let next = branch(b.clone(), right, costs);
            return branch(a.clone(), next, costs);
        }
        let Tree::Branch {
            left: c, right: d, ..
        } = b.as_ref()
        else {
            unreachable!()
        };
        let l = branch(a.clone(), c.clone(), costs);
        let r = branch(d.clone(), right, costs);
        return branch(l, r, costs);
    }
    if right.height() > left.height() + 1 {
        let Tree::Branch {
            left: a, right: b, ..
        } = right.as_ref()
        else {
            unreachable!()
        };
        if b.height() >= a.height() {
            let next = branch(left, a.clone(), costs);
            return branch(next, b.clone(), costs);
        }
        let Tree::Branch {
            left: c, right: d, ..
        } = a.as_ref()
        else {
            unreachable!()
        };
        let l = branch(left, c.clone(), costs);
        let r = branch(d.clone(), b.clone(), costs);
        return branch(l, r, costs);
    }
    branch(left, right, costs)
}
fn join<T: Measured>(
    left: Option<Arc<Tree<T>>>,
    right: Option<Arc<Tree<T>>>,
    costs: &mut Costs,
) -> Option<Arc<Tree<T>>> {
    let (left, right) = match (left, right) {
        (None, b) => return b,
        (a, None) => return a,
        (Some(a), Some(b)) => (a, b),
    };
    if let (Tree::Leaf { items: a, .. }, Tree::Leaf { items: b, .. }) =
        (left.as_ref(), right.as_ref())
    {
        if a.len() + b.len() <= LEAF {
            costs.copies += a.len() + b.len();
            let mut items = Vec::with_capacity(a.len() + b.len());
            items.extend_from_slice(a);
            items.extend_from_slice(b);
            return Some(leaf(items, costs));
        }
    }
    if left.height() > right.height() + 1 {
        let Tree::Branch {
            left: a, right: b, ..
        } = left.as_ref()
        else {
            unreachable!()
        };
        let next = join(Some(b.clone()), Some(right), costs).unwrap();
        return Some(balance(a.clone(), next, costs));
    }
    if right.height() > left.height() + 1 {
        let Tree::Branch {
            left: a, right: b, ..
        } = right.as_ref()
        else {
            unreachable!()
        };
        let next = join(Some(left), Some(a.clone()), costs).unwrap();
        return Some(balance(next, b.clone(), costs));
    }
    Some(branch(left, right, costs))
}
type Split<T> = (Option<Arc<Tree<T>>>, Option<Arc<Tree<T>>>);
fn split<T: Measured>(tree: Option<Arc<Tree<T>>>, index: usize, costs: &mut Costs) -> Split<T> {
    let Some(tree) = tree else {
        assert_eq!(index, 0);
        return (None, None);
    };
    if index == 0 {
        return (None, Some(tree));
    }
    if index == tree.len() {
        return (Some(tree), None);
    }
    assert!(index < tree.len());
    match tree.as_ref() {
        Tree::Leaf { items, .. } => {
            costs.copies += items.len();
            (
                Some(leaf(items[..index].to_vec(), costs)),
                Some(leaf(items[index..].to_vec(), costs)),
            )
        }
        Tree::Branch { left, right, .. } => {
            if index < left.len() {
                let (a, b) = split(Some(left.clone()), index, costs);
                (a, join(b, Some(right.clone()), costs))
            } else {
                let (a, b) = split(Some(right.clone()), index - left.len(), costs);
                (join(Some(left.clone()), a, costs), b)
            }
        }
    }
}
impl<T: Measured> Sequence<T> {
    pub(crate) fn from_vec(items: Vec<T>) -> Self {
        Self::from_vec_costed(items).0
    }
    pub(crate) fn from_vec_costed(items: Vec<T>) -> (Self, Costs) {
        let mut costs = Costs::default();
        let mut leaves = Vec::new();
        let mut iter = items.into_iter();
        loop {
            let items: Vec<_> = iter.by_ref().take(LEAF).collect();
            if items.is_empty() {
                break;
            }
            leaves.push(leaf(items, &mut costs));
        }
        fn build<T>(leaves: &[Arc<Tree<T>>], costs: &mut Costs) -> Option<Arc<Tree<T>>> {
            match leaves.len() {
                0 => None,
                1 => Some(leaves[0].clone()),
                n => {
                    let m = n / 2;
                    let l = build(&leaves[..m], costs).unwrap();
                    let r = build(&leaves[m..], costs).unwrap();
                    Some(branch(l, r, costs))
                }
            }
        }
        let root = build(&leaves, &mut costs);
        (Self { root }, costs)
    }
    pub(crate) fn len(&self) -> usize {
        self.root.as_ref().map_or(0, |n| n.len())
    }
    pub(crate) fn width(&self) -> u64 {
        self.root.as_ref().map_or(0, |n| n.width())
    }
    #[cfg(test)]
    fn height(&self) -> u16 {
        self.root.as_ref().map_or(0, |n| n.height())
    }
    pub(crate) fn replace(&self, range: Range<usize>, items: Vec<T>) -> Self {
        self.replace_counted(range, items).0
    }
    pub(crate) fn replace_counted(&self, range: Range<usize>, items: Vec<T>) -> (Self, usize) {
        let (result, costs) = self.replace_costed(range, items);
        (result, costs.copies)
    }
    pub(crate) fn replace_costed(&self, range: Range<usize>, items: Vec<T>) -> (Self, Costs) {
        assert!(range.start <= range.end && range.end <= self.len());
        let (middle, mut costs) = Self::from_vec_costed(items);
        let (left, tail) = split(self.root.clone(), range.start, &mut costs);
        let (_, right) = split(tail, range.end - range.start, &mut costs);
        let next = join(left, middle.root, &mut costs);
        let root = join(next, right, &mut costs);
        (Self { root }, costs)
    }
    pub(crate) fn slice_costed(&self, range: Range<usize>) -> (Self, Costs) {
        assert!(range.start <= range.end && range.end <= self.len());
        let mut costs = Costs::default();
        let (_, tail) = split(self.root.clone(), range.start, &mut costs);
        let (root, _) = split(tail, range.end - range.start, &mut costs);
        (Self { root }, costs)
    }
    /// Conservative directory path allocation bound when a caller does not retain costs.
    pub(crate) fn replacement_allocation_bound(&self, inserted: usize, copied: usize) -> usize {
        let height = self.root.as_ref().map_or(0, |node| node.height()) as usize;
        (inserted + copied)
            .saturating_mul(2 * std::mem::size_of::<T>())
            .saturating_add(
                (height + 1)
                    * 32
                    * (std::mem::size_of::<Tree<T>>() + 2 * std::mem::size_of::<usize>()),
            )
    }
    pub(crate) fn concat(&self, other: Self) -> Self {
        self.concat_costed(other).0
    }
    pub(crate) fn concat_costed(&self, other: Self) -> (Self, Costs) {
        let mut costs = Costs::default();
        let root = join(self.root.clone(), other.root, &mut costs);
        (Self { root }, costs)
    }
    /// Logarithmic lookup for sequences whose measured keys are sorted.
    pub(crate) fn find_key(&self, key: u32) -> Option<&T> {
        let mut node = self.root.as_deref()?;
        loop {
            match node {
                Tree::Leaf { items, .. } => {
                    return items
                        .binary_search_by_key(&key, Measured::key)
                        .ok()
                        .map(|i| &items[i]);
                }
                Tree::Branch { left, right, .. } => {
                    node = if key <= left.high_key() { left } else { right }
                }
            }
        }
    }
    pub(crate) fn get(&self, mut index: usize) -> Option<&T> {
        let mut node = self.root.as_deref()?;
        if index >= node.len() {
            return None;
        }
        loop {
            match node {
                Tree::Leaf { items, .. } => return items.get(index),
                Tree::Branch { left, right, .. } => {
                    if index < left.len() {
                        node = left;
                    } else {
                        index -= left.len();
                        node = right;
                    }
                }
            }
        }
    }
    pub(crate) fn prefix_width(&self, mut index: usize) -> u64 {
        assert!(index <= self.len());
        let Some(mut node) = self.root.as_deref() else {
            return 0;
        };
        let mut width = 0;
        loop {
            match node {
                Tree::Leaf { items, .. } => {
                    return width + items.iter().take(index).map(Measured::width).sum::<u64>();
                }
                Tree::Branch { left, right, .. } => {
                    if index <= left.len() {
                        node = left
                    } else {
                        index -= left.len();
                        width += left.width();
                        node = right
                    }
                }
            }
        }
    }
    pub(crate) fn locate(&self, mut offset: u64) -> Option<(usize, &T, u64)> {
        let mut node = self.root.as_deref()?;
        if offset >= node.width() {
            return None;
        }
        let mut index = 0;
        loop {
            match node {
                Tree::Leaf { items, .. } => {
                    for item in items {
                        if offset < item.width() {
                            return Some((index, item, offset));
                        }
                        offset -= item.width();
                        index += 1;
                    }
                    return None;
                }
                Tree::Branch { left, right, .. } => {
                    if offset < left.width() {
                        node = left;
                    } else {
                        offset -= left.width();
                        index += left.len();
                        node = right;
                    }
                }
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn shared_leaves(&self, other: &Self) -> usize {
        fn leaves<T>(root: &Option<Arc<Tree<T>>>) -> std::collections::HashSet<usize> {
            let mut result = std::collections::HashSet::new();
            let mut pending = Vec::new();
            if let Some(root) = root {
                pending.push(root);
            }
            while let Some(node) = pending.pop() {
                match node.as_ref() {
                    Tree::Leaf { .. } => {
                        result.insert(Arc::as_ptr(node) as usize);
                    }
                    Tree::Branch { left, right, .. } => {
                        pending.push(left);
                        pending.push(right);
                    }
                }
            }
            result
        }
        leaves(&self.root)
            .intersection(&leaves(&other.root))
            .count()
    }
    pub(crate) fn visit_allocations(
        &self,
        class: super::memory::Class,
        visit: &mut super::memory::Visitor<'_>,
    ) {
        use super::memory;
        let mut pending = Vec::new();
        if let Some(root) = &self.root {
            pending.push(root);
        }
        while let Some(node) = pending.pop() {
            let (id, size) = memory::arc(node);
            if !visit(class, id, size) {
                continue;
            }
            match node.as_ref() {
                Tree::Leaf { items, .. } => {
                    if let Some((id, size)) = memory::vector(items) {
                        visit(class, id, size);
                    }
                }
                Tree::Branch { left, right, .. } => {
                    pending.push(left);
                    pending.push(right);
                }
            }
        }
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.iter_from(0)
    }
    pub(crate) fn iter_from(&self, mut index: usize) -> impl Iterator<Item = &T> {
        let mut stack = Vec::new();
        let mut current: std::slice::Iter<'_, T> = [].iter();
        if index < self.len() {
            let mut node = self.root.as_deref().unwrap();
            loop {
                match node {
                    Tree::Leaf { items, .. } => {
                        current = items[index..].iter();
                        break;
                    }
                    Tree::Branch { left, right, .. } => {
                        if index < left.len() {
                            stack.push(right.as_ref());
                            node = left;
                        } else {
                            index -= left.len();
                            node = right;
                        }
                    }
                }
            }
        }
        std::iter::from_fn(move || {
            loop {
                if let Some(item) = current.next() {
                    return Some(item);
                }
                match stack.pop()? {
                    Tree::Leaf { items, .. } => current = items.iter(),
                    Tree::Branch { left, right, .. } => {
                        stack.push(right);
                        stack.push(left);
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Item(u32, Arc<AtomicUsize>);
    impl Clone for Item {
        fn clone(&self) -> Self {
            self.1.fetch_add(1, Ordering::Relaxed);
            Self(self.0, self.1.clone())
        }
    }
    impl Measured for Item {
        fn width(&self) -> u64 {
            u64::from(self.0)
        }
    }
    #[test]
    fn sorted_key_lookup_preserves_fences_across_splices() {
        #[derive(Clone)]
        struct Keyed(u32);
        impl Measured for Keyed {
            fn width(&self) -> u64 {
                1
            }
            fn key(&self) -> u32 {
                self.0
            }
        }
        let old = Sequence::from_vec((1..50_000).step_by(2).map(Keyed).collect());
        let at = 12_345;
        let removed = old.get(at).unwrap().0;
        let new = old.replace(at..at + 1, vec![Keyed(removed + 1)]);
        for key in 0..50_001 {
            let expected = key % 2 == 1 && key < 50_000;
            assert_eq!(old.find_key(key).map(|v| v.0), expected.then_some(key));
            let expected = (expected && key != removed) || key == removed + 1;
            assert_eq!(new.find_key(key).map(|v| v.0), expected.then_some(key));
        }
    }
    #[test]
    fn wide_sequence_insert_does_not_copy_all_siblings() {
        for count in [1_000, 100_000] {
            let copies = Arc::new(AtomicUsize::new(0));
            let items: Vec<_> = (0..count).map(|_| Item(1, copies.clone())).collect();
            let original = Sequence::from_vec(items);
            copies.store(0, Ordering::Relaxed);
            let edited = original.replace(0..0, vec![Item(2, copies.clone())]);
            assert_eq!(edited.len(), count + 1);
            assert_eq!(edited.width(), count as u64 + 2);
            assert_eq!(original.width(), count as u64);
            assert_eq!(edited.get(0).unwrap().0, 2);
            assert!(copies.load(Ordering::Relaxed) < 1024);
        }
    }
    #[test]
    fn repeated_split_join_preserves_order_and_width() {
        let copies = Arc::new(AtomicUsize::new(0));
        let mut seq = Sequence::from_vec((0..1000).map(|_| Item(1, copies.clone())).collect());
        for i in 0..1000 {
            seq = seq.replace(i..i + 1, vec![Item(2, copies.clone())]);
        }
        assert_eq!(seq.len(), 1000);
        assert_eq!(seq.width(), 2000);
        assert_eq!(seq.iter().map(|i| i.0).sum::<u32>(), 2000);
        let (index, item, local) = seq.locate(1999).unwrap();
        assert_eq!((index, item.0, local), (999, 2, 1));
        assert!(seq.height() < 24);
        assert_eq!(seq.replace(0..1000, vec![]).len(), 0);
    }
    #[test]
    fn separated_replacements_share_the_middle_and_account_boundary_operations() {
        let clones = Arc::new(AtomicUsize::new(0));
        let old = Sequence::from_vec((0..100_000).map(|_| Item(1, clones.clone())).collect());
        clones.store(0, Ordering::Relaxed);
        let (middle, mut costs) = old.slice_costed(1..99_999);
        let (first, charge) = Sequence::from_vec_costed(vec![Item(2, clones.clone())]);
        costs.include(charge);
        let (prefix, charge) = first.concat_costed(middle);
        costs.include(charge);
        let (last, charge) = Sequence::from_vec_costed(vec![Item(3, clones.clone())]);
        costs.include(charge);
        let (new, charge) = prefix.concat_costed(last);
        costs.include(charge);
        assert_eq!(costs.copies, clones.load(Ordering::Relaxed));
        assert!(costs.copies < 256);
        assert!(costs.allocated < 16_384);
        assert_eq!(new.len(), old.len());
        assert_eq!(new.width(), old.width() + 3);
        assert_eq!(old.get(0).unwrap().0, 1);
    }
    #[test]
    fn copied_payload_counts_actual_clones() {
        let copies = Arc::new(AtomicUsize::new(0));
        let seq = Sequence::from_vec((0..1000).map(|_| Item(1, copies.clone())).collect());
        copies.store(0, Ordering::Relaxed);
        let (edited, reported) = seq.replace_counted(250..251, vec![Item(2, copies.clone())]);
        assert_eq!(edited.width(), 1001);
        assert_eq!(reported, copies.load(Ordering::Relaxed));
        assert!(reported < 1024);
    }
}
