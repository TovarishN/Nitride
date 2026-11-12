//! Shared-storage strict recognizer. The flat recognizer remains the independent
//! compatibility oracle while this backend is admitted.
use crate::incremental::dependencies::{Hit, Index, Key};
use crate::syntax::{arena::ArenaBuilder, text::Reader};
use crate::*;
use std::collections::HashMap;

mod children;
pub(crate) mod dispatch;
mod facts;
pub mod generated;
pub(crate) mod hash;
mod memo;
mod pratt;
mod prediction;
mod probe;
mod recognize;
mod snapshot;
#[cfg(test)]
mod tests;
mod tokens;

pub(crate) use children::Children;
pub use generated::{CompiledParser, SyntaxContext, SyntaxMark};
use hash::FastMap;
use memo::MemoKey;
use snapshot::publish;
pub use snapshot::{
    parse_borrowed_snapshot, parse_snapshot, parse_snapshot_compiled, parse_snapshot_generated,
};
#[cfg(test)]
use snapshot::{parse_source, remap_facts};
use tokens::{Peek, TokenCache};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Handle {
    pub(crate) node: SubtreeId,
    pub(crate) start: u32,
    pub(crate) width: u32,
}
#[derive(Clone, Copy)]
struct PriorNode {
    node: SubtreeId,
    start: u32,
    old_start: u32,
}
type PriorPath = FastMap<u16, (PriorNode, bool)>;
const NO_NODE: Handle = Handle {
    node: SubtreeId::NONE,
    start: 0,
    width: 0,
};
#[derive(Clone, Copy)]
struct Edge {
    label: u16,
    node: Handle,
}
#[derive(Clone, Copy)]
struct Mark {
    position: usize,
    trail: usize,
}
struct Winner {
    end: usize,
    precedence: u32,
    node: Handle,
    tied: bool,
}
#[derive(Clone, Copy)]
enum Memo {
    Fail {
        furthest: usize,
        observed: u64,
        depth: u32,
    },
    Hit {
        end: usize,
        furthest: usize,
        root: Handle,
        observed: u64,
        depth: u32,
    },
    /// A lexical match recorded by a prediction, which needs only its end.
    /// Its leaf is built, and the entry becomes a `Hit`, on first real use.
    Scanned {
        end: usize,
        furthest: usize,
        observed: u64,
        depth: u32,
    },
}
impl Memo {
    /// Observation end and relative depth a hit replays.
    fn replay(self) -> (u64, u32) {
        match self {
            Memo::Fail {
                observed, depth, ..
            }
            | Memo::Hit {
                observed, depth, ..
            }
            | Memo::Scanned {
                observed, depth, ..
            } => (observed, depth),
        }
    }
}
/// Input-independent analysis retained by the language's snapshots.
/// Only genuinely static tables also enter the process-wide cache.
#[derive(Clone)]
pub(crate) struct LanguageTables<'g> {
    pub(crate) dispatch: std::sync::Arc<dispatch::Dispatch<'g>>,
    minimum_sensitive: std::sync::Arc<[bool]>,
    recovery:
        std::sync::Arc<std::sync::OnceLock<std::sync::Arc<crate::recovery::search::Tables<'g>>>>,
}
impl<'g> LanguageTables<'g> {
    fn new(language: &'g Language<'g>) -> Self {
        Self {
            dispatch: std::sync::Arc::new(dispatch::Dispatch::new(language)),
            minimum_sensitive: language
                .rules
                .iter()
                .map(|r| minimum_sensitive(language, r))
                .collect(),
            recovery: Default::default(),
        }
    }
    pub(crate) fn recovery(
        &self,
        language: &'g Language<'g>,
    ) -> std::sync::Arc<crate::recovery::search::Tables<'g>> {
        self.recovery
            .get_or_init(|| std::sync::Arc::new(crate::recovery::search::Tables::new(language)))
            .clone()
    }
}
impl LanguageTables<'static> {
    fn of(language: &'static Language<'static>) -> std::sync::Arc<Self> {
        type Cache = std::sync::Mutex<Vec<(usize, std::sync::Arc<LanguageTables<'static>>)>>;
        static CACHE: Cache = std::sync::Mutex::new(Vec::new());
        let key = std::ptr::from_ref(language) as usize;
        if let Some((_, tables)) = CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(k, _)| *k == key)
        {
            return tables.clone();
        }
        let tables = std::sync::Arc::new(Self::new(language));
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, existing)) = cache.iter().find(|(k, _)| *k == key) {
            return existing.clone();
        }
        cache.push((key, tables.clone()));
        tables
    }
}
fn minimum_sensitive(language: &Language, rule: &Rule) -> bool {
    if matches!(&rule.expr, Expr::Pratt { .. }) {
        return true;
    }
    if rule.owner.is_none() {
        return false;
    }
    fn direct_operand(language: &Language, expr: &Expr, owner: Option<u16>) -> bool {
        match expr {
            Expr::Label(_, inner) => direct_operand(language, inner, owner),
            Expr::Ref(id) => language.operand_of(*id, owner),
            _ => false,
        }
    }
    let tail = match &rule.expr {
        Expr::Sequence(parts) => parts.last(),
        expr => Some(expr),
    };
    tail.is_some_and(|expr| direct_operand(language, expr, rule.owner))
}
/// How many levels of nested repeated elements record recognition facts.
/// Facts let edits reuse unchanged elements, seed recovery and memoize
/// backtracking. Below this many enclosing recorded elements they are not
/// kept: an edit there reparses its nearest recorded ancestor instead. The
/// outermost levels must stay recorded, or an edit reparses the whole file.
pub(crate) const FACT_LEVELS: u32 = 4;
struct Parser<'a, 'g> {
    optimize_recognition: bool,
    compiled: Option<&'static CompiledParser>,
    lexers: Option<&'static crate::lexical::CompiledLexers>,
    choice_cache: FastMap<usize, Option<std::sync::Arc<dispatch::ChoicePlan>>>,
    choice_cache_bytes: usize,
    tokens: TokenCache,
    language: &'g Language<'g>,
    tables: std::sync::Arc<LanguageTables<'g>>,
    minimum_sensitive: std::sync::Arc<[bool]>,
    dispatch: std::sync::Arc<dispatch::Dispatch<'g>>,
    labels: Vec<&'g str>,
    label_ids: FastMap<(usize, usize), u16>,
    source: Reader<'a>,
    options: ParseOptions,
    limits: Limits,
    position: usize,
    furthest: usize,
    work: u64,
    cancellation: crate::cancellation::Poll,
    work_budget: u64,
    arena: ArenaBuilder<'g>,
    /// Boxed so that taking, returning and recycling a buffer moves a pointer
    /// rather than the whole buffer, once per rule attempt.
    #[allow(clippy::vec_box)]
    edge_buffers: Vec<Box<Children>>,
    in_token: u32,
    memo: [FastMap<MemoKey, Memo>; 2],
    memo_capacity: usize,
    cache_budget: u64,
    memo_cost: [usize; 2],
    pending_order: std::collections::VecDeque<(u32, Key)>,
    stack_base: usize,
    stack_budget: usize,
    stats: SnapshotStats,
    index: Index,
    reusable: Index,
    max_depth: u32,
    checkpoint_next: bool,
    /// Recorded checkpoints enclosing the current recognition.
    fact_level: u32,
    failure_checkpoint: Option<(u32, Key)>,
    pending_failures: usize,
    facts: facts::FactLog,
    prior: Index,
    prior_nodes: Vec<PriorNode>,
    prior_paths: FastMap<(SubtreeId, u32), PriorPath>,
    prior_paths_bytes: usize,
    reused_ranges: Vec<[u32; 2]>,
}
#[derive(Clone)]
struct Input<'a, 'g> {
    tables: Option<std::sync::Arc<LanguageTables<'g>>>,
    compiled: Option<&'static CompiledParser>,
    lexers: Option<&'static crate::lexical::CompiledLexers>,
    text: &'a SourceText,
    contiguous: Option<&'a str>,
}
/// Recognition facts from a failed cold strict attempt over the same text.
/// Recovery probes reuse them as an edit reuses a previous snapshot's facts;
/// the arena owns only syntax reachable from those facts. A generated attempt
/// also lends its callbacks and contiguous source to recovery probes.
pub(crate) struct Seed<'a, 'g> {
    pub(crate) tables: std::sync::Arc<LanguageTables<'g>>,
    arena: crate::syntax::arena::PublishedArena<'g>,
    recognitions: Index,
    dispatch: std::sync::Arc<dispatch::Dispatch<'g>>,
    generated: Option<Generated<'a>>,
}
impl<'a, 'g> Seed<'a, 'g> {
    /// The seed's source as one string, when it was parsed from one.
    pub(crate) fn contiguous(&self) -> Option<&'a str> {
        self.generated.map(|g| g.source)
    }
}
#[derive(Clone, Copy)]
struct Generated<'a> {
    compiled: Option<&'static CompiledParser>,
    lexers: &'static crate::lexical::CompiledLexers,
    source: &'a str,
}
impl<'a, 'g> Parser<'a, 'g> {
    fn new(
        language: &'g Language<'g>,
        input: Input<'a, 'g>,
        limits: Limits,
        options: ParseOptions,
        stack_budget: usize,
        base: Option<&BorrowedParseSnapshot<'g>>,
        index: Index,
    ) -> Self {
        let text = input.text;
        let tables = input
            .tables
            .clone()
            .or_else(|| base.and_then(|b| b.tables.clone()))
            .unwrap_or_else(|| std::sync::Arc::new(LanguageTables::new(language)));
        let labels = base.map_or_else(|| vec![""], |b| b.arena.labels.to_vec());
        let label_ids = label_ids(&labels);
        let root_hint = base.map(|b| PriorNode {
            node: b.root,
            start: index.current_position(b.start).unwrap_or(b.start),
            old_start: b.start,
        });
        Self {
            optimize_recognition: true,
            compiled: input.compiled,
            lexers: input.lexers,
            choice_cache: Default::default(),
            choice_cache_bytes: 0,
            tokens: TokenCache::for_text(text.len() as usize),
            language,
            minimum_sensitive: tables.minimum_sensitive.clone(),
            tables: tables.clone(),
            dispatch: base
                .and_then(|b| b.dispatch.clone())
                .unwrap_or_else(|| tables.dispatch.clone()),
            labels,
            label_ids,
            source: input
                .contiguous
                .map_or_else(|| Reader::new(text), |s| Reader::contiguous(text, s)),
            options,
            limits,
            position: 0,
            furthest: 0,
            work: 0,
            cancellation: crate::cancellation::Poll::new(),
            work_budget: limits
                .work
                .max(u64::from(text.len()).saturating_mul(limits.parser_work_per_byte)),
            arena: base.map_or_else(
                || ArenaBuilder::new(vec![""].into()),
                |b| ArenaBuilder::from_published(&b.arena),
            ),
            edge_buffers: Vec::new(),
            in_token: 0,
            memo: Default::default(),
            memo_capacity: (text.len() as usize / 256).clamp(4096, 1 << 14),
            cache_budget: 8 * 1024 * 1024,
            memo_cost: [0, 0],
            pending_order: std::collections::VecDeque::new(),
            stack_base: super::stack_position(),
            stack_budget,
            stats: SnapshotStats::default(),
            reusable: if base.is_some() {
                index.clone()
            } else {
                Index::default()
            },
            index,
            max_depth: 0,
            checkpoint_next: true,
            fact_level: 0,
            failure_checkpoint: None,
            pending_failures: 0,
            // An edit records facts only in the reparsed window, so its
            // log grows by doubling instead of predicting from the position.
            facts: if base.is_none() {
                facts::FactLog::for_text(text.len())
            } else {
                Default::default()
            },
            prior: base.map_or_else(Index::default, |b| b.recognitions.clone()),
            prior_nodes: root_hint.into_iter().collect(),
            prior_paths: FastMap::default(),
            prior_paths_bytes: 0,
            reused_ranges: Vec::new(),
        }
    }
}
fn label_ids(labels: &[&str]) -> FastMap<(usize, usize), u16> {
    labels
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, s)| ((s.as_ptr() as usize, s.len()), i as u16))
        .collect()
}

/// Recovery's opaque probe handles refer directly to immutable shared records.
/// Only selected handles acquire committed ownership at VM stable boundaries.
type RecoveryKeys = std::collections::HashSet<(u16, [u32; 2]), hash::Seeded>;
pub(crate) struct SharedProbe<'a, 'g> {
    parser: Parser<'a, 'g>,
    handles: Vec<Handle>,
    active: Vec<bool>,
    borrowed: Option<u32>,
    unselected: std::collections::VecDeque<u32>,
    free_handles: Vec<u32>,
    committed: usize,
    cache: FastMap<(u16, usize, u32), Option<super::ProbeHit>>,
    /// Scanned lexical ends, for `token_end`; cleared with `cache`.
    ends: FastMap<(u16, usize), Option<usize>>,
    prior_recovery: std::sync::Arc<[(u16, u32, SubtreeId)]>,
    recovery_keys: RecoveryKeys,
    generated: Option<Generated<'a>>,
}
