//! A construction buffer keeps unchanged child runs as borrowed persistent ranges.
//! Only new/changed payloads enter its scratch vector; labels and rollback split
//! run descriptors rather than materializing the shared children.
use super::{Edge, Handle};
use crate::{
    SubtreeId,
    syntax::{arena::Child, sequence::Sequence},
};
use std::ops::Range;
#[derive(Clone)]
enum Part {
    Old {
        range: Range<usize>,
        label: Option<u16>,
    },
    New(Range<usize>),
}
impl Part {
    fn range(&self) -> &Range<usize> {
        match self {
            Self::Old { range, .. } | Self::New(range) => range,
        }
    }
    fn range_mut(&mut self) -> &mut Range<usize> {
        match self {
            Self::Old { range, .. } | Self::New(range) => range,
        }
    }
    fn len(&self) -> usize {
        self.range().end - self.range().start
    }
    fn join(&mut self, other: &Self) -> bool {
        match (self, other) {
            (Self::Old { range: a, label: x }, Self::Old { range: b, label: y })
                if a.end == b.start && x == y =>
            {
                a.end = b.end;
                true
            }
            (Self::New(a), Self::New(b)) if a.end == b.start => {
                a.end = b.end;
                true
            }
            _ => false,
        }
    }
}
struct Previous {
    node: SubtreeId,
    start: u32,
    items: Sequence<Child>,
}
#[derive(Default)]
pub(crate) struct Children {
    previous: Option<Previous>,
    parts: Vec<Part>,
    changed: Vec<Child>,
    len: usize,
    start: u32,
    last_end: u32,
    cursor: usize,
    first: Option<Handle>,
    sink: bool,
}
impl Children {
    pub(crate) fn new(start: usize, previous: Option<(SubtreeId, u32, Sequence<Child>)>) -> Self {
        let mut result = Self::default();
        result.reset(start, previous);
        result
    }
    pub(crate) fn sink(start: usize) -> Self {
        Self {
            start: start as u32,
            last_end: start as u32,
            sink: true,
            ..Self::default()
        }
    }
    pub(crate) fn reset(
        &mut self,
        start: usize,
        previous: Option<(SubtreeId, u32, Sequence<Child>)>,
    ) {
        self.clear();
        self.start = start as u32;
        self.last_end = start as u32;
        self.previous = previous.map(|(node, start, items)| Previous { node, start, items });
    }
    pub(crate) fn clear(&mut self) {
        self.previous = None;
        self.parts.clear();
        self.changed.clear();
        self.len = 0;
        self.cursor = 0;
        self.first = None;
        self.sink = false;
    }
    pub(crate) fn allocated(&self) -> bool {
        self.parts.capacity() != 0 || self.changed.capacity() != 0
    }
    pub(crate) fn prior_node(&self) -> Option<SubtreeId> {
        self.previous.as_ref().map(|p| p.node)
    }
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub(crate) fn first_node(&self) -> Option<Handle> {
        self.first
    }
    pub(super) fn push(&mut self, edge: Edge, old_end: Option<u32>) -> usize {
        if self.sink {
            self.len += 1;
            return 0;
        }
        if self.len == 0 {
            self.first = Some(edge.node);
        }
        let end = edge.node.start + edge.node.width;
        let item = Child {
            node: edge.node.node,
            extent: end - self.last_end,
            label: edge.label,
        };
        self.last_end = end;
        self.len += 1;
        let matched = self.previous.as_ref().and_then(|p| {
            let nearby = (self.cursor..p.items.len().min(self.cursor.saturating_add(4)))
                .find(|i| p.items.get(*i).is_some_and(|c| c.node == item.node));
            nearby.or_else(|| {
                let relative = old_end?.checked_sub(p.start)?.checked_sub(1)?;
                let (i, child, within) = p.items.locate(u64::from(relative))?;
                (child.node == item.node && within + 1 == u64::from(child.extent)).then_some(i)
            })
        });
        if let Some(index) = matched {
            self.cursor = index + 1;
            let old = *self.previous.as_ref().unwrap().items.get(index).unwrap();
            if old.extent == item.extent {
                self.append(Part::Old {
                    range: index..index + 1,
                    label: (old.label != item.label).then_some(item.label),
                });
                return 0;
            }
        }
        let at = self.changed.len();
        self.changed.push(item);
        self.append(Part::New(at..at + 1));
        std::mem::size_of::<Child>()
    }
    fn append(&mut self, part: Part) {
        if self.parts.last_mut().is_some_and(|last| last.join(&part)) {
            return;
        }
        self.parts.push(part);
    }
    pub(crate) fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }
        if self.sink {
            self.len = len;
            return;
        }
        let mut remaining = len;
        let mut keep = 0;
        for part in &mut self.parts {
            if remaining >= part.len() {
                remaining -= part.len();
                keep += 1;
            } else {
                if remaining > 0 {
                    let range = part.range_mut();
                    range.end = range.start + remaining;
                    keep += 1;
                }
                break;
            }
        }
        self.parts.truncate(keep);
        self.len = len;
        if len == 0 {
            self.first = None;
        }
        let end = self
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::New(range) => Some(range.end),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        self.changed.truncate(end);
        self.cursor = self
            .parts
            .iter()
            .rev()
            .find_map(|p| match p {
                Part::Old { range, .. } => Some(range.end),
                _ => None,
            })
            .unwrap_or(0);
        let width: u64 = self
            .parts
            .iter()
            .map(|p| match p {
                Part::New(range) => self.changed[range.clone()]
                    .iter()
                    .map(|c| u64::from(c.extent))
                    .sum(),
                Part::Old { range, .. } => {
                    let s = &self.previous.as_ref().unwrap().items;
                    s.prefix_width(range.end) - s.prefix_width(range.start)
                }
            })
            .sum();
        self.last_end = self.start + width as u32;
    }
    pub(crate) fn label(&mut self, from: usize, label: u16) {
        if self.sink || from >= self.len {
            return;
        }
        let mut before = 0;
        let mut first = 0;
        for i in 0..self.parts.len() {
            let count = self.parts[i].len();
            if from < before + count {
                if from > before {
                    let mut suffix = self.parts[i].clone();
                    let middle = self.parts[i].range().start + from - before;
                    self.parts[i].range_mut().end = middle;
                    suffix.range_mut().start = middle;
                    self.parts.insert(i + 1, suffix);
                    first = i + 1;
                } else {
                    first = i;
                }
                break;
            }
            before += count;
        }
        for part in &mut self.parts[first..] {
            match part {
                Part::New(range) => {
                    for item in &mut self.changed[range.clone()] {
                        item.label = label;
                    }
                }
                Part::Old {
                    range,
                    label: override_label,
                } => {
                    let previous = &self.previous.as_ref().unwrap().items;
                    let same = if range.len() <= 32 {
                        (range.clone()).all(|i| previous.get(i).unwrap().label == label)
                    } else {
                        previous
                            .iter_from(range.start)
                            .take(range.len())
                            .all(|c| c.label == label)
                    };
                    *override_label = (!same).then_some(label);
                }
            }
        }
        let mut kept = 0;
        for i in 0..self.parts.len() {
            let part = self.parts[i].clone();
            if kept > 0 && self.parts[kept - 1].join(&part) {
                continue;
            }
            self.parts[kept] = part;
            kept += 1;
        }
        self.parts.truncate(kept);
    }
    fn get(&self, mut index: usize) -> Child {
        for part in &self.parts {
            if index < part.len() {
                return match part {
                    Part::New(range) => self.changed[range.start + index],
                    Part::Old { range, label } => {
                        let mut item = *self
                            .previous
                            .as_ref()
                            .unwrap()
                            .items
                            .get(range.start + index)
                            .unwrap();
                        if let Some(label) = label {
                            item.label = *label;
                        }
                        item
                    }
                };
            }
            index -= part.len();
        }
        unreachable!("child buffer ordinal")
    }
    pub(crate) fn prepare(&self) -> Option<(Sequence<Child>, crate::syntax::sequence::Costs)> {
        self.previous.as_ref()?;
        if self.len <= 32 {
            return None;
        }
        let mut result = Sequence::default();
        let mut costs = crate::syntax::sequence::Costs::default();
        for part in &self.parts {
            let (next, charge) = match part {
                Part::Old { range, label: None } => self
                    .previous
                    .as_ref()
                    .unwrap()
                    .items
                    .slice_costed(range.clone()),
                Part::Old {
                    range,
                    label: Some(label),
                } => {
                    let items = self
                        .previous
                        .as_ref()
                        .unwrap()
                        .items
                        .iter_from(range.start)
                        .take(range.len())
                        .map(|c| Child {
                            label: *label,
                            ..*c
                        })
                        .collect();
                    Sequence::from_vec_costed(items)
                }
                Part::New(range) => Sequence::from_vec_costed(self.changed[range.clone()].to_vec()),
            };
            costs.include(charge);
            let (joined, charge) = result.concat_costed(next);
            costs.include(charge);
            result = joined;
        }
        Some((result, costs))
    }
    pub(crate) fn pop_last(&mut self) -> Option<(Child, u32)> {
        if self.len == 0 {
            return None;
        }
        let child = self.get(self.len - 1);
        let end = self.last_end;
        self.truncate(self.len - 1);
        Some((child, end))
    }
    pub(crate) fn iter(&self) -> Iter<'_> {
        Iter {
            children: self,
            front: 0,
            back: self.len,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Iter<'a> {
    children: &'a Children,
    front: usize,
    back: usize,
}
impl Iterator for Iter<'_> {
    type Item = Child;
    fn next(&mut self) -> Option<Child> {
        if self.front == self.back {
            return None;
        }
        let item = self.children.get(self.front);
        self.front += 1;
        Some(item)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.back - self.front;
        (n, Some(n))
    }
    fn nth(&mut self, n: usize) -> Option<Child> {
        self.front = self.front.saturating_add(n).min(self.back);
        self.next()
    }
}
impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Child> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        Some(self.children.get(self.back))
    }
    fn nth_back(&mut self, n: usize) -> Option<Child> {
        self.back = self.back.saturating_sub(n).max(self.front);
        self.next_back()
    }
}
impl ExactSizeIterator for Iter<'_> {}
