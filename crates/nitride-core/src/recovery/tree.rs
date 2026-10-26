use crate::*;

#[derive(Debug, Clone, Copy)]
pub struct RecoveryOptions {
    pub max_local_cost: u32,
    pub max_states: usize,
    pub max_local_bytes: usize,
}

impl Default for RecoveryOptions {
    fn default() -> Self {
        Self {
            max_local_cost: 4,
            max_states: 4096,
            max_local_bytes: 4096,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RecoveryStats {
    pub work: u64,
    pub search_states: u64,
    pub repair_count: u64,
    pub fallback_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Expected<'g> {
    Literal(&'g str),
    Rule(u16),
    Character,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repair<'g> {
    Missing { expected: Expected<'g>, at: u32 },
    Skipped { span: [u32; 2] },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageKind {
    Retained,
    Trivia,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub span: [u32; 2],
    pub kind: CoverageKind,
}

#[derive(Debug)]
pub struct RecoveryData<'g> {
    pub repairs: Vec<Repair<'g>>,
    pub coverage: Vec<Coverage>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Missing/otherwise invalid syntax nodes, in postorder identity space.
    pub damaged_nodes: Vec<u32>,
}

impl<'g> RecoveryData<'g> {
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.repairs.capacity() * std::mem::size_of::<Repair>()
            + self.coverage.capacity() * std::mem::size_of::<Coverage>()
            + self.damaged_nodes.capacity() * std::mem::size_of::<u32>()
            + self.diagnostics.capacity() * std::mem::size_of::<Diagnostic>()
            + self
                .diagnostics
                .iter()
                .map(|d| d.message.capacity())
                .sum::<usize>()
    }
}

#[derive(Debug)]
pub struct RecoveredParse<'g> {
    pub tree: SyntaxTree<'g>,
    pub diagnostics: Vec<Diagnostic>,
    pub stats: RecoveryStats,
}

pub(super) fn fallback<'g>(
    language: &'g Language<'g>,
    source: &str,
    diagnostic: Diagnostic,
) -> RecoveredParse<'g> {
    let span = [0, source.len() as u32];
    let diagnostics = vec![diagnostic];
    let repairs = if source.is_empty() {
        vec![]
    } else {
        vec![Repair::Skipped { span }]
    };
    let coverage = if source.is_empty() {
        vec![]
    } else {
        vec![Coverage {
            span,
            kind: CoverageKind::Skipped,
        }]
    };
    RecoveredParse {
        tree: SyntaxTree {
            language,
            source: source.into(),
            nodes: vec![Node {
                rule: language.start,
                span,
                first_child: 0,
                child_count: 0,
            }],
            edges: vec![],
            labels: vec![""],
            root: 0,
            recovery: Some(Box::new(RecoveryData {
                repairs,
                coverage,
                diagnostics: diagnostics.clone(),
                damaged_nodes: vec![0],
            })),
        },
        diagnostics,
        stats: RecoveryStats {
            fallback_count: 1,
            ..RecoveryStats::default()
        },
    }
}
