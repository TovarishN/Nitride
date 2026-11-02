# Parse Throughput and Memory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Nitride's full parse faster and smaller on large inputs, and make repeated parses in one process cheaper than the first, measured against System.Text.Json with `scripts/json-bench`.

**Architecture:** Recognition facts go into a flat log during the parse, and the snapshot packs them into the edit-reuse `Index` only on its first edit, using a linear counting sort by position. A public `SnapshotParser` keeps its scratch collections between parses. Memory changes are chosen from heap measurements, PGO is measured with a bench-local script, and the comparison doc is rewritten with before and after numbers.

**Tech Stack:** Rust 2024 (`nitride-core`), Python 3 bench scripts, .NET 10 STJ harness (unchanged).

**Spec:** `docs/superpowers/specs/2026-11-02-parse-throughput-design.md`

## Global Constraints

- The public parsing API may change. Update in-repo callers in the same commit: `nitride-cli`, `nitride-lsp`, `scripts/json-bench`, `scripts/compiler-audit` and tests.
- Every existing test stays green (`cargo test --workspace`), and `cargo clippy --workspace --all-targets` stays at zero warnings. Run `cargo fmt` before every commit.
- Editable parses keep their behavior: an edit reuses the same subtrees as before (`stats().reused_subtrees` unchanged on the existing incremental tests).
- No JSON-specific code in `nitride-core`.
- Add no `unsafe` without a test that would catch its misuse.
- Do not commit the user's uncommitted `nitride.nproj` edit, `.DS_Store` or `.claude/`.
- Every bench number is taken on this host with `cargo build --release` in `scripts/json-bench` (LTO, one codegen unit). PGO numbers are reported separately.

## Review Focus

1. **Concurrent first edits.** Two clones of one cold snapshot are edited on two threads at once. Both must get a correct index, packed exactly once, with no deadlock or panic. Test in Task 2.
2. **Fact removal under cache pressure.** Eviction and `retain` remove facts from the log mid-parse. A stale slot must never point at another fact. Test in Task 2 (a model test of `FactLog` against a `HashMap`).
3. **A reused parser after a failed or spilled parse.** A syntax error, a recovery parse, or a parse that reran on the deep-stack thread must leave the scratch usable, with no stale memo or facts. Test in Task 3.
4. **Compaction of a cold sparse graph.** The deferred log must be remapped through the compaction mapping, and facts whose syntax was reclaimed must be dropped, as `Index::retain_mapped` does. The existing `tests/incremental.rs` edits cover this once cold snapshots defer. Task 2 also adds an explicit equivalence test.
5. **The first edit of a large snapshot.** It now pays to pack the index. Measure it on 16 MiB in Task 1's `edit` mode and report it. If it exceeds 50 ms, Task 2's last step packs eagerly on a background thread instead.

---

### Task 1: Bench instrumentation and baseline

**Files:**
- Modify: `scripts/json-bench/src/main.rs`
- Modify: `scripts/json-bench/Cargo.toml`
- Create: `docs/json-comparison-baseline.json`

**Interfaces:**
- Produces these modes of `nitride-json-bench` (existing `check` and `bench` are unchanged):
  - `stats <file>`: prints `SnapshotStats` and `StorageStats` to stderr.
  - `loop <file> <n>`: parses `n` times, for profiling.
  - `edit <file> <count>`: one JSON line `{"first_edit_ns", "median_edit_ns", "edits"}`.
  - `heap <file>`: one JSON line `{"peak_bytes", "live_after_bytes", "source_bytes"}`. This mode is built only with the cargo feature `heap`, which installs a counting global allocator, so the timing build has no counting allocator.

- [ ] **Step 1:** Keep the uncommitted `stats` and `loop` modes already in `main.rs`. Move the counting allocator and its statics behind `#[cfg(feature = "heap")]`, add `[features] heap = []` to `Cargo.toml`, and add the `heap` mode under the same cfg.
- [ ] **Step 2: Add the `edit` mode.** Parse once. Then apply `count` edits in sequence, each on the previous snapshot. Edit `i` replaces the ASCII digit at or after byte `i * len / count` with a different digit, which keeps the JSON valid. Time each `ParseSnapshot::edit(&TextEdit { range: [at, at + 1], replacement: … }, Limits::default())`. Assert `diagnostic_count() == 0` after each edit.
- [ ] **Step 3: Run it.** Run `cargo build --release && ./target/release/nitride-json-bench edit target/json-bench/numbers-1m.json 20`.
  Expected: one JSON line, and `first_edit_ns` is within 2× of `median_edit_ns`, since nothing is deferred yet.
- [ ] **Step 4: Run it.** Run `cargo build --release --features heap --target-dir target/heap && ./target/heap/release/nitride-json-bench heap target/json-bench/numbers-16m.json`.
  Expected: `peak_bytes` is about 237 MiB and `live_after_bytes` about 102 MiB, matching the design's numbers.
- [ ] **Step 5: Record the baseline.** Run `python3 -I bench.py > ../../docs/json-comparison-baseline.json`. Then append `edit` (numbers-1m and records-16m, 20 edits) and `heap` (all three 16 MiB inputs) results under a top-level `"extras"` key. Use a small inline Python step, or extend `bench.py` with an `--extras` flag, which is preferred so Task 6 reruns it.
- [ ] **Step 6: Commit.** `git commit -m "bench: stats, loop, edit and heap modes; baseline numbers"`

### Task 2: Fact log during the parse, index packed on first edit

**Files:**
- Modify: `crates/nitride-core/src/incremental/dependencies.rs` (add `from_unsorted`)
- Create: `crates/nitride-core/src/incremental/recognitions.rs`
- Modify: `crates/nitride-core/src/incremental/mod.rs`
- Modify: `crates/nitride-core/src/parser/shared.rs` (`pending_index` becomes `facts: FactLog`; `publish`, `into_seed`, eviction and lookups)
- Create: `crates/nitride-core/src/parser/shared/facts.rs` (`FactLog`)
- Modify: `crates/nitride-core/src/syntax/shared.rs` (`recognitions: Recognitions`)
- Modify: `crates/nitride-core/src/recovery/search/shared.rs`, `crates/nitride-core/src/recovery/incremental.rs` (readers)

**Interfaces:**
- Produces:
  - `Index::from_unsorted(entries: Vec<(u32, Key, Hit)>, text_len: u32) -> Index` builds the same index as `from_entries`. It counting-sorts by position (buckets `0..=text_len`), then sorts each position's few entries by `Key`. Positions and keys are unique.
  - `pub(crate) struct Recognitions` in `incremental/recognitions.rs`:
    - `Recognitions::packed(Index) -> Self`
    - `Recognitions::deferred(entries: Vec<(u32, Key, Hit)>, text_len: u32) -> Self`
    - `fn index(&self) -> &Index` packs once, through a `OnceLock` shared by clones (an `Arc` inside), and frees the log after packing.
    - `fn is_packed(&self) -> bool`
    - `fn previous_position(&self, at: u32) -> Option<u32>` and `fn current_position(&self, at: u32) -> Option<u32>` never pack. A deferred snapshot answers as `Index::default()` does, since it has no edit map.
    - `fn visit_allocations(&self, visit)` reports the log or the index.
    - `Default` is an empty packed index.
  - `pub(crate) struct FactLog` in `parser/shared/facts.rs` replaces `FastMap<(u32, Key), Hit>`. It holds `entries: Vec<(u32, Key, Hit)>` and `slots: FastMap<(u32, Key), u32>`. Methods, with the same meaning as the map's:
    - `get(&self, &(u32, Key)) -> Option<&Hit>`
    - `contains_key`
    - `insert(&mut self, (u32, Key), Hit) -> Option<Hit>`, which replaces in place
    - `remove(&mut self, &(u32, Key)) -> Option<Hit>`, which uses `swap_remove` and fixes the moved entry's slot
    - `retain(&mut self, impl FnMut(&(u32, Key), &mut Hit) -> bool)`
    - `values()`, `len()`
    - `with_capacity(n)`
    - `into_entries(self) -> Vec<(u32, Key, Hit)>`
    - `clear(&mut self)`, which keeps capacity for Task 3
- Consumes: nothing from Task 1, except the `edit` and `heap` modes for measuring.

- [ ] **Step 1: Write the failing tests.**
  - In `dependencies.rs` tests, `from_unsorted_matches_from_entries`: build 2,000 entries at random positions in `0..500`, with 1–3 distinct keys per position and a fixed seed. Shuffle them, then assert that `get(at, key)` agrees between `from_unsorted` and `from_entries` for every entry and for 100 absent `(at, key)` pairs.
  - In `parser/shared/facts.rs` tests, `fact_log_matches_hash_map_model`: run 10,000 random operations (insert, replace, remove, retain on odd positions, get) on a `FactLog` and a `HashMap` model with a fixed seed. After each operation, assert equal contents. This is Review Focus 2.
  - In `parser/shared.rs` tests, `cold_snapshot_defers_index_until_first_edit`: take the `tests/incremental.rs`-style language, `parse_snapshot` → `assert!(!snapshot.recognitions.is_packed())`. Then `edit` → the edited tree equals a fresh parse of the new text (compare node rule, span and child count), `edited.stats().reused_subtrees > 0`, and `snapshot.recognitions.is_packed()`.
  - In `parser/shared.rs` tests, `concurrent_first_edits_pack_once`: clone a cold snapshot and edit both clones from two `std::thread::scope` threads with different edits. Both results equal fresh parses. This is Review Focus 1.
  - In `parser/shared.rs` tests, `deferred_log_matches_eager_index_after_compaction`: on a backtracking grammar whose cold publish compacts (assert `stats().maintenance_passes == 0 && stats().reclaimed_syntax_bytes > 0`, or pick input until it does), compare `recognitions.index().get` with an eagerly built index for every logged `(at, key)`. This is Review Focus 4.
- [ ] **Step 2: Watch them fail.** Run `cargo test -p nitride-core from_unsorted fact_log cold_snapshot_defers concurrent_first_edits deferred_log_matches`.
  Expected: compile errors (`from_unsorted`, `FactLog`, `Recognitions`, `is_packed` do not exist).
- [ ] **Step 3: Implement the new types.** Implement `Index::from_unsorted`, `FactLog` and `Recognitions`.
- [ ] **Step 4: Switch the parser over.**
  - Replace `Parser.pending_index` with `facts: FactLog`. Pre-size it with `FactLog::with_capacity(text.len() / 16)`.
  - In `publish`, take the cold path `base.is_none() && parser.index.is_empty()`. When the graph compacts, remap and filter the log's roots through `mapping` (dropping facts whose syntax was reclaimed, as `into_seed` does), then store `Recognitions::deferred(entries, text.len())`.
  - Every other path keeps building eagerly and stores `Recognitions::packed`.
  - Point readers at `recognitions.index()` (`ParseSnapshot::edit`, `recovery/incremental.rs:291`, `recovery/search/shared.rs:471-476`), and the position lookups at `Recognitions::{previous,current}_position`.
  - Replace `self.recognitions.visit_allocations` accordingly.
- [ ] **Step 5: Run the new tests.** Run the Step 2 command.
  Expected: PASS, 5 tests.
- [ ] **Step 6: Run the whole suite.** Run `cargo test --workspace > $LEDGER_DIR/t2.txt 2>&1; tail -5 $LEDGER_DIR/t2.txt` and `cargo clippy --workspace --all-targets`.
  Expected: all green and zero warnings.
- [ ] **Step 7: Measure.** Rebuild the bench. Then run `bench target/json-bench/numbers-16m.json 10`, `edit target/json-bench/records-16m.json 20` and the `heap` mode on numbers-16m.
  Expected:
  - Warm time down at least 10% from baseline.
  - `live_after_bytes` down by roughly the index's 38 MB.
  - `first_edit_ns` reported.
  If the first edit exceeds 50 ms (Review Focus 5), have `publish` start packing on a detached `std::thread::spawn` holding an `Arc` clone, with `index()` waiting on the same `OnceLock`, and ledger the ruling.
- [ ] **Step 8: Commit.** `git commit -m "perf: record reuse facts in a flat log and pack the index on first edit"`

### Task 3: Reusable `SnapshotParser`

**Files:**
- Create: `crates/nitride-core/src/parser/shared/session.rs`
- Modify: `crates/nitride-core/src/parser/shared.rs` (`Parser::new` takes a `Scratch`, and `publish`/`into_seed` return it)
- Modify: `crates/nitride-core/src/parser.rs` and `crates/nitride-core/src/lib.rs` (export `SnapshotParser`)
- Modify: `scripts/json-bench/src/main.rs` (`bench` reports a reused-parser warm median too)

**Interfaces:**
- Consumes: `FactLog::clear` from Task 2.
- Produces:
  - `pub struct SnapshotParser`, `impl Default`:
    - `SnapshotParser::new() -> Self`
    - `fn parse(&mut self, language: &'static Language<'static>, source: &str, limits: Limits, mode: SnapshotMode) -> Result<ParseSnapshot, Diagnostic>`
    - `fn parse_generated(&mut self, compiled: &'static CompiledParser, source: &str, limits: Limits, mode: SnapshotMode) -> Result<ParseSnapshot, Diagnostic>`
    - `fn retained_bytes(&self) -> usize`, an estimate of scratch capacity in bytes
  - The free functions `parse_snapshot`, `parse_snapshot_generated` and `parse_snapshot_compiled` become `SnapshotParser::new().parse…` wrappers.
  - `pub(crate) struct Scratch` holds the per-parse collections that do not escape into the snapshot:
    - memo maps (`[FastMap<MemoKey, Memo>; 2]`)
    - `edge_buffers`
    - `choice_cache`
    - `tokens`
    - `facts` (cleared)
    - `pending_order`
    - `prior_paths`
    - `label_ids`

    Each is cleared, not dropped, between parses. After a parse, any collection whose capacity exceeds 64 MiB of payload is dropped, so one huge input does not pin memory forever.
  - Recovery-mode parses and edits (`base.is_some()`) may use fresh scratch. Only cold strict parses must reuse it.
  - The bench's `bench` mode adds `"warm_reused_median_ns"`: 10 warm parses through one `SnapshotParser`, the first of them discarded.

- [ ] **Step 1: Write the failing tests** in `session.rs` tests.
  - `reused_parser_matches_fresh_parses`: one `SnapshotParser` parses four sources in turn: a valid one, a syntax error (strict mode, expecting `Err` with category `syntax`), a valid one again, and an input deep enough to spill to the large-stack thread (a nesting depth the existing deep-input tests use). Each result equals `parse_snapshot` of the same source: same tree via `materialize()`, same diagnostics count. This is Review Focus 3.
  - `reused_parser_keeps_scratch`: `retained_bytes()` is `0` before the first parse and `> 0` after it.
  - `reused_parser_drops_oversized_scratch`: after parsing an input large enough to pass the 64 MiB cap (or a lowered cap through a `#[cfg(test)]` constant), `retained_bytes()` is at or below the cap.
- [ ] **Step 2: Watch them fail.** Run `cargo test -p nitride-core reused_parser`.
  Expected: compile errors (`SnapshotParser` does not exist).
- [ ] **Step 3: Implement it.** Implement `Scratch`, thread it through `Parser::new`, `parse_source`, `parse_with_spill` (pass `&mut Scratch` into the scoped spill thread) and `publish`/`into_seed`. Then add `SnapshotParser` and rewrite the free functions as wrappers.
- [ ] **Step 4: Run the new tests.** Run `cargo test -p nitride-core reused_parser`.
  Expected: PASS, 3 tests.
- [ ] **Step 5: Run the whole suite and clippy.** Same commands as Task 2 Step 6.
  Expected: green, zero warnings.
- [ ] **Step 6: Measure.** Run `bench` on `numbers-1m` and `records-16m`.
  Expected: `warm_reused_median_ns` is below `warm_median_ns`. Ledger the gain, whatever it is.
- [ ] **Step 7: Commit.** `git commit -m "perf: SnapshotParser keeps parse scratch between parses"`

### Task 4: Memory: kept bytes and peak

**Files:**
- Modify: as the measurements direct, within `crates/nitride-core/src/syntax/arena.rs`, `crates/nitride-core/src/parser/shared.rs` and `crates/nitride-core/src/parser/shared/*.rs`
- Test: existing suites, plus one regression test per kept change in the file it touches

**Interfaces:**
- Consumes: the `heap` and `stats` bench modes (Task 1), `FactLog` (Task 2), `Scratch` (Task 3).
- Produces: no new interfaces.

- [ ] **Step 1: Measure the breakdown.** Run `heap` and `stats` on all three 16 MiB inputs. Then sample peak heap at phase boundaries, which are:
  - after `parser.rule` returns
  - after memo drop in `publish`
  - after `arena.finish()`
  - after compaction

  Use a temporary `#[cfg(feature = "heap-trace")]` eprintln hook in the bench, read through a `pub static` hook pointer. Remove it before committing. Ledger the four numbers per input.
- [ ] **Step 2: Try the candidates** in order of measured share. Keep a change only if, on records-16m or numbers-16m, peak heap or `live_after_bytes` drops at least 5%, warm time does not rise more than 2%, and the suite stays green. The candidates:
  - (a) Pre-size the memo, `edge_buffers` and `ArenaBuilder` chunk vectors from `text.len()`, so the largest buffers do not double past their final size.
  - (b) Drop the memo and choice cache before `arena.finish()` (check what `publish` already drops), and release `retain_cache` holds on fact roots no fact still needs.
  - (c) Shrink per-node published storage, guided by `StorageStats::syntax_bytes / nodes`: narrower offsets or labels where `arena.rs` stores 64-bit values that fit 32 bits.
  - (d) Shrink the deferred `FactLog` entries to the `SmallEntry` packing that `PackedLeaf` already uses, where widths allow.

  For each kept change, write a test that fails without it before keeping the change. The test pins its invariant, such as a `#[test]` asserting `StorageStats::syntax_bytes` per node at or below the new bound on a fixed generated input. Ledger each rejected candidate with its numbers.
- [ ] **Step 3: Run the whole suite and clippy.** Expected: green, zero warnings.
- [ ] **Step 4: Commit.** One commit per kept change, for example `git commit -m "perf: pre-size parse buffers from input length"`.

### Task 5: PGO measurement for the bench

**Files:**
- Create: `scripts/json-bench/pgo.py`
- Modify: `scripts/json-bench/bench.py` (adds a `nitride-pgo` engine when `target/pgo/release/nitride-json-bench` exists)

**Interfaces:**
- Consumes: `profdata_tool(explicit)` from `scripts/release/build.py`, imported by inserting `scripts/release` into `sys.path`.
- Produces: `python3 -I pgo.py` builds `target/pgo/release/nitride-json-bench`:
  1. Instrumented build (`-Cprofile-generate=target/pgo/raw`, its own `--target-dir target/pgo-gen`).
  2. Training: `loop` 20× on each 1 MiB input, then `edit` 20 on records-1m.
  3. `llvm-profdata merge`.
  4. Rebuild with `-Cprofile-use=…` and `-Cllvm-args=-pgo-warn-missing-function` into `--target-dir target/pgo`.

  It uses `CARGO_ENCODED_RUSTFLAGS` as `build.py` does.

- [ ] **Step 1: Write and run it.** Write `pgo.py` and run it.
  Expected: it exits 0 and the binary exists. `target/pgo/release/nitride-json-bench bench target/json-bench/numbers-1m.json 10` prints a JSON line.
- [ ] **Step 2: Check the numbers.** Run `bench` on numbers-1m for both binaries.
  Expected: the PGO warm median is lower. Ledger the percentage.
- [ ] **Step 3: Commit.** `git commit -m "bench: PGO build of the JSON bench"`

### Task 6: Updated comparison

**Files:**
- Modify: `docs/json-comparison.md`, `docs/json-comparison.json`
- Modify: `scripts/json-bench/bench.py` only if `--extras` still needs finishing

**Interfaces:**
- Consumes: everything above. `docs/json-comparison-baseline.json` is the before.

- [ ] **Step 1: Rerun the bench.** Run `cd scripts/json-bench && cargo build --release && python3 -I pgo.py && python3 -I bench.py --extras > ../../docs/json-comparison.json`.
  Expected: rows for `nitride`, `nitride-pgo`, `stj-document` and `stj-reader`, plus the extras.
- [ ] **Step 2: Rewrite `docs/json-comparison.md`.**
  - Results table: add a "Nitride reused parser, warm" column and a PGO column, and keep the Nitride ÷ STJ document ratio.
  - A before/after table drawn from the baseline file: warm time, peak RSS, `live_after_bytes`, first-edit time and median-edit time.
  - Update the Assessment bullets: whether the ratio still grows from 1 MiB to 16 MiB, and the peak drop against the spec's "at least half".
  - State plainly which of the spec's success directions were met and which were not.
  - Keep the Conformance section. Rerun `check` for both engines and note any change.
- [ ] **Step 3: Check the conformance verdicts.** Run `./target/release/nitride-json-bench check target/json-bench/cases/*.json | diff - target/json-bench/nitride-check.txt`.
  Expected: no output, so the verdicts are unchanged.
- [ ] **Step 4: Commit.** `git commit -m "docs: JSON comparison after throughput and memory work"`
