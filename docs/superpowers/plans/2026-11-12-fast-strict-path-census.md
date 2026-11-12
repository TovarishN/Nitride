# Fast strict path, milestone 0: decision census — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure how many grammar decisions a table-driven fast engine could settle from lookahead alone, and decide go/no-go for milestone 1.

**Architecture:** An opt-in `census` feature in `nitride-core` counts, at every ordered choice, Pratt prefix/infix phase, syntax predicate and repetition exit, how many alternatives the current engine actually attempts after its existing first-character routes and two-token leads. `nitride-profile --census` prints those counts for one parse; `scripts/parser_census.py` aggregates them over the corpora, applies the go/no-go thresholds and writes a report. No parsing behavior changes; with the feature off the hooks compile to nothing. The spec's static per-decision tables (`FastTables`) belong to milestone 1: milestone 0 measures outcomes under today's routes and leads, which is the lookahead those tables start from.

**Tech Stack:** Rust 2024 (`nitride-core`, `nitride-cli`), Python 3 standard library.

**Spec:** [docs/superpowers/specs/2026-11-12-fast-strict-path-design.md](../specs/2026-11-12-fast-strict-path-design.md), milestone 0. Milestones 1–5 get their own plans after the go/no-go.

## Global Constraints

- `nitride-core` stays free of Cargo dependencies; `census` is a feature with no dependencies.
- With `census` off, parsing code, timing and every existing test are unchanged.
- The census never changes a parse result: trees, diagnostics and logical work are identical with the feature on (checked in Task 1).
- The census runs the fully interpreted snapshot path (`parse_snapshot`, the editor's path); generated and interpreted parses make the same decisions through the same runtime functions.
- Existing CI commands must keep passing: `cargo +stable fmt --all --check`, `cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings` (this compiles `census`), `cargo +stable test --workspace --locked`.
- Go/no-go thresholds (this plan's call, from the spec's "most dynamic decisions" criterion), measured on CSharp4 generated 4 MiB valid input plus the Mono corpus, strict mode:
  - **G1:** at least 90% of choice decisions attempt exactly one alternative.
  - **G2:** at least 90% of Pratt phases attempt at most one candidate.
  - **G3:** at most 20 failed attempts that consumed input, per 1,000 source bytes, across choices, Pratt candidates and repetition exits.
  - **Predicate support:** if syntax predicates are evaluated more than 5 times per 1,000 source bytes, milestone 1 must support predicates instead of bailing out on them. This is a scope finding, not a no-go.

## Review Focus

1. **A decision reached through generated code and the interpreter must land in the same census entry.** Generated code passes its own `&Expr::Choice` constant, so entries are keyed by the enclosing rule and the choice's ordinal within it, not by the expression's address. Test: `census_matches_between_generated_and_interpreted` (Task 2).
2. **Nested parses must not leak into the outer census:** the recovery fallback and the profiler's warm-up parse run the parser again in the same process. `census_take` resets, and the profiler calls `census_begin` right before the measured parse. Test: `census_take_resets` (Task 1).
3. **Stack-spill threads:** deep inputs continue on a spill thread, so counts must be process-wide, not thread-local. Test: `census_counts_on_spill_threads` (Task 1).
4. **An attempt that fails after consuming input is the expensive case.** It must be told apart from a first-token failure by comparing the position before restoring, not after. Test: `census_counts_backtracking_past_shared_leads` (Task 1).
5. **Census on must not change results.** Tests: `census_does_not_change_results` (Task 1) and the fingerprint parity run (Task 4).

---

## File Structure

- Create `crates/nitride-core/src/parser/shared/census.rs`: census types, recording functions and the rule stack. With the feature off it holds only empty `#[inline(always)]` functions.
- Modify `crates/nitride-core/Cargo.toml`: add the `census` feature.
- Modify `crates/nitride-core/src/parser/shared.rs`: declare the module.
- Modify `crates/nitride-core/src/lib.rs`: re-export the census API under the feature.
- Modify `crates/nitride-core/src/parser/shared/prediction.rs` (`choice`, `choice_with_plan`), `pratt.rs` (both drivers' prefix and infix loops), `recognize.rs` (`match_rule` rule stack, `Expr::Predicate`, `Expr::Repeat`) and `generated.rs` (`end_iteration`): add the hooks.
- Modify `crates/nitride-core/src/parser/shared/tests.rs`: census unit tests under `#[cfg(feature = "census")]`.
- Modify `crates/nitride-cli/Cargo.toml` and `crates/nitride-cli/src/bin/profile.rs`: `census` feature and the `--census` flag.
- Create `scripts/parser_census.py` and `scripts/test_parser_census.py`: aggregation, thresholds, report.
- Create `docs/benchmarks/parser/decision-census-<date>.md` and `.json`: the census result and the go/no-go.

---

### Task 1: Census recording in `nitride-core`

**Files:**
- Create: `crates/nitride-core/src/parser/shared/census.rs`
- Modify: `crates/nitride-core/Cargo.toml`, `crates/nitride-core/src/parser/shared.rs`, `crates/nitride-core/src/lib.rs`, `crates/nitride-core/src/parser/shared/prediction.rs`, `pratt.rs`, `recognize.rs`, `generated.rs`
- Test: `crates/nitride-core/src/parser/shared/tests.rs`

**Interfaces:**
- Produces (public under `census`, re-exported from the crate root):
  - `pub enum DecisionKind { Choice, PrattPrefix, PrattInfix }` (`Clone, Copy, Debug, PartialEq, Eq, Hash`)
  - `pub struct DecisionStats { pub kind: DecisionKind, pub rule: u16, pub ordinal: u32, pub alternatives: u32, pub entries: u64, pub single: u64, pub attempts: u64, pub failed_attempts: u64, pub consumed_failures: u64, pub unmatched: u64 }`
    - `entries`: decisions reached; `single`: entries with exactly one attempt; `attempts`: alternatives or candidates actually run; `failed_attempts`: those that failed; `consumed_failures`: failures with the position past the decision's start before restore; `unmatched`: entries where nothing matched. For Pratt phases, a success is a candidate that `consider` made the winner.
  - `pub struct Census { pub decisions: Vec<DecisionStats>, pub predicate_evaluations: u64, pub context_entries: u64, pub repeat_exits: u64, pub repeat_consumed_exits: u64 }` (`Debug, Default, Clone`). `context_entries` counts entries into context-instance rules (`Rule::origin` is `Some`), a construct the spec lists as possibly unsupported in milestone 1.
  - `pub fn census_begin()`: clears all counts.
  - `pub fn census_take() -> Census`: returns the counts, sorted by `(rule, kind, ordinal)`, and clears them.
- Produces (crate-internal, always compiled; empty when the feature is off): `enter_rule(id: u16)`, `exit_rule()`, `decision(kind: DecisionKind, expr: usize, alternatives: u32) -> Decision`, `Decision::attempted(&mut self)`, `Decision::failed(&mut self, consumed: bool)`, `Decision::finish(self, matched: bool)`, `predicate()`, `context_rule()`, `repeat_exit(consumed: bool)`.

- [ ] **Step 1: Write the failing tests** in `tests.rs`, under `#[cfg(feature = "census")]`, using fixture languages built like `prefix_fact_fixture`. Each test calls `census_begin()`, parses with `parse_snapshot(language, source, Limits::default(), strict())`, then `census_take()`:
  - `census_counts_one_attempt_when_leads_decide`: `Root = "a" "x" / "a" "y" / "b"`. Input `"ay"`: the single `Choice` entry has `entries == 1`, `single == 1`, `attempts == 1`, `failed_attempts == 0`. Input `"b"`: the same values.
  - `census_counts_backtracking_past_shared_leads`: `Root = "a" "b" "c" / "a" "b" "d"`, input `"abd"`: `entries == 1`, `single == 0`, `attempts == 2`, `failed_attempts == 1`, `consumed_failures == 1`.
  - `census_counts_pratt_phases`: `prefix_fact_fixture(true)` on `"1+2"`: one `PrattPrefix` and at least one `PrattInfix` entry for rule 0, each with `attempts >= 1`, and `Census::decisions` sorted by `(rule, kind, ordinal)`.
  - `census_keys_choices_by_rule_and_ordinal`: a rule with two choices (`Root = ("a" / "b") ("c" / "d")`), input `"bd"`: two `Choice` entries for rule 0 with ordinals 0 and 1.
  - `census_does_not_change_results`: for each fixture above, the materialized tree, diagnostics and `stats().work` equal those of the same parse with `census_begin()` never called.
  - `census_take_resets`: parse `"ay"` twice with `census_take()` between; the second `census_take()` reports the `Choice` entry with `entries == 1`.
  - `census_counts_on_spill_threads`: the `"ay"` parse with `Limits { parser_stack_bytes: 1, parser_spill_stack_bytes: 8 << 20, ..Limits::default() }`, which runs on a spill thread, reports the same counts as without those limits.
- [ ] **Step 2: Run them to verify they fail.** Run: `cargo test -p nitride-core --features census --lib census_`. Expected: compile errors for `census_begin`, `census_take` and `DecisionKind`.
- [ ] **Step 3: Implement `census.rs`.** One process-wide `static CENSUS: std::sync::Mutex<State>` (Review Focus 3), where `State` holds a rule stack (`Vec<u16>`), per-rule choice ordinals (`HashMap<(u16, usize), u32>`, assigned in order of first sight), the counters keyed by `(u16, DecisionKind, u32)`, and the global counts. `decision()` reads the top of the rule stack as the rule; for `Choice` the ordinal comes from `(rule, expr)`, and for Pratt phases it is 0. With the feature off, every function is an empty `#[inline(always)]` function and `Decision` is a zero-sized struct.
- [ ] **Step 4: Add the hooks.**
  - `match_rule`: `enter_rule(id)` after the entry tick, and `exit_rule()` on every return path; wrap the body so early returns cannot skip it. Call `context_rule()` when `rule.origin.is_some()`.
  - `choice` (the plan-less loop) and `choice_with_plan` (all three attempt loops): `decision(Choice, expr as *const Expr as usize, parts.len() as u32)`. Call `attempted()` before each `attempt(...)`; `failed(self.position > mark.position)` when it returns `false`, before `restore` (Review Focus 4); `finish(true)` on success; `finish(false)` at the end. Pass `expr` from `choice` into `choice_with_plan`.
  - Both Pratt drivers: one `PrattPrefix` decision around each prefix loop and one `PrattInfix` decision per infix round, keyed by the point's rule id. Count `attempted()` for each candidate `rule(...)` call and `failed(...)` when it returns `None`, using the position before restore. `finish(winner.is_some())`.
  - `Expr::Predicate` in `expression`, when `!lexical`: `predicate()`.
  - `Expr::Repeat` in `expression` and `end_iteration` in `generated.rs`: on the failed iteration that ends the loop, `repeat_exit(self.position > mark.position)` before restore.
- [ ] **Step 5: Run the tests to verify they pass.** Run: `cargo test -p nitride-core --features census --lib census_`. Expected: 7 passed.
- [ ] **Step 6: Run the full checks.** Run: `cargo +stable test -p nitride-core --locked` (feature off; all existing tests pass) and `cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings`. Expected: both clean.
- [ ] **Step 7: Commit.**

```bash
git add crates/nitride-core
git commit -m "Add an opt-in decision census to the shared parser"
```

### Task 2: `nitride-profile --census`

**Files:**
- Modify: `crates/nitride-cli/Cargo.toml`, `crates/nitride-cli/src/bin/profile.rs`
- Test: `crates/nitride-cli/tests/census.rs` (create)

**Interfaces:**
- Consumes: `census_begin`, `census_take`, `Census`, `DecisionStats` and `DecisionKind` from Task 1.
- Produces: `nitride-profile [--native-syntax] --census <profile> <source>` printing one JSON object: `{"language", "source_bytes", "diagnostics", "decisions": [{"kind": "choice"|"pratt-prefix"|"pratt-infix", "rule": <id>, "rule_name": <name>, "ordinal", "alternatives", "entries", "single", "attempts", "failed_attempts", "consumed_failures", "unmatched"}], "predicate_evaluations", "context_entries", "repeat_exits", "repeat_consumed_exits"}`. The parse is strict with `Limits::default()`: fully interpreted (`parse_snapshot`) by default, or generated with `--native-syntax`, through the existing `snapshot_parse` helper.

- [ ] **Step 1: Write the failing test** `census_reports_csharp4_decisions` in `crates/nitride-cli/tests/census.rs`, marked `#![cfg(feature = "census")]`. It runs the built `nitride-profile` binary (`env!("CARGO_BIN_EXE_nitride-profile")`) with `--census csharp4` on `languages/csharp4/corpus/Mono2004/Boolean.cs` and asserts: exit success; `diagnostics == 0`; `decisions` is non-empty; every entry has `single <= entries`, `attempts >= single` and `failed_attempts <= attempts`; some entry's `rule_name` contains `"Statement"`.
  - `census_matches_between_generated_and_interpreted`: the same file with and without `--native-syntax` yields identical `decisions` lists and identical global counts.
- [ ] **Step 2: Run it to verify it fails.** Run: `cargo test -p nitride-cli --features census --test census`. Expected: FAIL; the profiler rejects `--census` as a usage error.
- [ ] **Step 3: Implement.** Add `census = ["nitride-core/census"]` to `nitride-cli`'s features. In `profile.rs`, add `--census` to the commands allowed after `--native-syntax`, and a `--census` branch next to `--snapshot`: resolve the language with `recovery_language`, call `prepare_language` (warm-up) and then `census_begin()` (Review Focus 2), parse, call `census_take()` and print the JSON with rule names from `language.rules[rule].name`. Without the feature, `--census` exits with code 2 and the message `nitride-profile was built without the census feature`.
- [ ] **Step 4: Run the test to verify it passes.** Run: `cargo test -p nitride-cli --features census --test census`. Expected: 2 passed.
- [ ] **Step 5: Commit.**

```bash
git add crates/nitride-cli
git commit -m "Report parser decision census from nitride-profile"
```

### Task 3: Census aggregation and go/no-go script

**Files:**
- Create: `scripts/parser_census.py`, `scripts/test_parser_census.py`

**Interfaces:**
- Consumes: the JSON from Task 2.
- Produces:
  - `aggregate(reports: list[dict]) -> dict` with keys `source_bytes`, `choice_entries`, `choice_single_share`, `pratt_entries`, `pratt_single_share`, `consumed_failures_per_kb`, `predicates_per_kb`, `contexts_per_kb` and `top` (the 20 decisions with the most `attempts - single`, each with `rule_name`, `kind`, `ordinal`, `entries`, `single`, `attempts`, `consumed_failures`). Rates are per 1,000 source bytes.
  - `verdict(summary: dict) -> dict` with `{"go": bool, "g1": bool, "g2": bool, "g3": bool, "predicate_support": bool}`, using the thresholds in Global Constraints.
  - CLI: `python3 -I scripts/parser_census.py --profile <nitride-profile> --report <path.json>` builds the generated CSharp4 inputs under `target/parser-compare/` (`compare_parser_builds.inputs`), runs `--census csharp4` on `4m-valid.cs` and every Mono corpus file, and runs each other profiler language on its `languages/<dir>/sample.*` file (report only, no thresholds). It prints the summary and verdict and writes the JSON report: per-language summaries, the CSharp4 verdict and the raw per-file reports.

- [ ] **Step 1: Write the failing tests** in `scripts/test_parser_census.py` (`unittest`):
  - `test_aggregate_shares_and_rates`: two synthetic reports totalling 2,000 source bytes, choices `entries=10, single=9` and `entries=10, single=8` (share 0.85), Pratt `entries=5, single=5` (share 1.0), `consumed_failures` totalling 30 (15.0 per KB) and `predicate_evaluations` totalling 4 (2.0 per KB).
  - `test_verdict_thresholds`: shares of exactly 0.90 pass G1 and G2, 0.899 fails; 20.0 consumed failures per KB passes G3, 20.01 fails; `predicate_support` is true above 5.0 per KB only; `go` is G1 and G2 and G3.
  - `test_top_ranks_by_extra_attempts`: `top` is ordered by `attempts - single`, descending.
- [ ] **Step 2: Run them to verify they fail.** Run: `python3 -I -m unittest scripts/test_parser_census.py`. Expected: ImportError for `parser_census`.
- [ ] **Step 3: Implement `aggregate` and `verdict`, then the CLI.** Prepend the script's directory to `sys.path` for the sibling imports under `-I`, as `scripts/compare_parser_builds.py` does.
- [ ] **Step 4: Run the tests to verify they pass.** Run: `python3 -I -m unittest scripts/test_parser_census.py`. Expected: 3 passed. Add the file to the `python3 -m unittest` list in `.github/workflows/ci.yml` (the line that lists `scripts/test_release.py`).
- [ ] **Step 5: Commit.**

```bash
git add scripts/parser_census.py scripts/test_parser_census.py .github/workflows/ci.yml
git commit -m "Aggregate parser decision census against go/no-go thresholds"
```

### Task 4: Run the census and record the go/no-go

**Files:**
- Create: `docs/benchmarks/parser/decision-census-<date>.md`, `docs/benchmarks/parser/decision-census-<date>.json`
- Modify: `docs/README.md` (benchmark index line), `CHANGELOG.md` (one Unreleased line)

**Interfaces:**
- Consumes: Tasks 2 and 3.

- [ ] **Step 1: Build and run.** Run: `cargo build --release -p nitride-cli --bin nitride-profile --features census`, then `python3 -I scripts/parser_census.py --profile target/release/nitride-profile --report docs/benchmarks/parser/decision-census-<date>.json`. Expected: a printed summary with G1, G2 and G3 verdicts for CSharp4, and per-language summaries.
- [ ] **Step 2: Check that the census changes nothing.** Run `scripts/compare_parser_builds.py` on two compiler-audit builds, one with `nitride-core/census` enabled through a `census` feature added to `scripts/compiler-audit/Cargo.toml` and one without, using `--rounds 1`. Expected: `parity: 69 runs, 0 mismatches`, logical work included.
- [ ] **Step 3: Write the report.** Follow `docs/benchmarks/parser/fact-levels-2026-11-12.md`: what was measured and how; the CSharp4 shares and rates against G1–G3; the 20 costliest decisions by rule; the predicate-support finding and how often context-instance rules are reached; other languages (report only); the verdict; limits (one host, one generated corpus shape plus the Mono files; the census counts attempts under today's LL(2) leads, which is the lookahead the fast engine's tables start from).
  - **Go:** the report names the hot backtracking decisions milestone 1 must handle and whether predicates are in scope.
  - **No-go:** the report names the decisions that fail G1–G3, and the work stops for a design discussion.
- [ ] **Step 4: Add the index and changelog lines.** Run: `python3 -I scripts/check_evidence_archive.py`. Expected: verification passes.
- [ ] **Step 5: Commit.**

```bash
git add docs/benchmarks/parser docs/README.md CHANGELOG.md scripts/compiler-audit/Cargo.toml
git commit -m "Record the fast strict path decision census and go/no-go"
```
