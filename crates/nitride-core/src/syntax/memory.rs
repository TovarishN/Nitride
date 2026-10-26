//! Requested dynamic bytes, deduplicated by allocation identity. Arc accounting
//! follows the current std two-atomic header layout and is cross-checked against
//! the profiler allocator; it is not an estimate of allocator overhead or RSS.
use std::{
    alloc::Layout,
    collections::HashSet,
    sync::{Arc, atomic::AtomicUsize},
};
#[derive(Clone, Copy)]
pub(crate) enum Class {
    Syntax,
    Source,
    Index,
    Sidecar,
}
pub(crate) type Visitor<'a> = dyn FnMut(Class, usize, usize) -> bool + 'a;
pub(crate) fn arc<T: ?Sized>(value: &Arc<T>) -> (usize, usize) {
    let header = Layout::new::<[AtomicUsize; 2]>();
    let (layout, offset) = header
        .extend(Layout::for_value(value.as_ref()))
        .expect("valid Arc layout");
    (
        Arc::as_ptr(value) as *const () as usize - offset,
        layout.pad_to_align().size(),
    )
}
pub(crate) fn vector<T>(value: &Vec<T>) -> Option<(usize, usize)> {
    let size = value.capacity() * std::mem::size_of::<T>();
    (size != 0).then_some((value.as_ptr() as usize, size))
}
#[derive(Default)]
pub(crate) struct Counter {
    seen: HashSet<usize>,
    pub syntax: usize,
    pub source: usize,
    pub index: usize,
    pub sidecar: usize,
}
impl Counter {
    pub fn add(&mut self, class: Class, id: usize, bytes: usize) -> bool {
        if !self.seen.insert(id) {
            return false;
        }
        match class {
            Class::Syntax => self.syntax += bytes,
            Class::Source => self.source += bytes,
            Class::Index => self.index += bytes,
            Class::Sidecar => self.sidecar += bytes,
        };
        true
    }
    pub fn total(&self) -> usize {
        self.syntax + self.source + self.index + self.sidecar
    }
    pub fn allocations(&self) -> usize {
        self.seen.len()
    }
}
