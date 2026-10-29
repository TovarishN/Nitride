# Shared Subtrees Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans for inline execution, or superpowers:subagent-driven-development if the user chooses delegation. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Reuse immutable subtree storage between parser alternatives and edited snapshots, with measured latency, memory bounds and full-reparse equivalence.

**Architecture:** Chunked immutable syntax storage, persistent child sequences and source text, and snapshot-relative occurrence views. Memo/winner/probe results retain handles. Dependency-indexed incremental recognition invalidates uncertain regions conservatively, including recovery; flat arrays become an explicit compatibility export.

**Tech Stack:** Existing Rust 1.85+/edition 2024 workspace and std; Python 3 evidence scripts; current development-only Nitrogen/Roslyn adapters. No third-party core dependencies.

**Spec:** `docs/superpowers/specs/2026-10-25-shared-subtrees-design.md` (approved 2026-10-25), extending milestone C in `docs/superpowers/specs/2026-10-24-csharp-recovery-incremental-design.md`.

## Global Constraints

- Preserve the admitted grammar boundaries and existing observable parsing behavior.
- Keep previous snapshots valid for existing readers.
- Remain on ordinary stable Rust/std targets with no third-party native-core dependency.
- Physical node numbers/storage order may change; compare normalized occurrences, not allocator identities.
- No automatic whole-document flattening on the incremental entry point.
- LSP/editor integration and incremental semantic analysis remain separate work.
- Limits return structured errors; disclose warm/cold resource-exhaustion differences.
- Changes stay local and reviewable. Do not stage, commit, push or merge without further authorization. At execution, inspect attachments/status and use the worktree skill for isolation; preserve the approved design and plan in that checkout.
- Record RED/GREEN commands, outcomes and rulings in `docs/stage15-progress.md`; final evidence goes in `docs/stage15-evidence.md`. Earlier evidence files are immutable baselines.

## Review Focus

1. Appending at EOF or changing a predicate outside the consumed span must invalidate results that observed that boundary (Task 5).
2. Labels and alias/token-child modes must remain correct when independently built chunks meet (Tasks 2–4).
3. An insertion into an early sibling must neither shift stored spans of all later siblings nor copy a wide parent's entire child list (Tasks 2, 3, 5).
4. Retained old snapshots, sparse chunks, cache eviction and destruction must not cause dangling references, chunk cycles or memory growth with edit count (Tasks 2, 5, 8).
5. Edits near missing tokens or enclosing scopes must preserve full-reparse repairs and recomputed semantic results, rather than treating shared syntax as shared bindings (Tasks 6, 7).

## File and interface map

Keep `syntax.rs` and its public flat `Node`, `Edge`, `SyntaxTree` contract. Add
`syntax/shared.rs` (snapshot and occurrence access), `syntax/arena.rs` (chunk
ownership/handles), `syntax/sequence.rs` (persistent placement sequences), and
`syntax/text.rs` (persistent UTF-8 pieces). Export the new public API from
`lib.rs`. Add `incremental/mod.rs`, `incremental/dependencies.rs`, and
`incremental/checkpoints.rs` for edit orchestration, recognition dependencies
and balanced reuse indexes. These are one feature, delivered sequentially.

Modify `parser.rs` for shared construction, recognition tracking, text access,
and handle-based memo/winner/probe results. Modify `recovery/search.rs` and
`recovery/tree.rs` for shared probes/output/sidecars. Add
`recovery/incremental.rs` for conservative repair invalidation. Modify
`semantic.rs` and `templates.rs` behind `syntax/access.rs` for occurrence-based
consumption. Do not reorganize unrelated grammar/execution/codegen code.

Add integration test targets `shared_syntax`, `incremental`, and
`incremental_recovery` in core; `incremental` in CLI; module unit tests exercise
private arena/sequence/dependency contracts. Extend the existing CLI profiler
and add `scripts/shared_subtrees.py` plus `scripts/test_shared_subtrees.py`.

Public API names and signatures to keep consistent across tasks:

```rust
pub struct TextEdit { pub range: [u32; 2], pub replacement: String }
pub enum SnapshotMode { Strict(ParseOptions), Recovering(RecoveryOptions) }
pub struct ParseSnapshot { /* opaque, cloneable, Send + Sync */ }
pub struct SubtreeId { /* opaque stable identity, Eq + Hash + Copy */ }
pub struct NodeView<'a> { /* structural handle + snapshot placement */ }
pub struct SnapshotStats { /* cumulative operation counters, see Task 1 */ }
pub struct StorageStats { /* reachable/unique/pinned bytes and chunks */ }

pub fn parse_snapshot(
    language: &'static Language, source: &str, limits: Limits,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic>;

impl ParseSnapshot {
    pub fn edit(&self, edit: &TextEdit, limits: Limits)
        -> Result<ParseSnapshot, Diagnostic>;
    pub fn root(&self) -> NodeView<'_>;
    pub fn diagnostics(&self) -> &[Diagnostic];
    pub fn stats(&self) -> &SnapshotStats;
    pub fn storage_stats(&self) -> StorageStats;
    pub fn materialize(&self) -> Result<SyntaxTree, Diagnostic>;
}
impl<'a> NodeView<'a> {
    pub fn subtree_id(&self) -> SubtreeId;
    pub fn rule(&self) -> u16;
    pub fn span(&self) -> [u32; 2];
    pub fn text(&self) -> std::borrow::Cow<'a, str>;
    pub fn children(&self)
        -> impl Iterator<Item = (&'static str, NodeView<'a>)> + 'a;
}
```

`edit` uses coordinates in `self`, and returns a new snapshot. Wrong order,
out-of-range endpoints, non-UTF-8 boundaries and >u32 source lengths yield
`Diagnostic` category `edit` or `limit`, never panic. Language, mode and template
context are immutable per snapshot; changed contexts require a fresh parse.
Strict syntax errors return `Err`. Recovering mode returns a snapshot carrying
syntax/limit diagnostics, preserving the existing recovery fallback contract;
invalid edit requests still return `Err`. Recovery diagnostics may be assembled
on explicit request/materialization; do not force their flattening during an
edit. A lazy cached `diagnostics()` slice is acceptable and its cost is reported.

## Proposed budgets and admission gates

These are proposed plan choices, not measured results. Capture a fresh baseline
before parser changes, with executable/source hashes and fixed workload seeds.
Use same-machine Release comparisons; report allocator-instrumented timings
separately from uninstrumented latency. Never substitute historical Stage 13/14
figures for that baseline.

- Start with 64 KiB target payload chunks and a balanced persistent sequence
  fanout of 32. Oversized records use explicit bounded dedicated storage.
- Budget speculative syntax/memo retention at 8 MiB above live published
  output for the measured workloads, with allowance of two construction chunks.
  Budget pressure evicts cache facts and reclaims only unowned records/chunks;
  if a live necessary intermediate exceeds the cap, report/handle a structured
  limit rather than corrupting results or silently exceeding the advertised cap.
  Count source and dependency-index storage separately so this cap cannot hide it.
- Cold strict/recovery median elapsed time <=1.25x baseline; requested peak
  <=1.50x; retained latest snapshot including metadata <=1.35x the equivalent
  fresh baseline tree. Report shared and legacy-export paths separately.
- For bounded method-local edits in strict-valid and locally recoverable CSharp4,
  p95 end-to-end edit latency <=50/100/200 ms for 256 KiB/4 MiB/16 MiB inputs,
  and <=20% of same-source full-reparse median. At least 100 timed edits per
  size/location after 10 warmups; p50/p95/max and fallback rates all published.
  Dense/junk/truncated cases have full equivalence and bounded-work gates;
  full-file fallback latency remains explicitly visible, not claimed interactive.
- A one-character edit inside a fixed-size method in a wide file copies <=256
  KiB of existing source/syntax payload independent of total file size; separately
  report new ancestor/sequence/index allocation and worst-case fallback.
- Memo hits copy/rebase zero descendant nodes or edges. Winner/probe selection
  also copies zero descendants. Explicit export copies are a separate counter.
- After 10,000 bounded-size edits, dropping old readers and clearing optional
  caches leaves <=1.50x a fresh shared snapshot's live bytes +8 MiB+two chunks.
  Retain windows of 1 and 8 snapshots; release-all returns tracked dynamic
  storage to zero. Deliberately retained complete history is measured separately.
- Any missed numeric gate blocks a performance-completion claim; publish it,
  diagnose it, and obtain approval for a changed target rather than rewriting
  the gate after seeing results.

## Task 1: Baselines and measurable copy/retention contracts

**Files:** Modify `crates/nitride-cli/src/bin/profile.rs`; create
`scripts/shared_subtrees.py`, `scripts/test_shared_subtrees.py`,
`docs/stage15-progress.md`; extend `crates/nitride-cli/tests/profiling.rs`.

**Interfaces:** `SnapshotStats` in `syntax/shared.rs` will report `work`,
`memo_hits`, `memo_descendants_copied`, `winner_descendants_copied`,
`probe_descendants_copied`, `export_nodes_copied`, `reused_subtrees`,
`reparsed_bytes`, `source_bytes_copied`, `syntax_bytes_copied`, and
`full_parse_fallbacks` as u64. `StorageStats` reports syntax/source/index/sidecar
bytes, pinned bytes and live chunks. Baseline profiler emits the comparable
subset without pretending the old engine supports edit reuse.

- [x] Add `profile_separates_engine_and_export_storage` and Python
  `test_unique_shared_bytes_counted_once`: two readers of one chunk count one
  physical allocation; phase peaks cannot be summed as simultaneous live bytes.
- [x] Run `cargo test -p nitride-cli --test profiling` and
  `python3 scripts/test_shared_subtrees.py`; confirm the new assertions fail
  on missing fields/aggregation, then implement those measurement contracts.
- [x] Implement `python3 scripts/shared_subtrees.py --baseline --report
  docs/stage15-baseline.json`. Generate 256 KiB/4 MiB/16 MiB CSharp4 using the
  existing generator and recovery patterns; use 3 cold runs per workload and
  retain raw samples. Include earlier typed fixtures and small adversarial
  grammar inputs; do not overwrite Stage 13/14 artifacts.
- [x] Build `cargo build --release --locked`, run the baseline command, and
  record hashes, exact baseline configuration and proposed-gate ratios in the
  ledger. This is measurement work, not yet a performance improvement claim.

## Task 2: Immutable storage, persistent child placement and flat export

**Files:** Create `syntax/{shared,arena,sequence}.rs` and
`crates/nitride-core/tests/shared_syntax.rs`; modify `syntax.rs`, `lib.rs`.

**Interfaces:** Define the opaque types and `NodeView`/stats accessors above.
Crate-private `SyntaxBuilder` publishes children-before-parent records with
rule, consumed width, relative visible span, and persistent child placements.
Use grammar-owned stable labels. Arena builders supply stable handles while
published chunk dependencies obey an acyclic ownership order. `materialize`
visits occurrences postorder and resolves all absolute spans/labels.

- [x] Add `same_subtree_two_placements`, asserting identical `SubtreeId`,
  different absolute spans and equal text; `flat_export_is_postorder`, asserting
  every child ID is less than its parent; and `labels_survive_chunk_composition`.
- [x] Add `wide_sequence_insert_does_not_copy_all_siblings`: insert at the front
  of 1,000 and 100,000 placements; copied existing entries stay within the
  documented fanout/path bound, not proportional to list length. Include gaps,
  zero-width nodes and literal-only/alias-visible-span cases.
- [x] Run `cargo test -p nitride-core --test shared_syntax` to observe missing
  API failures; implement the storage, placement and export contracts.
- [x] Add arena unit tests `eviction_preserves_published_handles`,
  `sparse_chunks_release_unowned_dependencies`, and
  `deep_chunk_drop_is_iterative`; test explicit counters returning to zero
  after all roots release. Verify `ParseSnapshot: Send + Sync` at compile time.
- [x] Run `cargo test -p nitride-core`; retain all old public flat contracts.

## Task 3: Persistent UTF-8 text and parser-compatible readers

**Files:** Create `syntax/text.rs`; extend `shared_syntax.rs`; modify
`parser.rs` only to introduce reader access without changing parse selection.

**Interfaces:** `SourceText::new(&str)`, `len() -> u32`,
`replace(&TextEdit) -> Result<SourceText, Diagnostic>`,
`slice([u32; 2]) -> Result<Cow<'_, str>, Diagnostic>`; crate-private seekable
reader supports byte/scalar iteration and literal matching across pieces.
Store pieces in balanced fanout-32 sequences with aggregate UTF-8 byte lengths.

- [x] Add `edit_utf8_and_cross_piece_tokens` (supplementary scalar split attempts
  reject, valid edits preserve bytes); `old_source_survives_edits`; and
  `early_edit_copies_bounded_payload` for all three workload sizes.
- [x] Run `cargo test -p nitride-core --test shared_syntax`; confirm failures,
  then implement replacement, seeking and cross-piece slicing. Do not construct
  a full new String on edit; bounded token slices may allocate with counters.
- [x] Route parser literals/classes/trivia/exclusions through the reader and
  test every token boundary against the contiguous reader, including comments,
  EOF and `ParseOptions::token_children`/`alias_nodes`/`strict_comments`.
- [x] Run `cargo test --workspace --locked`; require existing tree/spans and
  preparation behavior unchanged before enabling shared construction.

## Task 4: Direct reuse in Pratt memos, alternative winners and recovery probes

**Files:** Modify `parser.rs`, `recovery/search.rs`, `recovery/tree.rs`;
extend `shared_syntax.rs`, existing `precedence.rs`, `predicates.rs`,
`transparent.rs`, and CLI `csharp4.rs` test targets.

**Interfaces:** Implement `parse_snapshot` for cold parsing, initially with edit
disabled until Task 5. Legacy parse functions call the shared backend and
explicitly export once. `Winner`, `Memo::Hit`, and `ProbeHit` retain stable root
handles plus context/end/failure facts. Preserve NO_NODE/alias semantics without
creating an ordinary structural node for absent syntax.

- [x] Add `memo_hit_reuses_root_without_descendant_copy` and
  `winning_alternative_retains_original_root`; assert nonzero hits and all
  memo/winner descendant-copy counters are zero. Add
  `recovered_strict_probe_attaches_same_handle` for selected healthy siblings.
- [x] Run focused tests to observe failure with suffix-copy storage; replace
  rollback suffix truncation with temporary-state rollback and root ownership.
  Remove memo save/replay and winner suffix copies, not just their counters.
- [x] Replace `stand_in`'s pop-last-node assumption with selection of the child
  handle; preserve ranking/ties, lexical failure extent and all parse options.
- [x] Integrate speculative budgets and safe reclamation. Force tiny test
  budgets and repeated generation eviction; compare normalized results when
  both runs complete, and require structured limits otherwise.
- [x] Run `cargo test --workspace --locked` and
  `cargo run --locked -p nitride-prepare --example bootstrap -- --check`.
  Rerun the cold matrix; report compatibility export separately from shared
  parsing and quantify removal of old memo/winner/probe copies.

## Task 5: Dependency-indexed strict incremental parsing

**Files:** Create `incremental/{mod,dependencies,checkpoints}.rs`,
`crates/nitride-core/tests/incremental.rs`; modify `parser.rs`, `lib.rs`.

**Interfaces:** Implement `ParseSnapshot::edit` in strict mode. Internal
`RecognitionContext` identifies language/start rule/options/template identity,
lexical mode and precedence. `DependencySet` records byte intervals and
insertion-sensitive boundary/EOF observations for successes and failures;
store balanced indexes with aggregate bounds and lazy edit displacement.
Cache records retain transitive dependency and failure facts, not `furthest`
alone. `ReusePolicy::{Enabled, Disabled}` is a crate-private test switch.

- [x] Add `editing_one_method_shares_untouched_methods`: a middle-method edit
  changes its handle, preserves compatible sibling handles, updates spans and
  leaves the previous snapshot unchanged. Compare to reuse-disabled full parse.
- [x] Add `append_at_eof_invalidates_predicate`,
  `failed_alternative_lookahead_changes_winner`,
  `cross_boundary_comment_changes_tokenization`,
  `precedence_change_invalidates_operand`, and
  `zero_width_boundary_insertion_is_observed`; each must match full parsing.
- [x] Run `cargo test -p nitride-core --test incremental` for RED; implement
  dependency collection at actual input reads/EOF checks, transitive merging,
  conservative invalidation and checkpointed continuation reuse. Index lookup,
  displacement and publication must not scan/copy every old cache record.
- [x] Add `wide_parent_rebuild_is_bounded`,
  `old_snapshot_reads_during_new_edit`, `invalid_edit_is_non_mutating`, and
  `warm_limits_do_not_bypass_structural_caps`. Test a failed strict edit leaves
  the previous snapshot valid and does not expose a partial published result.
- [x] Run the focused target and 1,000 deterministic edit sequences per small
  grammar fixture. Verify invalidation includes failed branches, trivia and EOF;
  fall back explicitly when no sound checkpoint/continuation can be proved.

## Task 6: Recovered edits and shared recovery sidecars

**Files:** Create `recovery/incremental.rs`,
`crates/nitride-core/tests/incremental_recovery.rs`; modify
`recovery/{mod,search,tree}.rs`, `syntax/shared.rs`.

**Interfaces:** Enable `SnapshotMode::Recovering` for edits. Store coverage,
repair and diagnostic records relative to reusable region placement in a
persistent sequence. `diagnostics()`/`materialize()` resolve current absolute
coordinates; expose recovery iteration on snapshot views for measurements.
Record search/continuation dependencies and distinguish local recovery reuse
from strict-only reuse and full-file fallback in stats.

- [x] Add `missing_closer_edit_matches_full_recovery`,
  `repair_boundary_insertion_reopens_search`,
  `healthy_sibling_after_error_keeps_identity`, and
  `shifted_sidecar_preserves_coverage_and_diagnostics`; compare normalized
  repairs, coverage, damage, ordering and text with a fresh recovery parse.
- [x] Run `cargo test -p nitride-core --test incremental_recovery` for RED;
  implement conservative enclosing-region invalidation. Reuse repair output
  only with a proven compatible search frontier; uncertain cases redo recovery
  up to the whole file without changing existing equal-cost ranking.
- [x] Add `coverage_edit_does_not_rebuild_unaffected_stream` with 100,000
  coverage entries; zero-width missing tokens, Unicode deletions, truncation
  and limited-search fallback must retain every byte and preserve old snapshots.
- [x] Run both core recovery targets and
  `cargo test -p nitride-cli --test recovery`; retain the existing exhaustive
  small repair oracle and every reviewed cascade/preserved-sibling regression.

## Task 7: Occurrence-based semantic and template consumers

**Files:** Create `syntax/access.rs`, `crates/nitride-cli/tests/incremental.rs`;
modify `semantic.rs`, `templates.rs`, `recovery/semantic.rs` and exports.

**Interfaces:** Crate-private `SyntaxAccess` provides occurrence count,
`node(u32) -> Option<Node>` by value, `children(u32) -> &[Edge]`,
`label(&Edge) -> &'static str`, and
`text(u32) -> Option<Cow<'_, str>>`. Implement for existing flat syntax and a
`SnapshotOccurrences<'a>` postorder view retaining snapshot structure, dense
occurrence edges/spans and lazy text; construction is explicit semantic work.
Add `compile_snapshot(language: &'static Language, snapshot: &ParseSnapshot,
limits: Limits) -> Result<Program, Vec<Diagnostic>>`, matching the existing
compile contract: reject recovery diagnostics before lowering and reject
syntax-only languages with a capability diagnostic. Add
`compile_snapshot_recovering(language: &'static Language,
snapshot: &ParseSnapshot, limits: Limits) -> RecoveredAnalysis` with the
existing `program: Option<Program>` and `diagnostics: Vec<Diagnostic>` behavior;
keep host-effect barriers unchanged. Both functions reject a language identity
mismatch rather than interpreting structural rule IDs through another language.

- [x] Add `scope_edit_rebinds_shared_method`: an enclosing declaration edit
  keeps unchanged method syntax shared but recomputes its binding/type/HIR to
  equal a fresh compilation. Add `same_structure_different_occurrences_bind_separately`.
- [x] Add `snapshot_templates_preserve_holes_labels_and_origins` and
  `recovered_snapshot_never_executes_host_effects`; compare with flat paths for
  frozen typed/rewrites fixtures, damaged declarations and unknown names.
  Add `snapshot_compilation_rejects_other_language` with two languages whose
  numeric rule IDs overlap, asserting a capability diagnostic before indexing.
- [x] Run `cargo test -p nitride-cli --test incremental` for RED; replace direct
  flat-vector assumptions in semantic/template algorithms with accessors and
  explicit dense semantic indexing. Use snapshot text pieces directly, avoiding
  full `SyntaxTree::materialize()` or full source concatenation internally.
- [x] Run `cargo test --workspace --locked`; record semantic-index allocations
  separately. Ensure parse/edit latency never includes an implicit semantic
  postorder traversal and the legacy public vectors remain usable.

## Task 8: Differential, lifetime and performance admission

**Files:** Extend `scripts/shared_subtrees.py`, `scripts/test_shared_subtrees.py`,
profiler and incremental test targets; create `docs/stage15-evidence.md` and
`docs/stage15-{differential,scaling,lifetime}.json`; update README and CI native
tests only as needed for the new API/contracts.

**Interfaces:** Profiler accepts `--snapshot <profile> <source>` and
`--edits <profile> <source> <edits.json>` (mode is in JSON). The script accepts
`--measure --baseline-report docs/stage15-baseline.json --report-dir docs`
and `--differential --seed 20261025`; report schemas retain every failure and
fallback. Fresh full/reuse-disabled parses are independent comparator modes.

- [x] Add script unit tests for percentile calculation, unique chunk accounting,
  limits versus semantic mismatches, fallback classification and failure-report
  retention; run `python3 scripts/test_shared_subtrees.py` to RED then GREEN.
- [x] Run seeded edit differentials over all frozen/preparation profiles,
  grammar self-source and generated inputs; keep failures with seed/edit trace.
  Include recovered input and byte-for-byte old-snapshot checks after edits.
- [x] Run 10,000-edit latest-only and eight-reader-window lifetime tests,
  deliberately retained history, sparse chunks and fragmented source. Confirm
  release-all dynamic counters return to zero and destruction is stack-bounded.
- [x] Run the complete cold/edit matrix and all numeric gates. Use local
  edits at early/middle/late positions; distinguish repeated local edits from
  alternating distant edits and dense/junk/truncated recovery fallback.
- [x] Run `cargo fmt --all --check`, `cargo test --workspace --locked`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo run --locked -p nitride-prepare --example bootstrap -- --check`, and
  `cargo build --release --locked`. Run ignored Release scaling tests by their
  existing names and each newly introduced ignored lifetime/scaling test.
- [x] Run existing Python reference/parity commands from their current entry
  points, including `scripts/differential.py`, `scripts/prepare_parity.py`,
  `scripts/recovery_adversarial.py`, and recovery comparison with
  `--report docs/stage15-recovery-quality.json`. Recheck source-only offline
  native deployment using the Stage 14 procedure without sibling modifications.
- [x] Document public API examples, quantitative results, fallback limitations,
  semantic recompilation cost and every missed gate. Request whole-change code
  review, address findings and verify changed paths. Hosted macOS/Linux/Windows
  CI is reported separately and only claimed after an authorized push actually
  runs it. Keep local work available for review; no integration is implied.

## Self-review

Spec sections map to Tasks 1–8: identity and placement (2), lifetime (2/8), wide
lists/text (2/3/5), parse copies (4), dependencies/edits (5), recovery/sidecars
(6), consumers/export (2/7), and acceptance/evidence (1/8). Each Review Focus
item has named tests. Flat export and dense semantic indexing remain explicit;
both requested reuse modes are required before completion. Numeric budgets are
proposals in this plan and must not be represented as verified capabilities.

## Final execution audit

Tasks 1–8 have been executed locally. Performance admission is **not met**: all 15 instrumented cold-time gates, two peak-memory gates and all six allocator-free cold-time controls fail. All 96 edit cases and retained-memory gates pass. See docs/stage15-evidence.md and docs/stage15-admission.json. Four Important review findings were fixed and verified; the independent reviewer reached its usage limit before final post-fix sign-off. Preserve the managed detached checkout, raw reports and ignored plan workspace; no integration is authorized or performed.
