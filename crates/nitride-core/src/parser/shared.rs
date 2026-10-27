//! Shared-storage strict recognizer. The flat recognizer remains the independent
//! compatibility oracle while this backend is admitted.
use crate::incremental::dependencies::{Hit, Index, Key};
use crate::syntax::{arena::ArenaBuilder, text::Reader};
use crate::*;
use std::collections::HashMap;
mod children;
pub(crate) mod hash;
mod memo;
use hash::FastMap;
use memo::MemoKey;
pub mod generated;
pub use generated::{CompiledParser, SyntaxContext, SyntaxMark};
pub(crate) mod dispatch;
pub(crate) use children::Children;

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
fn minimum_sensitive(rule: &Rule) -> bool {
    if matches!(&rule.expr, Expr::Pratt { .. }) {
        return true;
    }
    let Some(owner) = rule.owner else {
        return false;
    };
    fn direct_operand(expr: &Expr, owner: u16) -> bool {
        match expr {
            Expr::Label(_, inner) => direct_operand(inner, owner),
            Expr::Ref(id) => *id == owner,
            _ => false,
        }
    }
    let tail = match &rule.expr {
        Expr::Sequence(parts) => parts.last(),
        expr => Some(expr),
    };
    tail.is_some_and(|expr| direct_operand(expr, owner))
}
struct Parser<'a> {
    optimize_recognition: bool,
    compiled: Option<&'static CompiledParser>,
    lexers: Option<&'static crate::lexical::CompiledLexers>,
    choice_cache: FastMap<usize, Option<std::sync::Arc<dispatch::ChoicePlan>>>,
    choice_cache_bytes: usize,
    tokens: TokenCache,
    language: &'static Language<'static>,
    minimum_sensitive: Vec<bool>,
    dispatch: std::sync::Arc<dispatch::Dispatch>,
    labels: Vec<&'static str>,
    label_ids: FastMap<(usize, usize), u16>,
    source: Reader<'a>,
    options: ParseOptions,
    limits: Limits,
    position: usize,
    furthest: usize,
    work: u64,
    work_budget: u64,
    arena: ArenaBuilder,
    edge_buffers: Vec<Children>,
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
    failure_checkpoint: Option<(u32, Key)>,
    pending_failures: usize,
    pending_index: FastMap<(u32, Key), Hit>,
    prior: Index,
    prior_nodes: Vec<PriorNode>,
    prior_paths: FastMap<(SubtreeId, u32), PriorPath>,
    prior_paths_bytes: usize,
    reused_ranges: Vec<[u32; 2]>,
}
#[derive(Clone, Copy)]
struct Input<'a> {
    compiled: Option<&'static CompiledParser>,
    lexers: Option<&'static crate::lexical::CompiledLexers>,
    text: &'a SourceText,
    contiguous: Option<&'a str>,
}
/// Recognition facts from a failed cold strict attempt over the same text.
/// Recovery probes reuse them as an edit reuses a previous snapshot's facts;
/// the arena owns only syntax reachable from those facts. A generated attempt
/// also lends its callbacks and contiguous source to recovery probes.
pub(crate) struct Seed<'a> {
    arena: crate::syntax::arena::PublishedArena,
    recognitions: Index,
    dispatch: std::sync::Arc<dispatch::Dispatch>,
    generated: Option<Generated<'a>>,
}
#[derive(Clone, Copy)]
struct Generated<'a> {
    compiled: Option<&'static CompiledParser>,
    lexers: &'static crate::lexical::CompiledLexers,
    source: &'a str,
}
pub fn parse_snapshot(
    language: &'static Language<'static>,
    source: &str,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic> {
    let text = SourceText::new(source)?;
    match mode {
        SnapshotMode::Strict(options) => parse_with_spill(
            language,
            Input {
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: Some(source),
            },
            limits,
            options,
            None,
            Index::default(),
            None,
        ),
        SnapshotMode::Recovering(_) => recover_cold(
            language,
            Input {
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: Some(source),
            },
            limits,
            mode,
        ),
    }
}
/// Cold recovering parse: a strict attempt first, then recovery seeded with
/// that attempt's recognition facts instead of recognizing the prefix again.
fn recover_cold(
    language: &'static Language<'static>,
    input: Input<'_>,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic> {
    let SnapshotMode::Recovering(options) = mode else {
        unreachable!("cold recovery requires recovering mode")
    };
    let mut seed = None;
    let error = match parse_with_spill(
        language,
        input,
        limits,
        ParseOptions {
            strict_comments: true,
            ..Default::default()
        },
        None,
        Index::default(),
        Some(&mut seed),
    ) {
        Ok(mut snapshot) => {
            snapshot.mode = mode;
            return Ok(snapshot);
        }
        Err(error) => error,
    };
    crate::recovery::incremental::recover_snapshot(
        language,
        input.text,
        limits,
        options,
        None,
        Some(error),
        seed.as_ref(),
    )
}
/// Use generated lexical scanners for cold contiguous input. Scanner errors
/// rerun the ordinary parser, retaining its established diagnostics/recovery.
pub fn parse_snapshot_compiled(
    lexers: &'static crate::lexical::CompiledLexers,
    source: &str,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic> {
    parse_snapshot_specialized(lexers, None, source, limits, mode)
}
/// Use generated structural syntax bodies and lexical scanners for cold input.
/// Tree construction, ranking and recovery semantics remain in the runtime.
pub fn parse_snapshot_generated(
    compiled: &'static CompiledParser,
    source: &str,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic> {
    parse_snapshot_specialized(compiled.lexers, Some(compiled), source, limits, mode)
}
fn parse_snapshot_specialized(
    lexers: &'static crate::lexical::CompiledLexers,
    compiled: Option<&'static CompiledParser>,
    source: &str,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic> {
    let language = lexers.language;
    if lexers.scanners.len() != language.rules.len()
        || compiled.is_some_and(|p| {
            p.bodies.len() != language.rules.len() || p.tails.len() != language.rules.len()
        })
    {
        return parse_snapshot(language, source, limits, mode);
    }
    let text = SourceText::new(source)?;
    let options = match mode {
        SnapshotMode::Strict(options) => options,
        SnapshotMode::Recovering(_) => ParseOptions {
            strict_comments: true,
            ..Default::default()
        },
    };
    let mut seed = None;
    let result = parse_with_spill(
        language,
        Input {
            lexers: Some(lexers),
            compiled,
            text: &text,
            contiguous: Some(source),
        },
        limits,
        options,
        None,
        Index::default(),
        matches!(mode, SnapshotMode::Recovering(_)).then_some(&mut seed),
    );
    match (result, mode) {
        (Ok(mut snapshot), _) => {
            snapshot.mode = mode;
            Ok(snapshot)
        }
        // A syntax failure is the ordinary recovery trigger. Seed recovery with
        // the generated attempt's facts instead of a second interpreted attempt.
        (Err(error), SnapshotMode::Recovering(options)) if error.category == "syntax" => {
            crate::recovery::incremental::recover_snapshot(
                language,
                &text,
                limits,
                options,
                None,
                Some(error),
                seed.as_ref(),
            )
        }
        (Err(_), _) => {
            drop(text);
            parse_snapshot(language, source, limits, mode)
        }
    }
}
impl ParseSnapshot {
    pub fn edit(&self, edit: &TextEdit, limits: Limits) -> Result<Self, Diagnostic> {
        let text = self.source.replace(edit)?;
        let delta = edit.replacement.len() as i64 - i64::from(edit.range[1] - edit.range[0]);
        let (index, _) = self.recognitions.edit(edit.range, delta);
        match self.mode {
            SnapshotMode::Strict(options) => parse_with_spill(
                self.language,
                Input {
                    compiled: None,
                    lexers: None,
                    text: &text,
                    contiguous: None,
                },
                limits,
                options,
                Some(self),
                index,
                None,
            )
            .map(|mut snapshot| {
                snapshot.stats.source_bytes_copied = edit.replacement.len() as u64;
                snapshot
            }),
            SnapshotMode::Recovering(options) => {
                let failure = {
                    match parse_with_spill(
                        self.language,
                        Input {
                            compiled: None,
                            lexers: None,
                            text: &text,
                            contiguous: None,
                        },
                        limits,
                        ParseOptions {
                            strict_comments: true,
                            ..Default::default()
                        },
                        Some(self),
                        index,
                        None,
                    ) {
                        Ok(mut snapshot) => {
                            snapshot.mode = self.mode;
                            snapshot.stats.source_bytes_copied = edit.replacement.len() as u64;
                            return Ok(snapshot);
                        }
                        Err(error) => Some(error),
                    }
                };
                crate::recovery::incremental::recover_snapshot(
                    self.language,
                    &text,
                    limits,
                    options,
                    Some((self, edit)),
                    failure,
                    None,
                )
            }
        }
    }
}
fn parse_with_spill<'a>(
    language: &'static Language<'static>,
    text: Input<'a>,
    limits: Limits,
    options: ParseOptions,
    base: Option<&ParseSnapshot>,
    index: Index,
    mut seed: Option<&mut Option<Seed<'a>>>,
) -> Result<ParseSnapshot, Diagnostic> {
    let result = parse_source(
        language,
        text,
        limits,
        options,
        limits.parser_stack_bytes,
        base,
        index.clone(),
        seed.as_deref_mut(),
    );
    if result
        .as_ref()
        .is_err_and(|e| e.message == super::STACK_EXHAUSTED)
        && limits.parser_spill_stack_bytes > limits.parser_stack_bytes
    {
        std::thread::scope(|scope| {
            let budget = limits.parser_spill_stack_bytes - limits.parser_spill_stack_bytes / 8;
            std::thread::Builder::new()
                .name("nitride-shared-parse".into())
                .stack_size(limits.parser_spill_stack_bytes)
                .spawn_scoped(scope, move || {
                    parse_source(language, text, limits, options, budget, base, index, seed)
                })
                .map_err(|_| Diagnostic::new("limit", [0, 0], "cannot start a deep-parse thread"))?
                .join()
                .unwrap_or_else(|_| Err(Diagnostic::new("limit", [0, 0], "deep parse failed")))
        })
    } else {
        result
    }
}
#[allow(clippy::too_many_arguments)]
fn parse_source<'a>(
    language: &'static Language<'static>,
    text: Input<'a>,
    limits: Limits,
    options: ParseOptions,
    stack_budget: usize,
    base: Option<&ParseSnapshot>,
    index: Index,
    seed: Option<&mut Option<Seed<'a>>>,
) -> Result<ParseSnapshot, Diagnostic> {
    let input = text;
    let text = input.text;
    let mut parser = Parser::new(language, input, limits, options, stack_budget, base, index);
    let root = parser.rule(language.start, 0, 0)?;
    parser.trivia()?;
    if root.is_none() || parser.position != text.len() as usize {
        let at = parser.furthest.max(parser.position) as u32;
        if let Some(slot) = seed {
            // Seeding is an optional accelerator; recovery runs either way.
            *slot = parser.into_seed().ok();
        }
        return Err(Diagnostic::new("syntax", [at, at], "invalid source"));
    }
    publish(
        parser,
        text,
        root.unwrap(),
        base,
        SnapshotMode::Strict(options),
    )
}
fn publish(
    mut parser: Parser<'_>,
    text: &SourceText,
    root: Handle,
    base: Option<&ParseSnapshot>,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic> {
    parser.arena.retain(root.node);
    parser.clear_memo(0);
    parser.clear_memo(1);
    for hit in parser.pending_index.values() {
        parser.arena.release_cache(hit.root);
    }
    parser.arena.restore(0);
    parser
        .pending_index
        .retain(|_, hit| hit.root == SubtreeId::NONE || parser.arena.node(hit.root).is_some());
    drop(parser.memo);
    drop(parser.edge_buffers);
    let copied_bytes = parser.arena.copied_bytes();
    let allocation_charge = parser.arena.publication_allocation_bound();
    // Releases free most failed attempts during the parse, and every live node
    // is now reachable from the root. A cold strict graph with few dead slots
    // is published in place: compaction would copy it all to drop little.
    // Recovery publishes over the failed attempt's shared seed chunks, where
    // compaction rebuilds a graph laid out for the edits that follow.
    let (alive, slots) = parser.arena.occupancy();
    let sparse = slots - alive > slots / 8;
    let maintenance_debt = base
        .map_or(0, |b| b.maintenance_debt)
        .saturating_add(allocation_charge);
    let mut arena = parser.arena.finish();
    arena.labels = parser.labels.into();
    // A cold seed's facts are already in the index; compaction below keeps
    // those still reachable from the published root.
    if base.is_none() && parser.index.is_empty() {
        parser.index = Index::from_entries(
            parser
                .pending_index
                .into_iter()
                .map(|((at, key), hit)| (at, key, hit)),
        );
    } else if base.is_none() {
        parser.index = parser.index.merged(
            parser
                .pending_index
                .into_iter()
                .map(|((at, key), hit)| (at, key, hit)),
        );
    } else {
        for ((at, key), hit) in parser.pending_index {
            parser.index.insert(at, key, hit);
        }
    }
    let mut relocated = vec![root.node];

    // New live ancestors count toward the allocation debt too. Use the
    // approved half-fresh-output slack before evacuating an entire graph.
    let allowance =
        (8_usize * 1024 * 1024).saturating_add(base.map_or(0, |b| b.maintenance_base / 2));
    let compact = match base {
        None => sparse || matches!(mode, SnapshotMode::Recovering(_)),
        Some(b) => {
            maintenance_debt > allowance
                || u64::from(text.len()) * 3 < u64::from(b.source.len()) * 2
        }
    };
    let root = if compact {
        let (next, mapping) = arena.compact(&mut relocated)?;
        arena = next;
        parser.stats.reclaimed_syntax_bytes = mapping.removed_bytes as u64;
        parser.stats.syntax_bytes_copied += mapping.copied_bytes as u64;
        parser.stats.maintenance_passes = u64::from(base.is_some());
        parser.index = parser.index.retain_mapped(&|id| {
            if id == SubtreeId::NONE {
                Some(id)
            } else {
                mapping.get(id)
            }
        });
        Handle {
            node: relocated[0],
            ..root
        }
    } else {
        root
    };
    let mut snapshot = ParseSnapshot::from_parts(
        parser.language,
        text.clone(),
        arena,
        root.node,
        root.start,
        mode,
    );
    // A cold graph published in place starts debt-free, as a compacted one.
    if !compact && base.is_some() {
        snapshot.maintenance_debt = maintenance_debt;
        snapshot.maintenance_base = base.map_or(snapshot.maintenance_base, |b| b.maintenance_base);
    }
    snapshot.stats = parser.stats;
    snapshot.stats.work = parser.work;
    snapshot.recognitions = parser.index;
    snapshot.dispatch = Some(parser.dispatch);
    parser.reused_ranges.sort_unstable();
    let mut covered = 0_u64;
    let mut end = 0_u32;
    for range in parser.reused_ranges {
        if range[1] > end {
            covered += u64::from(range[1] - end.max(range[0]));
            end = range[1];
        }
    }
    snapshot.stats.reparsed_bytes = u64::from(text.len()).saturating_sub(covered);
    snapshot.stats.syntax_bytes_copied += copied_bytes;
    snapshot.stats.source_bytes_copied = if base.is_none() {
        u64::from(text.len())
    } else {
        0
    };
    Ok(snapshot)
}
impl<'a> Parser<'a> {
    fn new(
        language: &'static Language<'static>,
        input: Input<'a>,
        limits: Limits,
        options: ParseOptions,
        stack_budget: usize,
        base: Option<&ParseSnapshot>,
        index: Index,
    ) -> Self {
        let text = input.text;
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
            tokens: TokenCache::default(),
            language,
            minimum_sensitive: language.rules.iter().map(minimum_sensitive).collect(),
            dispatch: base
                .and_then(|b| b.dispatch.clone())
                .unwrap_or_else(|| std::sync::Arc::new(dispatch::Dispatch::new(language))),
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
            failure_checkpoint: None,
            pending_failures: 0,
            pending_index: FastMap::default(),
            prior: base.map_or_else(Index::default, |b| b.recognitions.clone()),
            prior_nodes: root_hint.into_iter().collect(),
            prior_paths: FastMap::default(),
            prior_paths_bytes: 0,
            reused_ranges: Vec::new(),
        }
    }
}
fn label_ids(labels: &[&'static str]) -> FastMap<(usize, usize), u16> {
    labels
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, s)| ((s.as_ptr() as usize, s.len()), i as u16))
        .collect()
}
impl<'a> Parser<'a> {
    /// Convert a failed cold strict attempt into recovery seed facts. Fact
    /// roots become owners; all other syntax, including the partial root, is
    /// reclaimed before compaction.
    fn into_seed(mut self) -> Result<Seed<'a>, Diagnostic> {
        self.clear_memo(0);
        self.clear_memo(1);
        let generated = self
            .lexers
            .zip(self.source.contiguous_text())
            .map(|(lexers, source)| Generated {
                compiled: self.compiled,
                lexers,
                source,
            });
        let facts: Vec<_> = self.pending_index.drain().collect();
        for (_, hit) in &facts {
            if hit.root != SubtreeId::NONE {
                self.arena.retain(hit.root);
            }
            self.arena.release_cache(hit.root);
        }
        self.arena.restore(0);
        let mut arena = self.arena.finish();
        arena.labels = self.labels.into();
        let mut roots: Vec<_> = facts
            .iter()
            .map(|(_, hit)| hit.root)
            .filter(|&root| root != SubtreeId::NONE)
            .collect();
        let (arena, mapping) = arena.compact(&mut roots)?;
        let recognitions =
            Index::from_entries(facts.into_iter().filter_map(|((at, key), mut hit)| {
                if hit.root != SubtreeId::NONE {
                    hit.root = mapping.get(hit.root)?;
                }
                Some((at, key, hit))
            }));
        Ok(Seed {
            arena,
            recognitions,
            dispatch: self.dispatch,
            generated,
        })
    }
    /// Reuse a cold seed for the same text: its facts need no edit mapping.
    fn adopt_seed(&mut self, text: &'a SourceText, seed: &Seed<'a>) {
        if let Some(generated) = seed.generated {
            self.source = Reader::contiguous(text, generated.source);
        }
        self.labels = seed.arena.labels.to_vec();
        self.label_ids = label_ids(&self.labels);
        self.arena = ArenaBuilder::from_published(&seed.arena);
        self.reusable = seed.recognitions.clone();
        self.index = seed.recognitions.clone();
        self.dispatch = seed.dispatch.clone();
    }
    fn key_minimum(&self, rule: u16, minimum: u32) -> u32 {
        if self
            .minimum_sensitive
            .get(rule as usize)
            .copied()
            .unwrap_or(true)
        {
            minimum
        } else {
            0
        }
    }

    fn mark(&self) -> Mark {
        Mark {
            position: self.position,
            trail: self.arena.mark(),
        }
    }
    fn restore(&mut self, mark: Mark) {
        self.position = mark.position;
        self.arena.restore(mark.trail);
        self.enforce_cache_budget();
    }
    fn clear_memo(&mut self, generation: usize) {
        for entry in std::mem::take(&mut self.memo[generation]).into_values() {
            if let Memo::Hit { root, .. } = entry {
                self.arena.release_cache(root.node);
            }
        }
        self.memo_cost[generation] = 0;
    }
    /// Retire the older generation for a new one. Its buckets are kept: the
    /// new generation refills to the same bounded size.
    fn rotate_memo(&mut self) {
        self.memo.swap(0, 1);
        self.memo_cost.swap(0, 1);
        for (_, entry) in self.memo[0].drain() {
            if let Memo::Hit { root, .. } = entry {
                self.arena.release_cache(root.node);
            }
        }
        self.memo_cost[0] = 0;
    }
    #[inline]
    fn memo_storage_bound(&self) -> u64 {
        self.memo
            .iter()
            .map(|m| {
                if m.capacity() == 0 {
                    0
                } else {
                    (m.capacity() * std::mem::size_of::<(MemoKey, Memo)>() * 2 + 64) as u64
                }
            })
            .sum::<u64>()
            + self.prior_paths_bytes as u64
            + if self.prior_paths.capacity() == 0 {
                0
            } else {
                (self.prior_paths.capacity()
                    * 2
                    * (std::mem::size_of::<((SubtreeId, u32), PriorPath)>() + 1)
                    + 64) as u64
            }
    }
    /// Runs after every backtrack; the bound check is the common case.
    #[inline]
    fn enforce_cache_budget(&mut self) {
        if self.arena.cache_only_bytes() + self.memo_storage_bound() > self.cache_budget {
            self.evict_caches();
        }
    }
    #[cold]
    #[inline(never)]
    fn evict_caches(&mut self) {
        let budget = self.cache_budget;
        self.stats.cache_evictions += 1;
        self.clear_memo(1);
        self.clear_memo(0);
        self.prior_paths.clear();
        self.prior_paths.shrink_to_fit();
        self.prior_paths_bytes = 0;
        let mut remaining = self.pending_order.len();
        while self.arena.cache_only_bytes() > budget && remaining != 0 {
            let Some(key) = self.pending_order.pop_back() else {
                break;
            };
            let Some(hit) = self.pending_index.get(&key).copied() else {
                remaining -= 1;
                continue;
            };
            if hit.root != SubtreeId::NONE && !self.arena.is_cache_only(hit.root) {
                // Its active/history owner already keeps the syntax alive.
                // Deleting this fact cannot relieve graph pressure, but loses
                // useful recognition for the first edit of a cold snapshot.
                self.pending_order.push_front(key);
                remaining -= 1;
                continue;
            }
            self.pending_index.remove(&key);
            self.pending_failures -= usize::from(hit.root == SubtreeId::NONE);
            self.arena.release_cache(hit.root);
            // Releasing a cached parent may expose previously owned children.
            remaining = self.pending_order.len();
        }
        self.arena.trim_dead_ids();
        debug_assert!(self.arena.cache_only_bytes() + self.memo_storage_bound() <= budget);
    }
    fn recycle(pool: &mut Vec<Children>, mut buffer: Children) {
        if buffer.allocated() {
            buffer.clear();
            pool.push(buffer);
        }
    }
    fn find_prior(&mut self, rule: u16, start: u32) -> Result<Option<PriorNode>, Diagnostic> {
        let approximate = self.reusable.construction_position(start).unwrap_or(start);
        let mut scope = None;
        for i in (0..self.prior_nodes.len()).rev() {
            let prior = self.prior_nodes[i];
            let Some(node) = self.arena.node(prior.node) else {
                continue;
            };
            if node.rule == rule && (prior.start == start || prior.old_start == approximate) {
                return Ok(Some(PriorNode { start, ..prior }));
            }
            if scope.is_none()
                && (prior.start == start
                    || (prior.old_start <= approximate
                        && approximate <= prior.old_start + node.width))
            {
                scope = Some(prior);
            }
        }
        let Some(mut cursor) = scope else {
            return Ok(None);
        };
        let key = (cursor.node, start);
        if let Some(path) = self.prior_paths.get(&key) {
            return Ok(path.get(&rule).map(|(p, _)| PriorNode { start, ..*p }));
        }
        let at = approximate.max(cursor.old_start);
        let mut path = PriorPath::default();
        loop {
            self.tick(0)?;
            let Some(node) = self.arena.node(cursor.node) else {
                break;
            };
            let exact = cursor.start == start || cursor.old_start == at;
            if path.get(&node.rule).is_none_or(|(_, old_exact)| !old_exact) {
                path.insert(node.rule, (cursor, exact));
            }
            if at < cursor.old_start || at >= cursor.old_start + node.width {
                break;
            }
            let offset = at - cursor.old_start;
            let next = if let Some(sequence) = self.arena.sequence(cursor.node) {
                self.work = self
                    .work
                    .saturating_add(u64::from(sequence.len().max(1).ilog2()));
                self.tick(0)?;
                sequence.locate(u64::from(offset)).map(|(_, edge, within)| {
                    let end = at - within as u32 + edge.extent;
                    let old_start = end - self.arena.node(edge.node).unwrap().width;
                    PriorNode {
                        node: edge.node,
                        start: self
                            .reusable
                            .current_position(old_start)
                            .unwrap_or(old_start),
                        old_start,
                    }
                })
            } else {
                self.work = self.work.saturating_add(u64::from(node.child_count()));
                self.tick(0)?;
                let mut previous = cursor.old_start;
                let mut chosen = None;
                let mut score = 0;
                for edge in self.arena.children(cursor.node) {
                    let end = previous + edge.extent;
                    let child = self.arena.node(edge.node).unwrap();
                    let old_start = end - child.width;
                    if previous <= at && at <= end {
                        let rank = 1
                            + u8::from(old_start == at) * 4
                            + u8::from(old_start <= at && at < end) * 2;
                        if rank > score {
                            score = rank;
                            chosen = Some(PriorNode {
                                node: edge.node,
                                start: self
                                    .reusable
                                    .current_position(old_start)
                                    .unwrap_or(old_start),
                                old_start,
                            });
                        }
                    }
                    previous = end;
                }
                chosen
            };
            let Some(next) = next else { break };
            cursor = next;
        }
        let result = path.get(&rule).map(|(p, _)| PriorNode { start, ..*p });
        // Hints are optional construction accelerators. Keep only a bounded
        // recent generation instead of retaining paths for the whole file.
        let budget = (self.cache_budget as usize / 4).min(2 * 1024 * 1024);
        let bytes = path
            .capacity()
            .saturating_mul(2 * std::mem::size_of::<(u16, (PriorNode, bool))>())
            .saturating_add(64);
        let directory = (self.prior_paths.len() + 1)
            .next_power_of_two()
            .saturating_mul(2 * (std::mem::size_of::<((SubtreeId, u32), PriorPath)>() + 1))
            .saturating_add(64);
        if self.prior_paths.len() >= 4096
            || self
                .prior_paths_bytes
                .saturating_add(bytes)
                .saturating_add(directory)
                > budget
        {
            self.prior_paths.clear();
            self.prior_paths.shrink_to_fit();
            self.prior_paths_bytes = 0;
        }
        if bytes.saturating_add(256) <= budget {
            self.prior_paths_bytes += bytes;
            self.prior_paths.insert(key, path);
        }
        Ok(result)
    }
    fn child_buffer(&mut self, rule: u16, start: usize) -> Result<Children, Diagnostic> {
        let prior = self.find_prior(rule, start as u32)?.and_then(|p| {
            self.arena
                .sequence(p.node)
                .map(|s| (p.node, p.old_start, s))
        });
        let mut buffer = self.edge_buffers.pop().unwrap_or_default();
        buffer.reset(start, prior);
        Ok(buffer)
    }
    fn consider(
        &mut self,
        _mark: Mark,
        node: Handle,
        precedence: u32,
        winner: &mut Option<Winner>,
    ) {
        let rank = (self.position, precedence);
        if winner.as_ref().is_none_or(|w| rank > (w.end, w.precedence)) {
            self.arena.retain(node.node);
            if let Some(old) = winner.as_ref() {
                self.arena.release(old.node.node);
            }
            *winner = Some(Winner {
                end: self.position,
                precedence,
                node,
                tied: false,
            });
        } else if let Some(winner) = winner {
            if rank == (winner.end, winner.precedence) {
                winner.tied = true;
            }
        }
    }
    fn accept_winner(
        &mut self,
        mark: Mark,
        winner: Option<Winner>,
    ) -> Result<Option<Handle>, Diagnostic> {
        self.restore(mark);
        let Some(winner) = winner else {
            return Ok(None);
        };
        if winner.tied {
            self.arena.release(winner.node.node);
            return Err(Diagnostic::new(
                "ambiguity",
                [mark.position as u32, winner.end as u32],
                "equally ranked extensible alternatives are outside current admission",
            ));
        }
        self.position = winner.end;
        self.arena.adopt(winner.node.node);
        Ok(Some(winner.node))
    }
    fn add_node(
        &mut self,
        rule: u16,
        start: usize,
        children: &Children,
    ) -> Result<Handle, Diagnostic> {
        if self.in_token > 1 {
            return Ok(NO_NODE);
        }
        let node = self.arena.add_prepared(
            rule,
            (self.position - start) as u32,
            children.iter(),
            children.prior_node(),
            children.prepare(),
        )?;
        Ok(Handle {
            node,
            start: start as u32,
            width: (self.position - start) as u32,
        })
    }
    fn hole(&self, _category: u16) -> Option<usize> {
        None
    }
    fn rule(&mut self, id: u16, minimum: u32, depth: u32) -> Result<Option<Handle>, Diagnostic> {
        let at = self.position as u32;
        let checkpoint = std::mem::replace(&mut self.checkpoint_next, false);
        let key = Key {
            rule: id,
            minimum: self.key_minimum(id, minimum),
            in_token: self.in_token.min(2) as u8,
        };
        let old = self.reusable.get(at, key);
        let current = checkpoint
            .then(|| self.pending_index.get(&(at, key)).copied())
            .flatten();
        // Facts may have been produced by a different recognition specialization
        // or a higher frame budget. An over-depth fact is not proof that normal
        // recognition needs that depth; decline reuse and run the recognizer.
        if let Some(hit) = old
            .or(current)
            .filter(|hit| depth.saturating_add(hit.depth) <= self.limits.parser_frames)
        {
            self.tick(depth.saturating_add(hit.depth))?;
            self.max_depth = self.max_depth.max(depth.saturating_add(hit.depth));
            self.source.observe(u64::from(at) + hit.observed_offset);
            self.position = at as usize + hit.end_offset as usize;
            self.furthest = self
                .furthest
                .max(at as usize + hit.furthest_offset as usize);
            if old.is_some() {
                self.stats.reused_subtrees += u64::from(hit.root != SubtreeId::NONE);
                let range = [at, self.position as u32];
                if range[0] == range[1] {
                } else if let Some(last) = self
                    .reused_ranges
                    .last_mut()
                    .filter(|last| last[0] <= range[0] && range[0] <= last[1])
                {
                    last[1] = last[1].max(range[1]);
                } else {
                    self.reused_ranges.push(range);
                }
            } else {
                self.stats.memo_hits += 1;
            }
            if hit.root == SubtreeId::NONE {
                return Ok(None);
            }
            self.arena.clone_temporary(hit.root);
            return Ok(Some(Handle {
                node: hit.root,
                start: at + hit.root_offset,
                width: self.arena.node(hit.root).expect("cached subtree").width,
            }));
        }
        if !checkpoint {
            return self.rule_inner(id, minimum, depth);
        }
        let prior = [self.reusable.construction_position(at), Some(at)]
            .into_iter()
            .flatten()
            .find_map(|old_at| {
                let hit = self.prior.get(old_at, key)?;
                self.arena.node(hit.root)?;
                let old_start = old_at + hit.root_offset;
                Some(PriorNode {
                    node: hit.root,
                    start: self
                        .reusable
                        .current_position(old_start)
                        .unwrap_or(old_start),
                    old_start,
                })
            });
        if let Some(prior) = prior {
            self.prior_nodes.push(prior);
        }
        let saved_observed = self.source.replace_observed(u64::from(at));
        let saved_depth = std::mem::replace(&mut self.max_depth, depth);
        let saved_furthest = std::mem::replace(&mut self.furthest, at as usize);
        let result = self.rule_inner(id, minimum, depth);
        if prior.is_some() {
            self.prior_nodes.pop();
        }
        let observed = self.source.replace_observed(saved_observed);
        self.source.observe(observed);
        let local_depth = std::mem::replace(&mut self.max_depth, saved_depth);
        self.max_depth = self.max_depth.max(local_depth);
        let local_furthest = std::mem::replace(&mut self.furthest, saved_furthest);
        self.furthest = self.furthest.max(local_furthest);
        let record = match result {
            Ok(Some(root)) if root != NO_NODE => Some(root),
            Ok(None) if self.failure_checkpoint == Some((at, key)) => Some(NO_NODE),
            _ => None,
        };
        if let Some(root) = record {
            if !self.language.rules[id as usize].lexical
                && (root == NO_NODE || self.position > at as usize)
            {
                let hit = Hit {
                    root: root.node,
                    root_offset: if root == NO_NODE { 0 } else { root.start - at },
                    end_offset: if root == NO_NODE {
                        0
                    } else {
                        self.position as u32 - at
                    },
                    observed_offset: observed.saturating_sub(u64::from(at)),
                    furthest_offset: (local_furthest as u32).saturating_sub(at),
                    depth: local_depth.saturating_sub(depth),
                };
                // Failed outer probes are a disposable lookup cache, not live output.
                // Bound their metadata independently of graph ownership.
                if root == NO_NODE
                    && self.pending_failures >= (self.cache_budget as usize / 512).max(1)
                    && !self.pending_index.contains_key(&(at, key))
                {
                    return result;
                }
                self.arena.retain_cache(root.node);
                if let Some(old) = self.pending_index.insert((at, key), hit) {
                    self.pending_failures -= usize::from(old.root == SubtreeId::NONE);
                    self.arena.release_cache(old.root);
                } else {
                    self.pending_order.push_back((at, key));
                }
                self.pending_failures += usize::from(root == NO_NODE);
            }
        }
        result
    }
    fn rule_inner(
        &mut self,
        id: u16,
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        let lexical = self
            .language
            .rules
            .get(id as usize)
            .is_some_and(|r| r.lexical);
        if !lexical || self.options.token_children {
            return self.match_rule(id, minimum, depth);
        }
        if self.optimize_recognition
            && self.in_token == 0
            && !matches!(self.language.rules[id as usize].expr, Expr::Pratt { .. })
        {
            return self.lexical_cached(id, minimum, depth);
        }
        self.in_token += 1;
        let result = self.match_rule(id, minimum, depth);
        self.in_token -= 1;
        match result {
            Ok(Some(_)) if self.in_token > 0 => Ok(Some(NO_NODE)),
            other => other,
        }
    }
    fn native_lexical(
        &mut self,
        id: u16,
        depth: u32,
    ) -> Option<Result<Option<Handle>, Diagnostic>> {
        let start = self.position;
        let (end, work, scan_depth, observed) = self.scan(id, start, depth)?;
        self.work = self.work.saturating_add(work);
        self.max_depth = self.max_depth.max(scan_depth);
        self.source.observe(observed);
        self.stats.native_lexical_calls += 1;
        self.stats.native_lexical_work = self.stats.native_lexical_work.saturating_add(work);
        let Some(end) = end else {
            self.furthest = self.furthest.max(start);
            return Some(Ok(None));
        };
        self.position = end;
        Some(self.lexical_leaf(id, start).map(Some))
    }
    /// The node a lexical match from `start` to the current position yields.
    fn lexical_leaf(&mut self, id: u16, start: usize) -> Result<Handle, Diagnostic> {
        let rule = &self.language.rules[id as usize];
        if rule.alias && !self.options.alias_nodes && rule.owner.is_none() {
            return Ok(NO_NODE);
        }
        self.add_node(id, start, &Children::new(start, None))
    }
    /// A compiled scanner's verdict at `start`, publishing nothing: its end
    /// when matched, logical work, depth and observation. `None` without a
    /// scanner, contiguous text or a valid native verdict.
    fn scan(
        &mut self,
        id: u16,
        start: usize,
        depth: u32,
    ) -> Option<(Option<usize>, u64, u32, u64)> {
        let scanner = self.lexers?.scanners.get(id as usize).copied().flatten()?;
        let source = self.source.contiguous_text()?;
        let mut context = crate::lexical::LexicalContext::new(
            source,
            start,
            self.work_budget.saturating_sub(self.work),
            self.limits.parser_frames,
            self.stack_base,
            self.stack_budget,
        );
        let matched = scanner(&mut context, depth)?;
        // Validate the publication boundary even for host-supplied callbacks.
        let end = context.position();
        if end < start || end > source.len() || !source.is_char_boundary(end) {
            return None;
        }
        Some((
            matched.then_some(end),
            context.work,
            context.max_depth,
            context.observed,
        ))
    }
    /// `lexical_cached` for a prediction, which needs only the end: the same
    /// memo hit, or the same scan, charges and memo entry, without the leaf.
    /// `None` defers to the full call (no scanner or a declined scan).
    fn lexical_predicted(
        &mut self,
        id: u16,
        at: usize,
        depth: u32,
    ) -> Option<Result<Option<usize>, Diagnostic>> {
        let key = MemoKey::new(id, at as u32, 0, 0x80);
        if let Some(entry) = self.memo.iter().find_map(|m| m.get(&key)).copied() {
            if let Err(error) = self.tick(depth) {
                return Some(Err(error));
            }
            self.stats.memo_hits += 1;
            self.stats.lexical_cache_hits += 1;
            let (observed, relative_depth) = entry.replay();
            let maximum = depth.saturating_add(relative_depth);
            self.max_depth = self.max_depth.max(maximum);
            if maximum > self.limits.parser_frames {
                return Some(Err(Diagnostic::new(
                    "limit",
                    [at as u32; 2],
                    "parser depth/work limit exceeded",
                )));
            }
            self.source.observe(observed);
            return Some(Ok(match entry {
                Memo::Fail { .. } => None,
                Memo::Hit { end, .. } | Memo::Scanned { end, .. } => Some(end),
            }));
        }
        let (end, work, scan_depth, observed) = self.scan(id, at, depth)?;
        self.work = self.work.saturating_add(work);
        self.stats.native_lexical_calls += 1;
        self.stats.native_lexical_work = self.stats.native_lexical_work.saturating_add(work);
        let local_depth = depth.max(scan_depth);
        self.max_depth = self.max_depth.max(local_depth);
        self.source.observe(observed);
        // The leaf this match would build weighs a bare node, unless the rule
        // is an alias that builds none.
        let rule = &self.language.rules[id as usize];
        let weight = if rule.alias && !self.options.alias_nodes && rule.owner.is_none() {
            0
        } else {
            32
        };
        let cost = weight + 256;
        if self.memo_cost[0] + cost > 2 * 1024 * 1024 || self.memo[0].len() >= self.memo_capacity {
            self.rotate_memo();
        }
        let relative_depth = local_depth.saturating_sub(depth);
        let entry = match end {
            None => Memo::Fail {
                furthest: at,
                observed,
                depth: relative_depth,
            },
            Some(end) => Memo::Scanned {
                end,
                furthest: 0,
                observed,
                depth: relative_depth,
            },
        };
        self.memo[0].insert(key, entry);
        self.memo_cost[0] += cost;
        Some(Ok(end))
    }
    fn lexical_cached(
        &mut self,
        id: u16,
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        // Pratt's context byte is 0..=2. The high bit reserves a separate
        // namespace while sharing the existing bounded generations/ownership.
        let key = MemoKey::new(id, self.position as u32, minimum, 0x80);
        if let Some(entry) = self.memo.iter().find_map(|m| m.get(&key)).copied() {
            self.tick(depth)?;
            self.stats.memo_hits += 1;
            self.stats.lexical_cache_hits += 1;
            let (observed, relative_depth) = entry.replay();
            let maximum = depth.saturating_add(relative_depth);
            self.max_depth = self.max_depth.max(maximum);
            if maximum > self.limits.parser_frames {
                return Err(Diagnostic::new(
                    "limit",
                    [self.position as u32; 2],
                    "parser depth/work limit exceeded",
                ));
            }
            self.source.observe(observed);
            return match entry {
                Memo::Fail { furthest, .. } => {
                    self.furthest = self.furthest.max(furthest);
                    Ok(None)
                }
                Memo::Hit {
                    end,
                    furthest,
                    root,
                    ..
                } => {
                    self.arena.clone_temporary(root.node);
                    self.position = end;
                    self.furthest = self.furthest.max(furthest);
                    Ok(Some(root))
                }
                Memo::Scanned {
                    end,
                    furthest,
                    observed,
                    depth: relative_depth,
                } => {
                    // Build the leaf a lexical miss would have built, and
                    // hold it in the entry as that miss would have.
                    let start = self.position;
                    self.position = end;
                    self.furthest = self.furthest.max(furthest);
                    let root = self.lexical_leaf(id, start)?;
                    self.arena.retain_cache(root.node);
                    let hit = Memo::Hit {
                        end,
                        furthest,
                        root,
                        observed,
                        depth: relative_depth,
                    };
                    if let Some(slot) = self.memo.iter_mut().find_map(|m| m.get_mut(&key)) {
                        *slot = hit;
                    }
                    Ok(Some(root))
                }
            };
        }
        let saved_observed = self.source.replace_observed(self.position as u64);
        let saved_depth = std::mem::replace(&mut self.max_depth, depth);
        let saved_furthest = std::mem::replace(&mut self.furthest, 0);
        self.in_token += 1;
        let result = self
            .native_lexical(id, depth)
            .unwrap_or_else(|| self.match_rule(id, minimum, depth));
        self.in_token -= 1;
        let observed = self.source.replace_observed(saved_observed);
        self.source.observe(observed);
        let local_depth = std::mem::replace(&mut self.max_depth, saved_depth);
        self.max_depth = self.max_depth.max(local_depth);
        let furthest = std::mem::replace(&mut self.furthest, saved_furthest);
        self.furthest = self.furthest.max(furthest);
        let root = result?;
        let weight = root.map_or(0, |root| self.arena.weight(root.node));
        if weight > 8192 * 64 {
            return Ok(root);
        }
        let cost = weight as usize + 256;
        if self.memo_cost[0] + cost > 2 * 1024 * 1024 || self.memo[0].len() >= self.memo_capacity {
            self.rotate_memo();
        }
        let relative_depth = local_depth.saturating_sub(depth);
        let entry = match root {
            None => Memo::Fail {
                furthest,
                observed,
                depth: relative_depth,
            },
            Some(root) => {
                self.arena.retain_cache(root.node);
                Memo::Hit {
                    end: self.position,
                    furthest,
                    root,
                    observed,
                    depth: relative_depth,
                }
            }
        };
        self.memo[0].insert(key, entry);
        self.memo_cost[0] += cost;
        Ok(root)
    }
    fn stand_in(&mut self, node: Handle) -> Handle {
        if node == NO_NODE {
            return node;
        }
        let current = self.arena.node(node.node).expect("built node");
        if !self.language.rules[current.rule as usize].transparent || current.child_count() != 1 {
            return node;
        }
        let child = self
            .arena
            .children(node.node)
            .next()
            .expect("one transparent child");
        let width = self.arena.node(child.node).unwrap().width;
        self.arena.replace_last_temporary(node.node, child.node);
        Handle {
            node: child.node,
            start: node.start + child.extent - width,
            width,
        }
    }
    fn pratt(
        &mut self,
        point: u16,
        prefixes: &'static [u16],
        infixes: &'static [u16],
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        let key = MemoKey::new(
            point,
            self.position as u32,
            minimum,
            self.in_token.min(2) as u8,
        );
        if let Some(entry) = self.memo.iter().find_map(|m| m.get(&key)).copied() {
            self.stats.memo_hits += 1;
            let (observed, relative_depth) = entry.replay();
            let maximum = depth.saturating_add(relative_depth);
            self.max_depth = self.max_depth.max(maximum);
            if maximum > self.limits.parser_frames {
                return Err(Diagnostic::new(
                    "limit",
                    [self.position as u32; 2],
                    "parser depth/work limit exceeded",
                ));
            }
            self.source.observe(observed);
            return match entry {
                Memo::Fail { furthest, .. } => {
                    self.furthest = self.furthest.max(furthest);
                    Ok(None)
                }
                Memo::Hit {
                    end,
                    furthest,
                    root,
                    ..
                } => {
                    self.arena.clone_temporary(root.node);
                    self.position = end;
                    self.furthest = self.furthest.max(furthest);
                    Ok(Some(root))
                }
                Memo::Scanned { .. } => unreachable!("Pratt keys hold no lexical scans"),
            };
        }
        let result = self.pratt_uncached(point, prefixes, infixes, minimum, depth)?;
        let weight = result.map_or(0, |root| self.arena.weight(root.node));
        if weight > 8192 * 64 {
            return Ok(result);
        }
        let cost = weight as usize + 256;
        if self.memo_cost[0] + cost > 2 * 1024 * 1024 || self.memo[0].len() >= self.memo_capacity {
            self.rotate_memo();
        }
        let observed = self.source.observed();
        let relative_depth = self.max_depth.saturating_sub(depth);
        let entry = match result {
            None => Memo::Fail {
                furthest: self.furthest,
                observed,
                depth: relative_depth,
            },
            Some(root) => {
                self.arena.retain_cache(root.node);
                Memo::Hit {
                    end: self.position,
                    furthest: self.furthest,
                    root,
                    observed,
                    depth: relative_depth,
                }
            }
        };
        self.memo[0].insert(key, entry);
        self.memo_cost[0] += cost;
        Ok(result)
    }
    fn trivia(&mut self) -> Result<(), Diagnostic> {
        // Trivia at a position is fixed by the source: skip it once, then
        // replay its end, observation and logical work. One physical check
        // covers a replay.
        if !(self.optimize_recognition && self.compiled.is_some()) {
            return self.trivia_scan();
        }
        let start = self.position;
        if let Some((end, observed, steps)) = self.tokens.trivia(start) {
            self.tick_steps(steps as usize, 0)?;
            self.position = end;
            self.source.observe(observed);
            return Ok(());
        }
        let saved = self.source.replace_observed(start as u64);
        let work = self.work;
        let result = self.trivia_scan();
        let observed = self.source.replace_observed(saved);
        self.source.observe(observed);
        result?;
        let steps = (self.work - work).min(u64::from(u32::MAX)) as u32;
        self.tokens
            .remember_trivia(start, self.position, observed, steps);
        Ok(())
    }
    fn trivia_scan(&mut self) -> Result<(), Diagnostic> {
        loop {
            self.tick(0)?;
            let character = self.source.character(self.position);
            if let Some(c) = character.filter(|c| c.is_whitespace()) {
                self.position += c.len_utf8();
            } else if self.optimize_recognition && self.compiled.is_some() && character != Some('/')
            {
                // Both original comment checks observe two bytes even when
                // their first byte cannot match. Preserve that dependency reach.
                self.source
                    .observe(self.position.saturating_add(2).min(self.source.len() + 1) as u64);
                self.stats.trivia_fast_skips += 1;
                return Ok(());
            } else if self.source.matches(self.position, "//") {
                self.position = self
                    .source
                    .find(self.position, "\n")
                    .unwrap_or(self.source.len());
            } else if self.source.matches(self.position, "/*") {
                if let Some(end) = self.source.find(self.position + 2, "*/") {
                    self.position = end + 2;
                } else if self.options.strict_comments {
                    return Err(Diagnostic::new(
                        "syntax",
                        [self.position as u32, self.source.len() as u32],
                        "unterminated comment",
                    ));
                } else {
                    self.position = self.source.len();
                }
            } else {
                return Ok(());
            }
        }
    }
    fn intern(&mut self, label: &'static str) -> Result<u16, Diagnostic> {
        if label.is_empty() {
            return Ok(0);
        }
        let key = (label.as_ptr() as usize, label.len());
        if let Some(&id) = self.label_ids.get(&key) {
            return Ok(id);
        }
        let id = u16::try_from(self.labels.len()).map_err(|_| {
            Diagnostic::new(
                "limit",
                [self.position as u32; 2],
                "too many distinct labels",
            )
        })?;
        self.labels.push(label);
        self.label_ids.insert(key, id);
        Ok(id)
    }
    #[inline]
    fn tick(&mut self, depth: u32) -> Result<(), Diagnostic> {
        self.work += 1;
        self.max_depth = self.max_depth.max(depth);
        let stack = super::stack_position_inline().abs_diff(self.stack_base);
        if stack > self.stack_budget
            || depth > self.limits.parser_frames
            || self.work > self.work_budget
        {
            return Err(self.tick_error(stack));
        }
        Ok(())
    }
    /// The stack limit takes precedence over the depth and work limits.
    #[cold]
    #[inline(never)]
    fn tick_error(&self, stack: usize) -> Diagnostic {
        let message = if stack > self.stack_budget {
            super::STACK_EXHAUSTED
        } else {
            "parser depth/work limit exceeded"
        };
        Diagnostic::new("limit", [self.position as u32; 2], message)
    }
    fn match_rule(
        &mut self,
        id: u16,
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        self.tick(depth)?;
        let rule = self
            .language
            .rules
            .get(id as usize)
            .ok_or_else(|| Diagnostic::new("grammar", [0, 0], "invalid rule identity"))?;
        let mark = self.mark();
        if !rule.lexical {
            self.trivia()?;
        }
        let start = self.position;
        let prior = self.find_prior(id, start as u32)?;
        if let Some(prior) = prior {
            self.prior_nodes.push(prior);
        }
        let result = self.match_contents(id, minimum, depth, mark, start);
        if prior.is_some() {
            self.prior_nodes.pop();
        }
        result
    }
    fn match_contents(
        &mut self,
        id: u16,
        minimum: u32,
        depth: u32,
        mark: Mark,
        start: usize,
    ) -> Result<Option<Handle>, Diagnostic> {
        let rule = &self.language.rules[id as usize];
        if let Expr::Pratt {
            prefixes,
            infixes,
            memoize,
        } = rule.expr
        {
            return if memoize {
                self.pratt(id, prefixes, infixes, minimum, depth + 1)
            } else {
                self.pratt_uncached(id, prefixes, infixes, minimum, depth + 1)
            };
        }
        if !rule.lexical {
            if let Some(length) = self.hole(id) {
                self.position += length;
                return self
                    .add_node(id, start, &Children::new(start, None))
                    .map(Some);
            }
        }
        let mut children = self.child_buffer(id, start)?;
        let lexical_failure_base = self.furthest;
        // Only a direct final reference to the extensible point is an operand.
        // Enclosed expressions start a fresh precedence context, as in Nitrogen.
        let compiled = self
            .compiled
            .filter(|_| self.optimize_recognition && !rule.lexical)
            .and_then(|p| p.bodies.get(id as usize).copied().flatten());
        let matched = if let Some(body) = compiled {
            self.stats.native_syntax_calls += 1;
            body(
                &mut SyntaxContext::new(self, &mut children),
                minimum,
                depth + 1,
            )?
        } else if rule.owner.is_some() {
            let parts = match &rule.expr {
                Expr::Sequence(parts) => *parts,
                expr => std::slice::from_ref(expr),
            };
            let mut matched = true;
            for (index, part) in parts.iter().enumerate() {
                let owner = if index + 1 == parts.len() {
                    rule.owner
                } else {
                    None
                };
                if !self.expression(part, rule.lexical, owner, minimum, depth + 1, &mut children)? {
                    matched = false;
                    break;
                }
            }
            matched
        } else {
            self.expression(&rule.expr, rule.lexical, None, 0, depth + 1, &mut children)?
        };
        if rule.lexical {
            self.furthest = lexical_failure_base;
        }
        if !matched
            || rule
                .except
                .iter()
                .any(|text| text.len() == self.position - start && self.source.matches(start, text))
        {
            self.furthest = self
                .furthest
                .max(if rule.lexical { start } else { self.position });
            Self::recycle(&mut self.edge_buffers, children);
            self.restore(mark);
            return Ok(None);
        }
        // An alias builds no node: its one child stands in, labeled by the
        // reference to the alias. With no child, nothing remains, except for
        // an extensible alternative, which must yield an operand node.
        if rule.alias
            && !self.options.alias_nodes
            && (children.len() == 1 || (children.is_empty() && rule.owner.is_none()))
        {
            let node = children.first_node().unwrap_or(NO_NODE);
            self.arena.commit(mark.trail, node.node);
            Self::recycle(&mut self.edge_buffers, children);
            return Ok(Some(node));
        }
        let result = self.add_node(id, start, &children);
        if let Ok(node) = result {
            self.arena.commit(mark.trail, node.node);
        }
        Self::recycle(&mut self.edge_buffers, children);
        result.map(Some)
    }
    fn choice_plan(
        &mut self,
        expr: &'static Expr<'static>,
        parts: &'static [Expr],
        lexical: bool,
    ) -> Option<std::sync::Arc<dispatch::ChoicePlan>> {
        if !self.optimize_recognition || !(2..=256).contains(&parts.len()) {
            return None;
        }
        if lexical && parts.len() < 4 {
            return None;
        }
        let limit = (self.cache_budget / 8).min(256 * 1024) as usize;
        let base_cost = std::mem::size_of::<dispatch::ChoicePlan>() + 192;
        if self.choice_cache_bytes > limit {
            self.choice_cache = Default::default();
            self.choice_cache_bytes = 0;
        }
        if base_cost > limit {
            return None;
        }
        // Lexical and syntactic uses of one expression plan differently.
        let key = expr as *const Expr as usize | usize::from(lexical);
        if let Some(plan) = self.choice_cache.get(&key) {
            return plan.clone();
        }
        self.stats.choice_plan_builds += 1;
        let starts: Vec<_> = parts
            .iter()
            .map(|part| self.dispatch.choice_start(part, self.language))
            .collect();
        let leads = if lexical {
            Box::default()
        } else {
            parts
                .iter()
                .map(|part| self.dispatch.alternative_lead(part, self.language))
                .collect()
        };
        let plan = dispatch::ChoicePlan::new(&starts, leads).map(std::sync::Arc::new);
        let cost = base_cost + plan.as_ref().map_or(0, |plan| plan.bytes());
        if cost > limit {
            return None;
        }
        if self.choice_cache_bytes + cost > limit {
            // Drop buckets too: retained capacity must not escape the charge.
            self.choice_cache = Default::default();
            self.choice_cache_bytes = 0;
        }
        self.choice_cache_bytes += cost;
        self.choice_cache.insert(key, plan.clone());
        plan
    }
    #[inline]
    fn tick_steps(&mut self, count: usize, depth: u32) -> Result<(), Diagnostic> {
        if count == 0 {
            return Ok(());
        }
        // Skipped alternatives don't recurse. One physical stack/depth check is
        // sufficient; the virtual work budget still stops at its first excess.
        self.tick(depth)?;
        let extra = (count - 1) as u64;
        if extra > self.work_budget.saturating_sub(self.work) {
            self.work = self.work_budget.saturating_add(1);
            return Err(Diagnostic::new(
                "limit",
                [self.position as u32; 2],
                "parser depth/work limit exceeded",
            ));
        }
        self.work += extra;
        Ok(())
    }
    /// An ordered choice. `attempt` runs alternative `index` at the choice's
    /// start; the interpreter and generated bodies differ only there.
    pub(super) fn choice<F>(
        &mut self,
        expr: &'static Expr<'static>,
        parts: &'static [Expr],
        lexical: bool,
        depth: u32,
        out: &mut Children,
        mut attempt: F,
    ) -> Result<bool, Diagnostic>
    where
        F: FnMut(&mut Self, &mut Children, usize) -> Result<bool, Diagnostic>,
    {
        if let Some(plan) = self.choice_plan(expr, parts, lexical) {
            return self.choice_with_plan(&plan, parts.len(), lexical, depth, out, &mut attempt);
        }
        for index in 0..parts.len() {
            let mark = self.mark();
            let len = out.len();
            if attempt(self, out, index)? {
                return Ok(true);
            }
            self.restore(mark);
            out.truncate(len);
        }
        Ok(false)
    }
    fn choice_with_plan<F>(
        &mut self,
        plan: &dispatch::ChoicePlan,
        count: usize,
        lexical: bool,
        depth: u32,
        out: &mut Children,
        attempt: &mut F,
    ) -> Result<bool, Diagnostic>
    where
        F: FnMut(&mut Self, &mut Children, usize) -> Result<bool, Diagnostic>,
    {
        for index in 0..plan.first() {
            let mark = self.mark();
            let len = out.len();
            if attempt(self, out, index)? {
                return Ok(true);
            }
            self.restore(mark);
            out.truncate(len);
        }
        let saved = (self.position, self.work, self.max_depth, self.furthest);
        let peek = self.dispatch_peek(!lexical);
        (self.position, self.work, self.max_depth, self.furthest) = saved;
        // Prediction never commits work or errors before an alternative's own
        // checks. Keep read observations conservatively; replay real matching
        // when trivia/limits prevented a reliable peek.
        let Ok((at, character)) = peek else {
            for index in plan.first()..count {
                let mark = self.mark();
                let len = out.len();
                if attempt(self, out, index)? {
                    return Ok(true);
                }
                self.restore(mark);
                out.truncate(len);
            }
            return Ok(false);
        };
        let mut peek = Peek::new(at, character);
        let mut cursor = plan.first();
        for index in plan.candidates(character, cursor) {
            if let Some(lead) = plan.lead(index) {
                if let Some(failure) = self.lead_rejects(lead, &mut peek, depth + 1) {
                    self.furthest = self.furthest.max(failure);
                    continue;
                }
            }
            let skipped = index - cursor;
            self.tick_steps(skipped, depth + 1)?;
            if skipped != 0 {
                self.furthest = self.furthest.max(at);
                self.stats.choice_skips += skipped as u64;
            }
            let mark = self.mark();
            let len = out.len();
            if attempt(self, out, index)? {
                return Ok(true);
            }
            self.restore(mark);
            out.truncate(len);
            cursor = index + 1;
        }
        let skipped = count - cursor;
        self.tick_steps(skipped, depth + 1)?;
        if skipped != 0 {
            self.furthest = self.furthest.max(at);
            self.stats.choice_skips += skipped as u64;
        }
        Ok(false)
    }
    /// `None` when the input can begin one of `lead`; otherwise where every
    /// path would fail: the first token, or the token after a matched one.
    /// Tokens are read exactly as the alternative's own first steps would
    /// read them. Any error defers to the alternative, which meets it itself.
    fn lead_rejects(
        &mut self,
        lead: &[dispatch::Step],
        peek: &mut Peek,
        depth: u32,
    ) -> Option<usize> {
        self.lead_failure(lead, peek, depth).unwrap_or(None)
    }
    fn lead_failure(
        &mut self,
        lead: &[dispatch::Step],
        peek: &mut Peek,
        depth: u32,
    ) -> Result<Option<usize>, Diagnostic> {
        let mut furthest = peek.at;
        let byte = peek.character.map_or(dispatch::Dispatch::LEXICAL, |c| {
            if c.is_ascii() {
                c as u8
            } else {
                dispatch::Dispatch::WIDE
            }
        });
        // Steps sort by first token: read each distinct one once.
        let mut first = (u16::MAX, None);
        for step in lead {
            if step.first != first.0 {
                let lead_byte = self.dispatch.first_byte(step.first);
                let end = if lead_byte < dispatch::Dispatch::LEXICAL && lead_byte != byte {
                    None
                } else {
                    self.token_end(step.first, peek.at, peek.character, depth)?
                };
                first = (step.first, end);
            }
            let Some(end) = first.1 else {
                continue;
            };
            let Some(second) = step.second() else {
                return Ok(None);
            };
            let (next, character) = self.after_token(end, peek)?;
            if self.token_end(second, next, character, depth)?.is_some() {
                return Ok(None);
            }
            furthest = furthest.max(next);
        }
        Ok(Some(furthest))
    }
    /// Where terminal `id` ends when read at `at`, exactly as the alternative
    /// would read it. Reads are recorded as observations, cached or not.
    fn token_end(
        &mut self,
        id: u16,
        at: usize,
        character: Option<char>,
        depth: u32,
    ) -> Result<Option<usize>, Diagnostic> {
        match self.dispatch.terminal(id) {
            dispatch::Terminal::Literal(text) => {
                if !text.starts_with(|c| Some(c) == character) {
                    return Ok(None);
                }
                let keyword = self.dispatch.keyword(id);
                Ok(self
                    .literal_at(at, text, keyword)
                    .then_some(at + text.len()))
            }
            dispatch::Terminal::Lexical(rule) => {
                if !self.dispatch.start(rule).allows(character) {
                    return Ok(None);
                }
                if let Some((end, observed)) = self.tokens.lexical(at, rule) {
                    self.source.observe(observed);
                    return Ok(end);
                }
                let saved = self.source.replace_observed(at as u64);
                let end = if self.lexical_memoized(rule) {
                    self.lexical_predicted(rule, at, depth)
                } else {
                    None
                }
                .unwrap_or_else(|| self.lexical_end(rule, at, depth));
                let observed = self.source.replace_observed(saved);
                self.source.observe(observed);
                let end = end?;
                self.tokens.remember_lexical(at, rule, end, observed);
                Ok(end)
            }
        }
    }
    /// The position and character after trivia following a token.
    fn after_token(
        &mut self,
        end: usize,
        peek: &mut Peek,
    ) -> Result<(usize, Option<char>), Diagnostic> {
        if let Some(known) = peek.after(end) {
            return Ok(known);
        }
        let saved = (self.position, self.work, self.max_depth, self.furthest);
        self.position = end;
        let result = self.dispatch_peek(true);
        (self.position, self.work, self.max_depth, self.furthest) = saved;
        let value = result?;
        peek.remember_after(end, value);
        Ok(value)
    }
    /// `literal`'s comparison. A keyword compares against the identifier word
    /// at `at`, which also decides its boundary; reading the whole word only
    /// widens the recorded observation.
    fn literal_at(&mut self, at: usize, text: &str, keyword: bool) -> bool {
        let Some(source) = self.source.contiguous_text() else {
            return self.source.matches(at, text)
                && (!keyword
                    || self
                        .source
                        .character(at + text.len())
                        .is_none_or(|c| !(c.is_alphanumeric() || c == '_')));
        };
        let end = at + text.len();
        if !keyword {
            self.source.observe(end.min(source.len() + 1) as u64);
            return source.as_bytes().get(at..end) == Some(text.as_bytes());
        }
        let (word, observed) = self.tokens.word(source, at);
        self.source
            .observe(observed.max(end.min(source.len() + 1) as u64));
        word == end && source.as_bytes().get(at..end) == Some(text.as_bytes())
    }
    /// Whether a call of lexical rule `id` goes through `lexical_cached`.
    fn lexical_memoized(&self, id: u16) -> bool {
        self.optimize_recognition
            && self.in_token == 0
            && !self.options.token_children
            && !matches!(self.language.rules[id as usize].expr, Expr::Pratt { .. })
    }
    fn lexical_end(&mut self, id: u16, at: usize, depth: u32) -> Result<Option<usize>, Diagnostic> {
        let mark = self.mark();
        let furthest = self.furthest;
        let checkpoint = std::mem::replace(&mut self.checkpoint_next, false);
        self.position = at;
        // Lexical rules have no reusable or pending facts: enter below `rule`.
        let result = self.rule_inner(id, 0, depth);
        let end = self.position;
        self.checkpoint_next = checkpoint;
        self.restore(mark);
        self.furthest = furthest;
        Ok(result?.map(|_| end))
    }
    fn dispatch_peek(&mut self, trivia: bool) -> Result<(usize, Option<char>), Diagnostic> {
        let saved = self.position;
        if trivia {
            self.trivia()?;
        }
        let at = self.position;
        let character = self.source.character(at);
        self.position = saved;
        Ok((at, character))
    }
    fn pratt_uncached(
        &mut self,
        point: u16,
        prefixes: &'static [u16],
        infixes: &'static [u16],
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        if self.optimize_recognition && self.hole(point).is_none() {
            let dispatch = std::sync::Arc::clone(&self.dispatch);
            if let Some(plan) = dispatch.pratt(point) {
                return self.pratt_planned(&dispatch, plan, minimum, depth);
            }
        }
        self.tick(depth)?;
        let prefix_mark = self.mark();
        let mut winner = None;
        // A hole is one operand node; operators may still follow it.
        let hole = self.hole(point);
        let dispatch = std::sync::Arc::clone(&self.dispatch);
        let mut prefix_peek = None;
        let mut prefix_leads = None;
        for id in prefixes.iter().filter(|_| hole.is_none()) {
            self.restore(prefix_mark);
            let rule =
                self.language.rules.get(*id as usize).ok_or_else(|| {
                    Diagnostic::new("grammar", [0, 0], "invalid Pratt rule identity")
                })?;
            let power = rule
                .precedence
                .checked_add(1)
                .ok_or_else(|| Diagnostic::new("grammar", [0, 0], "precedence overflow"))?;
            let start = dispatch.start(*id);
            if self.optimize_recognition && !start.nullable() {
                let (at, character) = if rule.lexical {
                    self.dispatch_peek(false)?
                } else if let Some(value) = prefix_peek {
                    value
                } else {
                    let value = self.dispatch_peek(true)?;
                    prefix_peek = Some(value);
                    value
                };
                let failure = if !start.allows(character) {
                    Some(at)
                } else if let Some(lead) = dispatch.lead(*id).filter(|_| !rule.lexical) {
                    let peek = prefix_leads.get_or_insert_with(|| Peek::new(at, character));
                    self.lead_rejects(lead, peek, depth + 1)
                } else {
                    None
                };
                if let Some(failure) = failure {
                    self.tick(depth + 1)?;
                    self.furthest = self.furthest.max(failure);
                    continue;
                }
            }
            if let Some(node) = self.rule(*id, power, depth + 1)? {
                if self.position > prefix_mark.position {
                    self.consider(prefix_mark, node, rule.precedence, &mut winner);
                }
            }
        }
        let left = if let Some(length) = hole {
            let start = self.position;
            self.position += length;
            Some(self.add_node(point, start, &Children::new(start, None))?)
        } else {
            self.accept_winner(prefix_mark, winner)?
                .map(|node| self.stand_in(node))
        };
        let Some(mut left) = left else {
            return Ok(None);
        };
        loop {
            self.tick(depth)?;
            let infix_mark = self.mark();
            let mut winner = None;
            let mut infix_peek = None;
            let mut infix_leads = None;
            for id in infixes {
                self.restore(infix_mark);
                let rule = self.language.rules.get(*id as usize).ok_or_else(|| {
                    Diagnostic::new("grammar", [0, 0], "invalid Pratt rule identity")
                })?;
                if rule.precedence < minimum {
                    continue;
                }
                let Expr::Sequence(parts) = &rule.expr else {
                    return Err(Diagnostic::new(
                        "grammar",
                        [0, 0],
                        "infix must be a sequence",
                    ));
                };
                let label = match parts.first() {
                    Some(Expr::Label(label, _)) => *label,
                    Some(Expr::Ref(_)) => "",
                    _ => {
                        return Err(Diagnostic::new(
                            "grammar",
                            [0, 0],
                            "invalid infix left operand",
                        ));
                    }
                };
                let power = rule
                    .precedence
                    .checked_add(u32::from(!rule.right_associative))
                    .ok_or_else(|| Diagnostic::new("grammar", [0, 0], "precedence overflow"))?;
                if let Some(start) = dispatch
                    .tail(*id)
                    .filter(|start| self.optimize_recognition && !start.nullable())
                {
                    let (at, character) = if let Some(value) = infix_peek {
                        value
                    } else {
                        let value = self.dispatch_peek(true)?;
                        infix_peek = Some(value);
                        value
                    };
                    let failure = if !start.allows(character) {
                        Some(at)
                    } else if let Some(lead) = dispatch.tail_lead(*id) {
                        let peek = infix_leads.get_or_insert_with(|| Peek::new(at, character));
                        self.lead_rejects(lead, peek, depth + 1)
                    } else {
                        None
                    };
                    if let Some(failure) = failure {
                        self.tick(depth + 1)?;
                        self.furthest = self.furthest.max(failure);
                        continue;
                    }
                }
                let mark = self.mark();
                let start = left.start as usize;
                let mut children = self.child_buffer(*id, start)?;
                let label = self.intern(label)?;
                children.push(
                    Edge { label, node: left },
                    self.reusable.previous_position(left.start + left.width),
                );
                let compiled = self
                    .compiled
                    .filter(|_| self.optimize_recognition)
                    .and_then(|p| p.tails.get(*id as usize).copied().flatten());
                let matched = if let Some(tail) = compiled {
                    self.stats.native_syntax_tails += 1;
                    tail(
                        &mut SyntaxContext::new(self, &mut children),
                        power,
                        depth + 1,
                    )?
                } else {
                    let mut matched = true;
                    for (index, part) in parts.iter().enumerate().skip(1) {
                        let owner = if index + 1 == parts.len() {
                            rule.owner
                        } else {
                            None
                        };
                        if !self.expression(part, false, owner, power, depth + 1, &mut children)? {
                            matched = false;
                            break;
                        }
                    }
                    matched
                };
                if matched {
                    if self.position == mark.position {
                        return Err(Diagnostic::new(
                            "grammar",
                            [start as u32; 2],
                            "zero-width infix",
                        ));
                    }
                    let node = self.add_node(*id, start, &children)?;
                    self.consider(infix_mark, node, rule.precedence, &mut winner);
                }
                Self::recycle(&mut self.edge_buffers, children);
                self.restore(mark);
            }
            if let Some(node) = self.accept_winner(infix_mark, winner)? {
                self.arena.commit(prefix_mark.trail, node.node);
                left = node;
            } else {
                return Ok(Some(left));
            }
        }
    }
    /// `pratt_uncached` over a precomputed plan: one peek per operand or
    /// operator position, routed candidates in grammar order, and one bulk
    /// charge for the excluded remainder.
    fn pratt_planned(
        &mut self,
        dispatch: &dispatch::Dispatch,
        plan: &dispatch::PrattPlan,
        minimum: u32,
        depth: u32,
    ) -> Result<Option<Handle>, Diagnostic> {
        self.tick(depth)?;
        let prefix_mark = self.mark();
        let mut winner = None;
        let (at, character) = self.dispatch_peek(true)?;
        let mut peek = Peek::new(at, character);
        let mut attempted = 0;
        for index in plan.prefix_candidates(character) {
            let prefix = &plan.prefixes[index];
            if let Some(lead) = dispatch.lead(prefix.id) {
                if let Some(failure) = self.lead_rejects(lead, &mut peek, depth + 1) {
                    self.furthest = self.furthest.max(failure);
                    continue;
                }
            }
            if attempted != 0 {
                self.restore(prefix_mark);
            }
            attempted += 1;
            if let Some(node) = self.rule(prefix.id, prefix.power, depth + 1)? {
                if self.position > prefix_mark.position {
                    self.consider(prefix_mark, node, prefix.precedence, &mut winner);
                }
            }
        }
        self.skip_alternatives(plan.prefixes.len() - attempted, at, depth + 1)?;
        let left = self
            .accept_winner(prefix_mark, winner)?
            .map(|node| self.stand_in(node));
        let Some(mut left) = left else {
            return Ok(None);
        };
        loop {
            self.tick(depth)?;
            let infix_mark = self.mark();
            if !plan.has_infixes(minimum) {
                return Ok(Some(left));
            }
            let mut winner = None;
            let (at, character) = self.dispatch_peek(true)?;
            let mut peek = Peek::new(at, character);
            let (candidates, excluded) = plan.infix_candidates(character, minimum);
            self.skip_alternatives(excluded as usize, at, depth + 1)?;
            for index in candidates {
                let infix = &plan.infixes[index];
                if let Some(lead) = dispatch.tail_lead(infix.id) {
                    if let Some(failure) = self.lead_rejects(lead, &mut peek, depth + 1) {
                        self.skip_alternatives(1, failure, depth + 1)?;
                        continue;
                    }
                }
                let mark = self.mark();
                let start = left.start as usize;
                let mut children = self.child_buffer(infix.id, start)?;
                let label = self.intern(infix.label)?;
                children.push(
                    Edge { label, node: left },
                    self.reusable.previous_position(left.start + left.width),
                );
                let compiled = self
                    .compiled
                    .and_then(|p| p.tails.get(infix.id as usize).copied().flatten());
                let matched = if let Some(tail) = compiled {
                    self.stats.native_syntax_tails += 1;
                    tail(
                        &mut SyntaxContext::new(self, &mut children),
                        infix.power,
                        depth + 1,
                    )?
                } else {
                    let mut matched = true;
                    for (index, part) in infix.parts.iter().enumerate().skip(1) {
                        let owner = if index + 1 == infix.parts.len() {
                            infix.owner
                        } else {
                            None
                        };
                        if !self.expression(
                            part,
                            false,
                            owner,
                            infix.power,
                            depth + 1,
                            &mut children,
                        )? {
                            matched = false;
                            break;
                        }
                    }
                    matched
                };
                if matched {
                    if self.position == mark.position {
                        return Err(Diagnostic::new(
                            "grammar",
                            [start as u32; 2],
                            "zero-width infix",
                        ));
                    }
                    let node = self.add_node(infix.id, start, &children)?;
                    self.consider(infix_mark, node, infix.precedence, &mut winner);
                }
                Self::recycle(&mut self.edge_buffers, children);
                self.restore(mark);
            }
            if let Some(node) = self.accept_winner(infix_mark, winner)? {
                self.arena.commit(prefix_mark.trail, node.node);
                left = node;
            } else {
                return Ok(Some(left));
            }
        }
    }
    /// Charge alternatives excluded before reaching their recognizers. One
    /// physical check covers them, as in `tick_steps`.
    fn skip_alternatives(&mut self, count: usize, at: usize, depth: u32) -> Result<(), Diagnostic> {
        if count != 0 {
            self.tick_steps(count, depth)?;
            self.furthest = self.furthest.max(at);
        }
        Ok(())
    }
    fn expression(
        &mut self,
        expr: &'static Expr<'static>,
        lexical: bool,
        owner: Option<u16>,
        power: u32,
        depth: u32,
        out: &mut Children,
    ) -> Result<bool, Diagnostic> {
        self.tick(depth)?;
        if !lexical
            && matches!(
                expr,
                Expr::Literal(_) | Expr::Class(_) | Expr::NotClass(_) | Expr::Any | Expr::Ref(_)
            )
        {
            self.trivia()?;
        }
        let result = match expr {
            Expr::Any => {
                if let Some(c) = self.source.character(self.position) {
                    self.position += c.len_utf8();
                    true
                } else {
                    false
                }
            }
            Expr::Predicate { positive, inner } => {
                let mark = self.mark();
                let furthest = self.furthest;
                let len = out.len();
                let matched = self.expression(inner, lexical, None, 0, depth + 1, out);
                self.restore(mark);
                self.furthest = furthest;
                out.truncate(len);
                matched? == *positive
            }
            Expr::Literal(text) => {
                let keyword = !lexical
                    && text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_')
                    && text.chars().all(|c| c.is_alphanumeric() || c == '_');
                let end = self.position + text.len();
                let boundary = !keyword
                    || self
                        .source
                        .character(end)
                        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                if self.source.matches(self.position, text) && boundary {
                    self.position += text.len();
                    true
                } else {
                    false
                }
            }
            Expr::Class(ranges) | Expr::NotClass(ranges) => {
                if let Some(c) = self.source.character(self.position).filter(|c| {
                    ranges.iter().any(|(a, b)| a <= c && c <= b)
                        != matches!(expr, Expr::NotClass(_))
                }) {
                    self.position += c.len_utf8();
                    true
                } else {
                    false
                }
            }
            Expr::Ref(id) => {
                if let Some(node) =
                    self.rule(*id, if Some(*id) == owner { power } else { 0 }, depth + 1)?
                {
                    if node != NO_NODE {
                        out.push(
                            Edge { label: 0, node },
                            self.reusable.previous_position(node.start + node.width),
                        );
                    }
                    true
                } else {
                    false
                }
            }
            Expr::Label(label, inner) => {
                let from = out.len();
                let matched = self.expression(inner, lexical, owner, power, depth + 1, out)?;
                if out.len() > from {
                    let label = self.intern(label)?;
                    self.work = self.work.saturating_add((out.len() - from) as u64);
                    self.tick(depth)?;
                    out.label(from, label);
                }
                matched
            }
            Expr::Sequence(parts) => {
                let mut matched = true;
                for part in *parts {
                    if !self.expression(part, lexical, None, 0, depth + 1, out)? {
                        matched = false;
                        break;
                    }
                }
                matched
            }
            Expr::Choice(parts) => {
                self.choice(expr, parts, lexical, depth, out, |parser, out, index| {
                    parser.expression(&parts[index], lexical, None, 0, depth + 1, out)
                })?
            }
            Expr::Repeat { inner, min, max } => {
                let mut count = 0;
                while count < *max {
                    let mark = self.mark();
                    let len = out.len();
                    let saved_checkpoint = self.checkpoint_next;
                    if *max > 1 && !lexical {
                        self.checkpoint_next = true;
                    }
                    let matched = self.expression(inner, lexical, None, 0, depth + 1, out);
                    self.checkpoint_next = saved_checkpoint;
                    if !matched? {
                        self.restore(mark);
                        out.truncate(len);
                        break;
                    }
                    if self.position == mark.position && *max > 1 {
                        return Err(Diagnostic::new(
                            "grammar",
                            [self.position as u32; 2],
                            "zero-width repetition",
                        ));
                    }
                    count += 1;
                }
                count >= *min
            }
            Expr::Pratt { .. } => {
                return Err(Diagnostic::new(
                    "grammar",
                    [0, 0],
                    "Pratt expressions must be rule roots",
                ));
            }
        };
        if !result {
            self.furthest = self.furthest.max(self.position);
        }
        Ok(result)
    }
}

/// Recovery's opaque probe handles refer directly to immutable shared records.
/// Only selected handles acquire committed ownership at VM stable boundaries.
type RecoveryKeys = std::collections::HashSet<(u16, [u32; 2]), hash::Seeded>;
impl SharedProbe<'_> {
    pub(crate) fn may_consume(&self, id: u16, c: char) -> bool {
        self.parser.dispatch.may_consume(id, c)
    }
}
pub(crate) struct SharedProbe<'a> {
    parser: Parser<'a>,
    handles: Vec<Handle>,
    active: Vec<bool>,
    borrowed: Option<u32>,
    unselected: std::collections::VecDeque<u32>,
    free_handles: Vec<u32>,
    committed: usize,
    cache: FastMap<(u16, usize, u32), Option<super::ProbeHit>>,
    prior_recovery: std::sync::Arc<[(u16, u32, SubtreeId)]>,
    recovery_keys: RecoveryKeys,
    generated: Option<Generated<'a>>,
}
impl<'a> SharedProbe<'a> {
    pub(crate) fn new(
        language: &'static Language<'static>,
        text: &'a SourceText,
        limits: Limits,
        base: Option<&ParseSnapshot>,
        index: Index,
        seed: Option<&Seed<'a>>,
    ) -> Self {
        let mut probe = Self {
            parser: Parser::new(
                language,
                Input {
                    compiled: None,
                    lexers: None,
                    text,
                    contiguous: None,
                },
                limits,
                ParseOptions {
                    strict_comments: true,
                    ..Default::default()
                },
                limits.parser_stack_bytes,
                base,
                index,
            ),
            handles: Vec::new(),
            active: Vec::new(),
            borrowed: None,
            unselected: Default::default(),
            free_handles: Vec::new(),
            committed: 0,
            cache: Default::default(),
            prior_recovery: base
                .and_then(|b| b.recovery())
                .map_or_else(|| std::sync::Arc::from([]), |r| r.structures.clone()),
            recovery_keys: Default::default(),
            generated: None,
        };
        // Probe work is part of the existing recovery search budget. Keep its
        // recognizer unchanged until that accounting contract is versioned.
        probe.parser.optimize_recognition = false;
        if let Some(seed) = seed.filter(|_| base.is_none()) {
            probe.parser.adopt_seed(text, seed);
            probe.generated = seed.generated;
        }
        probe
    }
    pub(crate) fn rule(
        &mut self,
        id: u16,
        at: usize,
        power: u32,
    ) -> Result<Option<super::ProbeHit>, Diagnostic> {
        self.retire_borrowed();
        self.enforce_budget();
        if let Some(hit) = self.cache.get(&(id, at, power)).copied() {
            if let Some(hit) = hit {
                self.borrow_result(hit.root);
            }
            return Ok(hit);
        }
        let key = Key {
            rule: id,
            minimum: self.parser.key_minimum(id, power),
            in_token: 0,
        };
        self.prerecognize(id, at, power, key);
        self.parser.position = at;
        self.parser.furthest = at;
        self.parser.checkpoint_next = true;
        let mark = self.parser.mark();
        self.parser.failure_checkpoint = Some((at as u32, key));
        let result = self.parser.rule(id, power, 0);
        self.parser.failure_checkpoint = None;
        let hit = result.map(|root| {
            root.map(|root| {
                if root == NO_NODE {
                    return super::ProbeHit {
                        root: u32::MAX,
                        end: self.parser.position,
                        furthest: self.parser.furthest,
                    };
                }
                self.parser.arena.retain(root.node);
                let id = if let Some(id) = self.free_handles.pop() {
                    self.handles[id as usize] = root;
                    self.active[id as usize] = false;
                    id
                } else {
                    let id = self.handles.len() as u32;
                    self.handles.push(root);
                    self.active.push(false);
                    id
                };
                self.borrowed = Some(id);
                self.unselected.push_back(id);
                super::ProbeHit {
                    root: id,
                    end: self.parser.position,
                    furthest: self.parser.furthest,
                }
            })
        });
        self.parser.position = mark.position;
        self.parser.arena.restore(mark.trail);
        if let Ok(result) = hit {
            self.cache.insert((id, at, power), result);
        }
        self.enforce_budget();
        hit
    }
    /// Recognize a missing successful fact with the generated parser before
    /// the interpreted probe runs. The probe then reuses it as it reuses a
    /// seeded fact, so only that reuse is charged to the recovery budget.
    /// Failures leave no fact; the interpreted probe decides them as before.
    fn prerecognize(&mut self, id: u16, at: usize, power: u32, key: Key) {
        let Some(generated) = self.generated else {
            return;
        };
        let parser = &mut self.parser;
        if parser.language.rules[id as usize].lexical
            || parser.reusable.get(at as u32, key).is_some()
            || parser.pending_index.contains_key(&(at as u32, key))
        {
            return;
        }
        let saved = (parser.work, parser.max_depth, parser.furthest);
        parser.compiled = generated.compiled;
        parser.lexers = Some(generated.lexers);
        parser.optimize_recognition = true;
        parser.position = at;
        parser.furthest = at;
        parser.checkpoint_next = true;
        let mark = parser.mark();
        let _ = parser.rule(id, power, 0);
        parser.checkpoint_next = false;
        parser.position = mark.position;
        parser.arena.restore(mark.trail);
        parser.compiled = None;
        parser.lexers = None;
        parser.optimize_recognition = false;
        (parser.work, parser.max_depth, parser.furthest) = saved;
    }
    /// A predicate over a single syntax reference skips trivia, then calls the
    /// rule there. Pre-recognize that call; report whether a fact now exists.
    fn prerecognize_reference(&mut self, expr: &'static Expr<'static>, at: usize) -> bool {
        let Expr::Ref(id) = *expr else {
            return false;
        };
        if self.generated.is_none() || self.parser.language.rules[id as usize].lexical {
            return false;
        }
        let saved = (
            self.parser.work,
            self.parser.max_depth,
            self.parser.furthest,
        );
        self.parser.position = at;
        let skipped = self.parser.trivia();
        let start = self.parser.position;
        (
            self.parser.work,
            self.parser.max_depth,
            self.parser.furthest,
        ) = saved;
        if skipped.is_err() {
            return false;
        }
        let key = Key {
            rule: id,
            minimum: self.parser.key_minimum(id, 0),
            in_token: 0,
        };
        self.prerecognize(id, start, 0, key);
        self.parser.pending_index.contains_key(&(start as u32, key))
    }
    fn borrow_result(&mut self, id: u32) {
        if id == u32::MAX || self.active[id as usize] {
            return;
        }
        let node = self.handles[id as usize].node;
        self.parser.arena.retain(node);
        self.parser.arena.release_cache(node);
        self.borrowed = Some(id);
    }
    fn retire_borrowed(&mut self) {
        if let Some(id) = self.borrowed.take() {
            if !self.active[id as usize] {
                let node = self.handles[id as usize].node;
                self.parser.arena.retain_cache(node);
                self.parser.arena.release(node);
            }
        }
    }
    fn enforce_budget(&mut self) {
        let budget = self.parser.cache_budget;
        let limit = (budget as usize / 512).clamp(1, 16384);
        let clear = self.cache.len() > limit;
        let lookup = if self.cache.capacity() == 0 {
            0
        } else {
            (self.cache.capacity()
                * std::mem::size_of::<((u16, usize, u32), Option<super::ProbeHit>)>()
                * 2
                + 64) as u64
        };
        let reserved = lookup.saturating_add(
            (self.unselected.capacity() * 4 + self.free_handles.capacity() * 4) as u64,
        );
        let available = budget.saturating_sub(reserved);
        if !clear
            && self.parser.arena.cache_only_bytes() + self.parser.memo_storage_bound() <= available
        {
            return;
        }
        self.parser.stats.cache_evictions += 1;
        self.parser.clear_memo(0);
        self.parser.clear_memo(1);
        while clear || self.parser.arena.cache_only_bytes() > available {
            let Some(&id) = self.unselected.front() else {
                break;
            };
            if self.borrowed == Some(id) {
                break;
            }
            self.unselected.pop_front();
            if self.active[id as usize] || self.handles[id as usize] == NO_NODE {
                continue;
            }
            self.parser
                .arena
                .release_cache(self.handles[id as usize].node);
            self.handles[id as usize] = NO_NODE;
            self.free_handles.push(id);
        }
        let handles = &self.handles;
        if clear {
            self.cache.clear();
            self.cache.shrink_to(limit);
        } else {
            self.cache.retain(|_, hit| {
                hit.is_none_or(|h| h.root == u32::MAX || handles[h.root as usize] != NO_NODE)
            });
        }
        // Pending facts may own the last cached references after probe eviction.
        self.parser.cache_budget = available;
        self.parser.enforce_cache_budget();
        self.parser.cache_budget = budget;
    }
    pub(crate) fn reusable_rule(
        &mut self,
        id: u16,
        at: usize,
    ) -> Result<Option<super::ProbeHit>, Diagnostic> {
        let key = Key {
            rule: id,
            minimum: 0,
            in_token: 0,
        };
        let Some(hit) = self.parser.reusable.get(at as u32, key) else {
            return Ok(None);
        };
        if hit.root == SubtreeId::NONE || hit.end_offset == 0 {
            return Ok(None);
        }
        self.rule(id, at, 0)
    }
    pub(crate) fn predicate(
        &mut self,
        expr: &'static Expr<'static>,
        at: usize,
    ) -> Result<bool, Diagnostic> {
        self.retire_borrowed();
        self.enforce_budget();
        let reuse = self.prerecognize_reference(expr, at);
        self.parser.position = at;
        let mark = self.parser.mark();
        let mut out = Children::sink(at);
        // A pre-recognized reference is reused only by that one rule call.
        self.parser.checkpoint_next = reuse;
        let result = self.parser.expression(expr, false, None, 0, 0, &mut out);
        self.parser.checkpoint_next = false;
        self.parser.restore(mark);
        self.enforce_budget();
        result
    }
    /// A history event is active speculative/output syntax, like a live winner.
    /// Its owner must survive cache eviction until the VM discards that history.
    pub(crate) fn selected(&mut self, root: u32) {
        if root == u32::MAX || self.active[root as usize] {
            return;
        }
        let node = self.handles[root as usize].node;
        if self.borrowed != Some(root) {
            self.parser.arena.retain(node);
            self.parser.arena.release_cache(node);
        }
        self.active[root as usize] = true;
    }
    pub(crate) fn commit(&mut self, roots: &mut [u32], keep_from: usize) {
        let mut next = Vec::new();
        let mut mapped = HashMap::new();
        for root in roots {
            if *root == u32::MAX || (*root as usize) < self.committed {
                continue;
            }
            *root = *mapped.entry(*root).or_insert_with(|| {
                let handle = self.handles[*root as usize];
                self.parser.arena.retain(handle.node);
                let id = (self.committed + next.len()) as u32;
                next.push(handle);
                id
            });
        }
        for (offset, (handle, active)) in self
            .handles
            .drain(self.committed..)
            .zip(self.active.drain(self.committed..))
            .enumerate()
        {
            if active || self.borrowed == Some((self.committed + offset) as u32) {
                self.parser.arena.release(handle.node)
            } else {
                self.parser.arena.release_cache(handle.node)
            }
        }
        let committed = self.committed;
        self.cache.retain(|(_, at, _), hit| {
            if *at < keep_from {
                return false;
            }
            match hit {
                None => true,
                Some(hit) if hit.root == u32::MAX || (hit.root as usize) < committed => true,
                Some(hit) => {
                    if let Some(root) = mapped.get(&hit.root) {
                        hit.root = *root;
                        true
                    } else {
                        false
                    }
                }
            }
        });
        self.borrowed = None;
        self.unselected.clear();
        self.free_handles.clear();
        self.active.extend(std::iter::repeat_n(true, next.len()));
        self.handles.extend(next);
        self.committed = self.handles.len();
        self.enforce_budget();
    }
    pub(crate) fn work(&self) -> u64 {
        self.parser.work
    }
    pub(crate) fn handle(&self, id: u32) -> Option<Handle> {
        (id != u32::MAX).then(|| self.handles[id as usize])
    }
    pub(crate) fn label(&mut self, name: &'static str) -> Result<u16, Diagnostic> {
        self.parser.intern(name)
    }
    pub(crate) fn arena(&mut self) -> &mut ArenaBuilder {
        &mut self.parser.arena
    }
    pub(crate) fn recovery_children(
        &mut self,
        rule: u16,
        start: u32,
    ) -> Result<Children, Diagnostic> {
        let units = u64::from(self.prior_recovery.len().max(1).ilog2()) + 2;
        if units > self.parser.work_budget.saturating_sub(self.parser.work) {
            return Err(Diagnostic::new(
                "limit",
                [start, start],
                "recovery metadata work limit exceeded",
            ));
        }
        self.parser.work += units;
        self.parser.stats.recovery_metadata_work += units;
        let prior = self
            .parser
            .reusable
            .previous_position(start)
            .and_then(|at| {
                let i = self
                    .prior_recovery
                    .partition_point(|(id, old_at, _)| (*id, *old_at) < (rule, at));
                self.prior_recovery
                    .get(i)
                    .filter(|(id, old_at, _)| (*id, *old_at) == (rule, at))
                    .map(|(_, _, node)| *node)
            });
        if let Some((node, old_start, items)) = prior.and_then(|node| {
            let at = self.parser.reusable.previous_position(start)?;
            self.parser
                .arena
                .sequence(node)
                .map(|items| (node, at, items))
        }) {
            Ok(Children::new(
                start as usize,
                Some((node, old_start, items)),
            ))
        } else {
            self.parser.child_buffer(rule, start as usize)
        }
    }
    pub(crate) fn recovery_push(&mut self, children: &mut Children, label: u16, node: Handle) {
        let copied = children.push(
            Edge { label, node },
            self.parser
                .reusable
                .previous_position(node.start + node.width),
        );
        self.parser.stats.recovery_child_bytes += copied as u64;
    }
    pub(crate) fn recovery_pop(&mut self, children: &mut Children) -> Option<Handle> {
        let (child, end) = children.pop_last()?;
        let width = self.parser.arena.node(child.node)?.width;
        Some(Handle {
            node: child.node,
            start: end - width,
            width,
        })
    }
    pub(crate) fn recovery_root_children(&mut self, root: Handle) -> Children {
        let previous = self
            .parser
            .arena
            .sequence(root.node)
            .map(|s| (root.node, root.start, s));
        let mut children = Children::new(0, previous);
        let mut end = root.start;
        for child in self.parser.arena.children(root.node) {
            end += child.extent;
            let width = self.parser.arena.node(child.node).unwrap().width;
            let node = Handle {
                node: child.node,
                start: end - width,
                width,
            };
            self.parser.stats.recovery_child_bytes += children.push(
                Edge {
                    label: child.label,
                    node,
                },
                Some(end),
            ) as u64;
        }
        children
    }
    pub(crate) fn recovery_node(
        &mut self,
        rule: u16,
        start: u32,
        end: u32,
        children: &Children,
    ) -> Result<SubtreeId, Diagnostic> {
        let node = self.parser.arena.add_prepared(
            rule,
            end - start,
            children.iter(),
            children.prior_node(),
            children.prepare(),
        )?;
        self.recovery_keys.insert((rule, [start, end]));
        Ok(node)
    }
    pub(crate) fn finish(
        mut self,
        text: &SourceText,
        root: Handle,
        base: Option<&ParseSnapshot>,
        mode: SnapshotMode,
    ) -> Result<(ParseSnapshot, RecoveryKeys), Diagnostic> {
        self.parser.arena.retain(root.node);
        for (i, (handle, active)) in self.handles.into_iter().zip(self.active).enumerate() {
            if active || self.borrowed == Some(i as u32) {
                self.parser.arena.release(handle.node)
            } else {
                self.parser.arena.release_cache(handle.node)
            }
        }
        let selected: std::collections::HashSet<_> = self
            .recovery_keys
            .iter()
            .map(|(rule, span)| (*rule, span[0]))
            .collect();
        self.parser.pending_index.retain(|(at, key), hit| {
            hit.root != SubtreeId::NONE || selected.contains(&(key.rule, *at))
        });
        publish(self.parser, text, root, base, mode).map(|snapshot| (snapshot, self.recovery_keys))
    }
}
/// One trivia-skipped position and the token reads already made from it.
struct Peek {
    at: usize,
    character: Option<char>,
    after: [(u32, u32, Option<char>); 4],
    after_len: usize,
}
impl Peek {
    fn new(at: usize, character: Option<char>) -> Self {
        Self {
            at,
            character,
            after: [(0, 0, None); 4],
            after_len: 0,
        }
    }
    fn after(&self, end: usize) -> Option<(usize, Option<char>)> {
        self.after[..self.after_len]
            .iter()
            .find(|(known, _, _)| *known as usize == end)
            .map(|(_, next, character)| (*next as usize, *character))
    }
    fn remember_after(&mut self, end: usize, (next, character): (usize, Option<char>)) {
        if self.after_len < self.after.len() {
            self.after[self.after_len] = (end as u32, next as u32, character);
            self.after_len += 1;
        }
    }
}

/// Token reads shared by every prediction at a position: identifier word
/// extents and lexical rule ends, each with the observation it made. The
/// source never changes during a parse, so entries never go stale; slots are
/// direct-mapped by position and simply overwritten.
#[derive(Default)]
struct TokenCache {
    /// Allocated on first use: small edits often read few positions.
    slots: Vec<TokenSlot>,
}
#[derive(Clone, Copy)]
struct TokenSlot {
    at: u32,
    /// Word end and the observation that found it; `u32::MAX` when unknown.
    word: u32,
    word_observed: u32,
    /// End of trivia from here, its observation and logical steps;
    /// `u32::MAX` when unknown.
    trivia: u32,
    trivia_observed: u32,
    trivia_steps: u32,
    /// Lexical rule, end (`u32::MAX` for no match) and observation.
    lexical: [(u16, u32, u32); 3],
    lexical_len: u8,
}
/// Backtracking revisits recent positions; a short window covers it.
const TOKEN_SLOTS: usize = 512;
impl TokenSlot {
    const EMPTY: Self = Self {
        at: u32::MAX,
        word: u32::MAX,
        word_observed: 0,
        trivia: u32::MAX,
        trivia_observed: 0,
        trivia_steps: 0,
        lexical: [(0, 0, 0); 3],
        lexical_len: 0,
    };
}
impl TokenCache {
    fn get(&self, at: usize) -> Option<&TokenSlot> {
        self.slots
            .get(at % TOKEN_SLOTS)
            .filter(|slot| slot.at == at as u32)
    }
    fn slot(&mut self, at: usize) -> &mut TokenSlot {
        if self.slots.is_empty() {
            self.slots = vec![TokenSlot::EMPTY; TOKEN_SLOTS];
        }
        let slot = &mut self.slots[at % TOKEN_SLOTS];
        if slot.at != at as u32 {
            *slot = TokenSlot {
                at: at as u32,
                ..TokenSlot::EMPTY
            };
        }
        slot
    }
    fn trivia(&self, at: usize) -> Option<(usize, u64, u32)> {
        let slot = self.get(at).filter(|slot| slot.trivia != u32::MAX)?;
        Some((
            slot.trivia as usize,
            u64::from(slot.trivia_observed),
            slot.trivia_steps,
        ))
    }
    fn remember_trivia(&mut self, at: usize, end: usize, observed: u64, steps: u32) {
        let slot = self.slot(at);
        slot.trivia = end as u32;
        slot.trivia_observed = observed.min(u64::from(u32::MAX)) as u32;
        slot.trivia_steps = steps;
    }
    fn lexical(&mut self, at: usize, rule: u16) -> Option<(Option<usize>, u64)> {
        let slot = self.get(at)?;
        slot.lexical[..slot.lexical_len as usize]
            .iter()
            .find(|(known, _, _)| *known == rule)
            .map(|(_, end, observed)| {
                (
                    (*end != u32::MAX).then_some(*end as usize),
                    u64::from(*observed),
                )
            })
    }
    fn remember_lexical(&mut self, at: usize, rule: u16, end: Option<usize>, observed: u64) {
        let slot = self.slot(at);
        let entry = (
            rule,
            end.map_or(u32::MAX, |end| end as u32),
            observed.min(u64::from(u32::MAX)) as u32,
        );
        if (slot.lexical_len as usize) < slot.lexical.len() {
            slot.lexical[slot.lexical_len as usize] = entry;
            slot.lexical_len += 1;
        } else {
            slot.lexical.rotate_left(1);
            slot.lexical[2] = entry;
        }
    }
    /// The end of the identifier word at `at` (`at` when none), and the
    /// observation: through the first character after the word.
    fn word(&mut self, source: &str, at: usize) -> (usize, u64) {
        let slot = self.slot(at);
        if slot.word != u32::MAX {
            return (slot.word as usize, u64::from(slot.word_observed));
        }
        let mut end = at;
        let mut observed = None;
        for (offset, c) in source.get(at..).unwrap_or("").char_indices() {
            if !(c.is_alphanumeric() || c == '_') {
                observed = Some(at + offset + c.len_utf8());
                break;
            }
            end = at + offset + c.len_utf8();
        }
        let observed = observed.unwrap_or(source.len() + 1);
        slot.word = end as u32;
        slot.word_observed = observed as u32;
        (end, observed as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn choice_limits_precede_speculative_trivia_errors() {
        for (wrapped, frames, work) in [(false, 2, 7), (true, 3, 8)] {
            let template = prefix_fact_fixture(false);
            let parts: &'static [Expr] = Box::leak(
                ["a", "b", "c", "d"]
                    .into_iter()
                    .map(|text| {
                        if wrapped {
                            Expr::Label("value", Box::leak(Box::new(Expr::Literal(text))))
                        } else {
                            Expr::Literal(text)
                        }
                    })
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            );
            let root = Rule {
                name: "Root",
                lexical: false,
                expr: Expr::Sequence(Box::leak(
                    vec![Expr::Literal("@"), Expr::Choice(parts)].into_boxed_slice(),
                )),
                except: &[],
                precedence: 0,
                right_associative: false,
                transparent: false,
                alias: false,
                owner: None,
                action: Action::None,
            };
            let language = Box::leak(Box::new(Language {
                rules: Box::leak(vec![root].into_boxed_slice()),
                ..*template
            }));
            let options = ParseOptions {
                strict_comments: true,
                ..Default::default()
            };
            for limits in [
                Limits {
                    parser_frames: frames,
                    ..Default::default()
                },
                Limits {
                    work,
                    parser_work_per_byte: 0,
                    ..Default::default()
                },
            ] {
                let expected = parse_with_options(language, "@/*", limits, options)
                    .expect_err("flat limit diagnostic");
                let actual = parse_snapshot(language, "@/*", limits, SnapshotMode::Strict(options))
                    .err()
                    .expect("shared limit diagnostic");
                assert_eq!(
                    (actual.category, actual.span, actual.message),
                    (expected.category, expected.span, expected.message)
                );
            }
        }
    }
    #[test]
    fn batched_skips_preserve_single_step_budget_and_depth_checks() {
        let language = prefix_fact_fixture(false);
        let text = SourceText::new("a").unwrap();
        let make = || {
            Parser::new(
                language,
                Input {
                    compiled: None,
                    lexers: None,
                    text: &text,
                    contiguous: None,
                },
                Limits::default(),
                ParseOptions::default(),
                Limits::default().parser_stack_bytes,
                None,
                Index::default(),
            )
        };
        for (initial, count, depth) in [(0, 0, 0), (0, 3, 0), (4, 9, 0), (0, 9, 3)] {
            let mut batch = make();
            let mut reference = make();
            for parser in [&mut batch, &mut reference] {
                parser.work = initial;
                parser.work_budget = 5;
                parser.limits.parser_frames = 2;
            }
            let actual = batch.tick_steps(count, depth);
            let expected = (0..count).try_for_each(|_| reference.tick(depth));
            assert_eq!(
                actual.err().map(|d| (d.category, d.span, d.message)),
                expected.err().map(|d| (d.category, d.span, d.message))
            );
            assert_eq!(batch.work, reference.work);
            assert_eq!(batch.max_depth, reference.max_depth);
        }
    }
    #[test]
    fn choice_lookahead_stays_lazy_before_a_leading_predicate_error() {
        let template = prefix_fact_fixture(false);
        let root = Rule {
            name: "Root",
            lexical: false,
            expr: Expr::Sequence(&[
                Expr::Literal("@"),
                Expr::Choice(&[
                    Expr::Predicate {
                        positive: true,
                        inner: &Expr::Pratt {
                            prefixes: &[],
                            infixes: &[],
                            memoize: false,
                        },
                    },
                    Expr::Literal("x"),
                    Expr::Literal("y"),
                    Expr::Literal("z"),
                ]),
            ]),
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            action: Action::None,
        };
        let language = Box::leak(Box::new(Language {
            rules: Box::leak(vec![root].into_boxed_slice()),
            ..*template
        }));
        let options = ParseOptions {
            strict_comments: true,
            ..Default::default()
        };
        let expected = parse_with_options(language, "@/*", Limits::default(), options)
            .expect_err("leading predicate grammar error");
        let actual = parse_snapshot(
            language,
            "@/*",
            Limits::default(),
            SnapshotMode::Strict(options),
        )
        .err()
        .expect("leading predicate must run before trivia lookahead");
        assert_eq!(
            (actual.category, actual.span, actual.message),
            (expected.category, expected.span, expected.message)
        );
    }
    #[test]
    fn choice_cache_reclaims_buckets_and_keeps_active_plans_valid() {
        let language = prefix_fact_fixture(false);
        let text = SourceText::new("a").unwrap();
        let mut parser = Parser::new(
            language,
            Input {
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: None,
            },
            Limits::default(),
            ParseOptions::default(),
            Limits::default().parser_stack_bytes,
            None,
            Index::default(),
        );
        let parts: &'static [Expr] = Box::leak(
            vec![
                Expr::Literal("a"),
                Expr::Literal("b"),
                Expr::Literal("c"),
                Expr::Literal("d"),
            ]
            .into_boxed_slice(),
        );
        let mut held = None;
        for index in 0..900 {
            let expr = Box::leak(Box::new(Expr::Choice(parts)));
            let plan = parser.choice_plan(expr, parts, true).unwrap();
            if index == 0 {
                held = Some(plan);
            }
            assert!(parser.choice_cache_bytes <= 256 * 1024);
        }
        parser.cache_budget = 8192;
        let expr = Box::leak(Box::new(Expr::Choice(parts)));
        parser.choice_plan(expr, parts, true);
        assert!(parser.choice_cache_bytes <= 1024);
        assert!(
            parser.choice_cache.capacity() * std::mem::size_of::<usize>() <= 1024,
            "old bucket capacity escaped the smaller budget"
        );
        assert_eq!(
            held.unwrap().candidates(Some('a'), 0).collect::<Vec<_>>(),
            vec![0]
        );
        assert!(parser.rule(0, 0, 0).unwrap().is_some());
        assert_eq!(parser.position, 1);
    }
    #[test]
    fn ordinary_choices_skip_impossible_rule_bodies() {
        let template = prefix_fact_fixture(false);
        let make = |name, expr| Rule {
            name,
            expr,
            owner: None,
            lexical: false,
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            action: Action::None,
        };
        let mut rules = vec![make(
            "Root",
            Expr::Choice(Box::leak(
                (1..=65)
                    .map(Expr::Ref)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            )),
        )];
        for index in 0..65 {
            rules.push(make(
                "Candidate",
                Expr::Literal(if index == 64 { "z" } else { "x" }),
            ));
        }
        let language = Box::leak(Box::new(Language {
            rules: Box::leak(rules.into_boxed_slice()),
            ..*template
        }));
        let snapshot = parse_snapshot(
            language,
            "z",
            Limits::default(),
            SnapshotMode::Strict(ParseOptions::default()),
        )
        .unwrap();
        let expected = parse(language, "z", Limits::default()).unwrap();
        let exported = snapshot.materialize().unwrap();
        assert_eq!(
            exported
                .nodes
                .iter()
                .map(|n| (n.rule, n.span))
                .collect::<Vec<_>>(),
            expected
                .nodes
                .iter()
                .map(|n| (n.rule, n.span))
                .collect::<Vec<_>>()
        );
        assert!(
            snapshot.stats().work < 100,
            "ordinary choice consumed {} steps",
            snapshot.stats().work
        );
    }
    #[test]
    fn predictive_dispatch_keeps_fatal_leading_grammar_checks() {
        for expr in [
            Expr::Sequence(&[
                Expr::Repeat {
                    inner: &Expr::Literal(""),
                    min: 0,
                    max: 2,
                },
                Expr::Literal("x"),
            ]),
            Expr::Sequence(&[
                Expr::Pratt {
                    prefixes: &[3],
                    infixes: &[],
                    memoize: false,
                },
                Expr::Literal("x"),
            ]),
        ] {
            let template = prefix_fact_fixture(false);
            let make = |name, expr, owner| Rule {
                name,
                expr,
                owner,
                lexical: false,
                except: &[],
                precedence: 0,
                right_associative: false,
                transparent: false,
                alias: false,
                action: Action::None,
            };
            let rules = vec![
                make(
                    "Root",
                    Expr::Pratt {
                        prefixes: &[1, 2],
                        infixes: &[],
                        memoize: true,
                    },
                    None,
                ),
                make("Bad", expr, None),
                make("Good", Expr::Literal("y"), Some(0)),
                make("X", Expr::Literal("x"), None),
            ];
            let language = Box::leak(Box::new(Language {
                rules: Box::leak(rules.into_boxed_slice()),
                ..*template
            }));
            let expected = parse(language, "y", Limits::default())
                .expect_err("flat must reject malformed grammar");
            let actual = parse_snapshot(
                language,
                "y",
                Limits::default(),
                SnapshotMode::Strict(ParseOptions::default()),
            )
            .err()
            .expect("prediction must preserve fatal grammar checks");
            assert_eq!(
                (actual.category, actual.span, actual.message),
                (expected.category, expected.span, expected.message)
            );
        }
    }
    #[test]
    fn predictive_dispatch_keeps_infix_precedence_overflow() {
        let template = prefix_fact_fixture(false);
        let make = |name, expr, owner, precedence| Rule {
            name,
            expr,
            owner,
            precedence,
            lexical: false,
            except: &[],
            right_associative: false,
            transparent: false,
            alias: false,
            action: Action::None,
        };
        let rules = vec![
            make(
                "Root",
                Expr::Pratt {
                    prefixes: &[1],
                    infixes: &[2],
                    memoize: true,
                },
                None,
                0,
            ),
            make("X", Expr::Literal("x"), Some(0), 0),
            make(
                "Overflow",
                Expr::Sequence(&[Expr::Ref(0), Expr::Literal("+"), Expr::Ref(0)]),
                Some(0),
                u32::MAX,
            ),
        ];
        let language = Box::leak(Box::new(Language {
            rules: Box::leak(rules.into_boxed_slice()),
            ..*template
        }));
        let expected =
            parse(language, "x", Limits::default()).expect_err("flat precedence overflow");
        let actual = parse_snapshot(
            language,
            "x",
            Limits::default(),
            SnapshotMode::Strict(ParseOptions::default()),
        )
        .err()
        .expect("prediction must preserve overflow");
        assert_eq!(
            (actual.category, actual.span, actual.message),
            (expected.category, expected.span, expected.message)
        );
    }
    #[test]
    fn productive_leading_recursion_keeps_the_flat_limit_diagnostic() {
        let template = prefix_fact_fixture(false);
        let make = |name, expr, owner| Rule {
            name,
            expr,
            owner,
            lexical: false,
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            action: Action::None,
        };
        let rules = vec![
            make(
                "Root",
                Expr::Pratt {
                    prefixes: &[1, 2],
                    infixes: &[],
                    memoize: true,
                },
                None,
            ),
            make(
                "Recursive",
                Expr::Choice(&[Expr::Ref(1), Expr::Literal("x")]),
                None,
            ),
            make("Other", Expr::Literal("y"), Some(0)),
        ];
        let language = Box::leak(Box::new(Language {
            rules: Box::leak(rules.into_boxed_slice()),
            ..*template
        }));
        let limits = Limits {
            parser_frames: 32,
            ..Default::default()
        };
        let expected = parse(language, "y", limits).expect_err("flat must report recursive limit");
        let actual = parse_snapshot(
            language,
            "y",
            limits,
            SnapshotMode::Strict(ParseOptions::default()),
        )
        .err()
        .expect("predictive filtering must not bypass leading recursion");
        assert_eq!(
            (actual.code, actual.span, actual.message),
            (expected.code, expected.span, expected.message)
        );
    }
    #[test]
    fn lexical_results_are_reused_across_repeated_lookahead() {
        let template = prefix_fact_fixture(false);
        let mut parts = (0..16)
            .map(|_| Expr::Predicate {
                positive: true,
                inner: &Expr::Ref(1),
            })
            .collect::<Vec<_>>();
        parts.push(Expr::Ref(1));
        let rules = vec![
            Rule {
                name: "Root",
                lexical: false,
                expr: Expr::Sequence(Box::leak(parts.into_boxed_slice())),
                except: &[],
                precedence: 0,
                right_associative: false,
                transparent: false,
                alias: false,
                owner: None,
                action: Action::None,
            },
            Rule {
                name: "Token",
                lexical: true,
                expr: Expr::Repeat {
                    inner: &Expr::Class(&[('a', 'z')]),
                    min: 1,
                    max: u32::MAX,
                },
                except: &[],
                precedence: 0,
                right_associative: false,
                transparent: false,
                alias: false,
                owner: None,
                action: Action::None,
            },
        ];
        let language = Box::leak(Box::new(Language {
            rules: Box::leak(rules.into_boxed_slice()),
            ..*template
        }));
        let source = "abcdefghijklmnopqrstuvwxyz";
        let snapshot = parse_snapshot(
            language,
            source,
            Limits::default(),
            SnapshotMode::Strict(ParseOptions::default()),
        )
        .unwrap();
        let flat = parse(language, source, Limits::default()).unwrap();
        let exported = snapshot.materialize().unwrap();
        assert_eq!(
            exported
                .nodes
                .iter()
                .map(|n| (n.rule, n.span))
                .collect::<Vec<_>>(),
            flat.nodes
                .iter()
                .map(|n| (n.rule, n.span))
                .collect::<Vec<_>>()
        );
        assert!(
            snapshot.stats().work < 200,
            "lexical lookahead consumed {} work steps",
            snapshot.stats().work
        );
    }
    #[test]
    fn predictive_starts_bound_work_without_changing_selected_tree() {
        let template = prefix_fact_fixture(false);
        let mut rules = vec![Rule {
            name: "Root",
            lexical: false,
            expr: Expr::Pratt {
                prefixes: Box::leak((1..=65).collect::<Vec<u16>>().into_boxed_slice()),
                infixes: &[],
                memoize: true,
            },
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            action: Action::None,
        }];
        for index in 0..65 {
            rules.push(Rule {
                name: "Candidate",
                lexical: false,
                expr: Expr::Literal(if index == 64 { "z" } else { "x" }),
                except: &[],
                precedence: 0,
                right_associative: false,
                transparent: false,
                alias: false,
                owner: Some(0),
                action: Action::None,
            });
        }
        let language = Box::leak(Box::new(Language {
            rules: Box::leak(rules.into_boxed_slice()),
            ..*template
        }));
        let snapshot = parse_snapshot(
            language,
            "z",
            Limits::default(),
            SnapshotMode::Strict(ParseOptions::default()),
        )
        .unwrap();
        let flat = parse(language, "z", Limits::default()).unwrap();
        let exported = snapshot.materialize().unwrap();
        assert_eq!(
            exported
                .nodes
                .iter()
                .map(|n| (n.rule, n.span))
                .collect::<Vec<_>>(),
            flat.nodes
                .iter()
                .map(|n| (n.rule, n.span))
                .collect::<Vec<_>>()
        );
        assert!(
            snapshot.stats().work < 100,
            "impossible candidates consumed {} work steps",
            snapshot.stats().work
        );
    }
    fn prefix_fact_fixture(sensitive: bool) -> &'static Language<'static> {
        let make = |name, expr, owner, precedence| Rule {
            name,
            expr,
            lexical: false,
            owner,
            precedence,
            except: &[],
            right_associative: false,
            transparent: false,
            alias: false,
            action: Action::None,
        };
        let rules = if sensitive {
            vec![
                make(
                    "Root",
                    Expr::Pratt {
                        prefixes: &[1, 2],
                        infixes: &[3],
                        memoize: true,
                    },
                    None,
                    0,
                ),
                make(
                    "Minus",
                    Expr::Sequence(&[Expr::Literal("-"), Expr::Label("Right", &Expr::Ref(0))]),
                    Some(0),
                    10,
                ),
                make("Number", Expr::Class(&[('0', '9')]), Some(0), 100),
                make(
                    "Add",
                    Expr::Sequence(&[
                        Expr::Label("Left", &Expr::Ref(0)),
                        Expr::Literal("+"),
                        Expr::Label("Right", &Expr::Ref(0)),
                    ]),
                    Some(0),
                    1,
                ),
            ]
        } else {
            vec![
                make(
                    "Root",
                    Expr::Pratt {
                        prefixes: &[1],
                        infixes: &[],
                        memoize: true,
                    },
                    None,
                    0,
                ),
                make("Value", Expr::Literal("a"), Some(0), 0),
            ]
        };
        Box::leak(Box::new(Language {
            profile: Profile::SyntaxOnly,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "prefix-fact",
            rules: Box::leak(rules.into_boxed_slice()),
        }))
    }
    fn prefix_snapshot(language: &'static Language<'static>, text: &SourceText) -> ParseSnapshot {
        let mut parser = Parser::new(
            language,
            Input {
                compiled: None,
                lexers: None,
                text,
                contiguous: None,
            },
            Limits::default(),
            ParseOptions::default(),
            Limits::default().parser_stack_bytes,
            None,
            Index::default(),
        );
        let root = parser.rule(1, 0, 0).unwrap().unwrap();
        assert_eq!(parser.position, text.len() as usize);
        publish(
            parser,
            text,
            root,
            None,
            SnapshotMode::Strict(ParseOptions::default()),
        )
        .unwrap()
    }
    #[test]
    fn cache_pressure_preserves_facts_for_history_owned_roots() {
        let language = prefix_fact_fixture(false);
        let text = SourceText::new(&"a ".repeat(5000)).unwrap();
        let mut probe = SharedProbe::new(
            language,
            &text,
            Limits::default(),
            None,
            Index::default(),
            None,
        );
        probe.parser.cache_budget = 1024;
        let first = probe.rule(1, 0, 0).unwrap().unwrap();
        let root = probe.handle(first.root).unwrap();
        probe.selected(first.root);
        for at in 1..5000 {
            assert!(probe.rule(1, at * 2, 0).unwrap().is_some());
        }
        let replay = probe.rule(1, 0, 0).unwrap().unwrap();
        assert_eq!(
            probe.handle(replay.root).unwrap().node,
            root.node,
            "cache pressure discarded a fact whose history-owned root could not release graph storage"
        );
        assert!(probe.parser.arena.cache_only_bytes() <= probe.parser.cache_budget);
    }
    #[test]
    fn warm_pratt_reuses_a_validated_power_insensitive_prefix_fact() {
        let language = prefix_fact_fixture(false);
        let text = SourceText::new("a").unwrap();
        let base = prefix_snapshot(language, &text);
        let next = parse_source(
            language,
            Input {
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: None,
            },
            Limits::default(),
            ParseOptions::default(),
            Limits::default().parser_stack_bytes,
            Some(&base),
            base.recognitions.clone(),
            None,
        )
        .unwrap();
        assert_eq!(
            next.root().subtree_id(),
            base.root().subtree_id(),
            "validated prefix was reparsed because its caller was not a recording checkpoint"
        );
        assert!(next.stats().reused_subtrees > 0);
    }
    #[test]
    fn warm_pratt_does_not_reuse_a_prefix_at_a_different_sensitive_power() {
        let language = prefix_fact_fixture(true);
        let text = SourceText::new("-1+2").unwrap();
        let base = prefix_snapshot(language, &text);
        assert_eq!(base.root().rule(), 1);
        let next = parse_source(
            language,
            Input {
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: None,
            },
            Limits::default(),
            ParseOptions::default(),
            Limits::default().parser_stack_bytes,
            Some(&base),
            base.recognitions.clone(),
            None,
        )
        .unwrap();
        assert_eq!(next.root().rule(), 3);
        assert_eq!(next.root().children().next().unwrap().1.text(), "-1");
    }
    #[test]
    fn construction_hint_cache_does_not_retain_every_source_position() {
        let language = Box::leak(Box::new(Language {
            profile: Profile::SyntaxOnly,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "hint-cache",
            rules: &[],
        }));
        let count = 50_000;
        let text = SourceText::new(&"a".repeat(count)).unwrap();
        let mut builder = ArenaBuilder::new(vec![""].into());
        let leaf = builder.add(1, 1, vec![]).unwrap();
        let root = builder
            .add(
                0,
                count as u32,
                vec![
                    crate::syntax::arena::Child {
                        node: leaf,
                        extent: 1,
                        label: 0
                    };
                    count
                ],
            )
            .unwrap();
        let mode = SnapshotMode::Strict(ParseOptions::default());
        let base =
            ParseSnapshot::from_parts(language, text.clone(), builder.finish(), root, 0, mode);
        let mut parser = Parser::new(
            language,
            Input {
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: None,
            },
            Limits::default(),
            ParseOptions::default(),
            Limits::default().parser_stack_bytes,
            Some(&base),
            Index::default(),
        );
        for at in 0..count as u32 {
            let hint = parser.find_prior(1, at).unwrap().unwrap();
            assert_eq!((hint.node, hint.start, hint.old_start), (leaf, at, at));
        }
        assert!(
            parser.prior_paths.len() <= 4096,
            "retained {} construction paths",
            parser.prior_paths.len()
        );
        assert_eq!(base.root().subtree_id(), root);
    }
    #[test]
    fn warm_maintenance_uses_the_approved_retention_allowance() {
        let language = Box::leak(Box::new(Language {
            profile: Profile::SyntaxOnly,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "maintenance-slack",
            rules: &[],
        }));
        let count = 200_000;
        let text = SourceText::new(&"a".repeat(count)).unwrap();
        let mut builder = ArenaBuilder::new(vec![""].into());
        let leaf = builder.add(1, 1, vec![]).unwrap();
        let root = builder
            .add(
                0,
                count as u32,
                vec![
                    crate::syntax::arena::Child {
                        node: leaf,
                        extent: 1,
                        label: 0
                    };
                    count
                ],
            )
            .unwrap();
        let mode = SnapshotMode::Strict(ParseOptions::default());
        let mut base =
            ParseSnapshot::from_parts(language, text.clone(), builder.finish(), root, 0, mode);
        // Model previously accumulated publication churn, without allocating
        // disposable history merely to test the retention trigger.
        base.maintenance_debt = 8 * 1024 * 1024 + 1;
        let parser = Parser::new(
            language,
            Input {
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: None,
            },
            Limits::default(),
            ParseOptions::default(),
            Limits::default().parser_stack_bytes,
            Some(&base),
            Index::default(),
        );
        let next = publish(
            parser,
            &text,
            Handle {
                node: root,
                start: 0,
                width: count as u32,
            },
            Some(&base),
            mode,
        )
        .unwrap();
        assert_eq!(
            next.stats().maintenance_passes,
            0,
            "full graph copied before the approved retention allowance was used"
        );
        assert_eq!(next.stats().syntax_bytes_copied, 0);
        assert_eq!(next.root().subtree_id(), base.root().subtree_id());
        base.maintenance_debt = 8 * 1024 * 1024 + base.arena.payload_bytes() / 2 + 1;
        let parser = Parser::new(
            language,
            Input {
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: None,
            },
            Limits::default(),
            ParseOptions::default(),
            Limits::default().parser_stack_bytes,
            Some(&base),
            Index::default(),
        );
        let compacted = publish(
            parser,
            &text,
            Handle {
                node: root,
                start: 0,
                width: count as u32,
            },
            Some(&base),
            mode,
        )
        .unwrap();
        assert_eq!(compacted.stats().maintenance_passes, 1);
        assert!(compacted.stats().syntax_bytes_copied > 0);
        assert_eq!(
            base.root().children().nth(count - 1).unwrap().1.span(),
            [count as u32 - 1, count as u32]
        );
    }
    #[test]
    fn unselected_recovery_probe_cache_is_bounded() {
        let make = |name, expr| Rule {
            name,
            expr,
            lexical: false,
            precedence: 0,
            owner: None,
            except: &[],
            right_associative: false,
            transparent: false,
            alias: false,
            action: Action::None,
        };
        let language = Box::leak(Box::new(Language {
            profile: Profile::SyntaxOnly,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "unselected-recovery",
            rules: Box::leak(
                vec![
                    make(
                        "Root",
                        Expr::Repeat {
                            inner: &Expr::Ref(1),
                            min: 1,
                            max: u32::MAX,
                        },
                    ),
                    make("Item", Expr::Literal(";")),
                ]
                .into_boxed_slice(),
            ),
        }));
        let text = SourceText::new(&";".repeat(4000)).unwrap();
        let mut probe = SharedProbe::new(
            language,
            &text,
            Limits::default(),
            None,
            Index::default(),
            None,
        );
        probe.parser.cache_budget = 1024;
        for at in 0..4000 {
            assert!(probe.rule(1, at, 0).unwrap().is_some());
        }
        assert!(probe.parser.arena.cache_only_bytes() + probe.parser.memo_storage_bound() <= 1024);
        assert!(probe.parser.stats.cache_evictions > 0);
    }
    #[test]
    fn recovery_history_roots_are_active_syntax_not_disposable_cache() {
        let make = |name, expr| Rule {
            name,
            expr,
            lexical: false,
            precedence: 0,
            owner: None,
            except: &[],
            right_associative: false,
            transparent: false,
            alias: false,
            action: Action::None,
        };
        let language = Box::leak(Box::new(Language {
            profile: Profile::SyntaxOnly,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "active-recovery",
            rules: Box::leak(
                vec![
                    make(
                        "Root",
                        Expr::Repeat {
                            inner: &Expr::Ref(1),
                            min: 1,
                            max: u32::MAX,
                        },
                    ),
                    make("Item", Expr::Literal(";")),
                ]
                .into_boxed_slice(),
            ),
        }));
        let text = SourceText::new(&";".repeat(4000)).unwrap();
        let mut probe = SharedProbe::new(
            language,
            &text,
            Limits::default(),
            None,
            Index::default(),
            None,
        );
        probe.parser.cache_budget = 1024;
        for at in 0..4000 {
            let hit = probe.rule(1, at, 0).unwrap().unwrap();
            probe.selected(hit.root);
        }
        assert_eq!(probe.handles.len(), 4000);
        assert!(
            probe
                .handles
                .iter()
                .all(|h| probe.parser.arena.node(h.node).is_some())
        );
        assert_eq!(probe.parser.arena.cache_only_bytes(), 0);
    }
    #[test]
    fn cache_pressure_reclaims_failed_alternative_roots() {
        let make = |name, expr| Rule {
            name,
            expr,
            lexical: false,
            precedence: 0,
            owner: None,
            except: &[],
            right_associative: false,
            transparent: false,
            alias: false,
            action: Action::None,
        };
        let language = Box::leak(Box::new(Language {
            profile: Profile::SyntaxOnly,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "pressure",
            rules: Box::leak(
                vec![
                    make(
                        "Root",
                        Expr::Choice(&[
                            Expr::Sequence(&[Expr::Ref(1), Expr::Literal("!")]),
                            Expr::Ref(1),
                        ]),
                    ),
                    make(
                        "List",
                        Expr::Repeat {
                            inner: &Expr::Ref(2),
                            min: 1,
                            max: u32::MAX,
                        },
                    ),
                    make("Item", Expr::Literal(";")),
                ]
                .into_boxed_slice(),
            ),
        }));
        let text = SourceText::new(&";".repeat(4000)).unwrap();
        let mut parser = Parser::new(
            language,
            Input {
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: None,
            },
            Limits::default(),
            ParseOptions::default(),
            usize::MAX,
            None,
            Index::default(),
        );
        parser.cache_budget = 1024;
        assert!(parser.rule(0, 0, 0).unwrap().is_some());
        assert_eq!(parser.position, 4000);
        assert!(parser.stats.cache_evictions > 0);
        assert!(parser.arena.cache_only_bytes() + parser.memo_storage_bound() <= 1024);
    }
    #[test]
    fn wide_snapshot_edit_shares_sequence_blocks() {
        let make = |name, expr, lexical| Rule {
            name,
            expr,
            lexical,
            precedence: 0,
            owner: None,
            except: &[],
            right_associative: false,
            transparent: false,
            alias: false,
            action: Action::None,
        };
        let language = Box::leak(Box::new(Language {
            profile: Profile::SyntaxOnly,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "wide",
            rules: Box::leak(
                vec![
                    make(
                        "Root",
                        Expr::Repeat {
                            inner: &Expr::Ref(1),
                            min: 1,
                            max: u32::MAX,
                        },
                        false,
                    ),
                    make(
                        "Item",
                        Expr::Sequence(&[Expr::Ref(2), Expr::Literal(";")]),
                        false,
                    ),
                    make(
                        "Number",
                        Expr::Repeat {
                            inner: &Expr::Class(&[('0', '9')]),
                            min: 16,
                            max: 16,
                        },
                        true,
                    ),
                ]
                .into_boxed_slice(),
            ),
        }));
        let source = "1111111111111111;".repeat(1000);
        let old = parse_snapshot(
            language,
            &source,
            Limits::default(),
            SnapshotMode::Strict(ParseOptions::default()),
        )
        .unwrap();
        let at = 500 * 17 + 5;
        let edited = old
            .edit(
                &TextEdit {
                    range: [at, at + 1],
                    replacement: "2".into(),
                },
                Limits::default(),
            )
            .unwrap();
        assert!(
            old.arena
                .sequence(old.root)
                .unwrap()
                .shared_leaves(&edited.arena.sequence(edited.root).unwrap())
                > 25
        );
        assert!(edited.stats.reused_subtrees >= 998);
    }
}
