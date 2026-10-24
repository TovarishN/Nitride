#[derive(Debug, Clone, Copy)]
pub struct Node {
    pub rule: u16,
    pub span: [u32; 2],
    pub first_child: u32,
    pub child_count: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub label: &'static str,
    pub node: u32,
}

#[derive(Debug)]
pub struct SyntaxTree {
    pub(crate) language: &'static crate::Language,
    pub source: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub root: u32,
}

impl SyntaxTree {
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
    pub fn field(&self, id: u32, label: &str) -> Option<u32> {
        self.children(id)
            .iter()
            .find(|e| e.label == label)
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
    }
}
