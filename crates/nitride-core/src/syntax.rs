#[derive(Debug, Clone, Copy)]
pub struct Node {
    pub rule: u16,
    pub span: [u32; 2],
    pub first_child: u32,
    pub child_count: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    /// The field label, an index into the tree's labels (see
    /// [`SyntaxTree::label`]); 0 is the empty label.
    pub label: u16,
    pub node: u32,
}

#[derive(Debug)]
pub struct SyntaxTree<'g> {
    pub(crate) language: &'g crate::Language<'g>,
    pub source: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub root: u32,
    /// The distinct labels of this tree's edges; entry 0 is "".
    pub(crate) labels: Vec<&'g str>,
    pub(crate) recovery: Option<Box<crate::RecoveryData<'g>>>,
}

impl<'g> SyntaxTree<'g> {
    pub fn recovery(&self) -> Option<&crate::RecoveryData<'g>> {
        self.recovery.as_deref()
    }
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.get(id as usize)
    }
    pub fn children(&self, id: u32) -> &[Edge] {
        let Some(n) = self.node(id) else {
            return &[];
        };
        let Some(end) = n.first_child.checked_add(n.child_count) else {
            return &[];
        };
        self.edges
            .get(n.first_child as usize..end as usize)
            .unwrap_or(&[])
    }
    /// The field label of an edge of this tree.
    pub fn label(&self, edge: &Edge) -> &'g str {
        self.labels.get(edge.label as usize).copied().unwrap_or("")
    }
    pub fn field(&self, id: u32, label: &str) -> Option<u32> {
        self.children(id)
            .iter()
            .find(|e| self.label(e) == label)
            .map(|e| e.node)
    }
    pub fn text(&self, id: u32) -> Option<&str> {
        let n = self.node(id)?;
        self.source.get(n.span[0] as usize..n.span[1] as usize)
    }
    pub fn retained_bytes(&self) -> usize {
        self.source.capacity()
            + self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.edges.capacity() * std::mem::size_of::<Edge>()
            + self.labels.capacity() * std::mem::size_of::<&str>()
            + self.recovery.as_ref().map_or(0, |r| r.retained_bytes())
    }
}

pub(crate) mod sequence;
pub(crate) mod text;
pub use text::{SourceText, TextEdit};
pub(crate) mod arena;
pub use arena::SubtreeId;
pub(crate) mod shared;
pub use shared::{NodeView, ParseSnapshot, SnapshotMode, SnapshotStats, StorageStats};

pub(crate) mod access;
pub use access::{compile_snapshot, compile_snapshot_recovering};

pub(crate) mod memory;
pub use shared::shared_storage_bytes;

pub(crate) mod sidecars;
