# Nitride Next Milestones Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the five approved project directions with measured results and current documentation.

**Architecture:** Establish reproducible measurements first. Preserve language defaults while connecting owned runtime languages to shared editor snapshots, adding explicit trivia policies, and extending Calendar through existing typed operations. Select storage changes from measured costs.

**Tech Stack:** Rust workspace, Python benchmark tooling, TypeScript VS Code extension, generated `.ngr` parsers and `.nproj` manifests.

**Spec:** `docs/superpowers/specs/2026-11-07-engine-next-milestones-design.md`

## Global Constraints

- Preserve untracked `.DS_Store`, `.claude/`, and `scripts/clean-build.sh`.
- Default language parsing retains current Unicode whitespace and C-style comments.
- No JSON-specific parser branches or Calendar-specific semantic-engine branches.
- Do not leak reloaded languages or extend their lifetimes with an unchecked cast.
- Keep static generated-language APIs ergonomic and old snapshots immutable.
- Keep Gregorian and Clock interface contracts unchanged.
- No upstream project modifications or release/publication required.
- Benchmark five measured repetitions after one untimed run; separate first-use initialization and steady-state measurements.
- Syntax reuse does not imply incremental binding; semantic recomputation remains explicit.

## Review Focus

- UTF-16 surrogate boundaries, CRLF, stale versions and multi-change notifications must not corrupt document text.
- Reloading/dropping a runtime language must neither invalidate old snapshots nor leak each language generation.
- Strict trivia must apply to recovery, generated parsing, runtime loading and lookahead as well as flat parsing.
- Calendar preflight/type failures must invoke no handlers, including when rewrites duplicate or discard syntax.
- Packed storage must preserve large offsets/depth/lookahead and immutable retained history through wide fallbacks.

---

## Task 1: Baseline and current project status

**Files:** Create `scripts/benchmark_checkpoint.py`, `scripts/test_benchmark_checkpoint.py`, `docs/status.md`, `docs/benchmark-2026-11-07.md`; generated data under `docs/benchmarks/2026-11-07/`. Reuse `scripts/compiler-audit`, `scripts/json-bench`, `scripts/csharp4_scaling.py`, `scripts/recovery_scaling.py` and `nitride-profile`.

**Interfaces:** Orchestrator CLI `python3 scripts/benchmark_checkpoint.py --suite csharp|json|edits|all --output PATH [--smoke]`. Output JSON identifies revision/diff/input hashes, variant, metric definitions, missing counters, raw samples and medians. `--smoke` labels output as incomplete evidence.

- [ ] Test metadata/sample serialization and reject mismatched-input before/after comparisons with synthetic harness results; run `python3 -m unittest scripts/test_benchmark_checkpoint.py` and observe the intended failures.
- [ ] Implement orchestration using existing harness commands; keep process startup, parse time, fingerprints, initialization and memory metrics distinct.
- [ ] Run smoke coverage, then the full native matrix specified in the design. Collect external comparison runs where available and record tool identities; do not substitute historical numbers for fresh runs.
- [ ] Record current baseline and measurement limitations in the report and current status entry point. Preserve historical reports.
- [ ] Verify harness tests and version metadata, inspect emitted JSON, and commit only this milestone's files.

## Task 2: Runtime-language snapshot ownership

**Files:** `crates/nitride-core/src/syntax/shared.rs`, `syntax/access.rs`, `parser/shared.rs` and related snapshot/arena interfaces; `crates/nitride-codegen/src/runtime.rs`; create `crates/nitride-cli/tests/runtime_snapshots.rs`.

**Interfaces:** Provide an owned runtime snapshot adapter from an `Arc<LoadedLanguage>` and source. The adapter exposes edits, diagnostics, storage/reuse counters and borrowed materialization/semantic access. It retains the language owner across edits and clones. Existing static `parse_snapshot` callers remain supported.

- [ ] Write regressions for snapshot access after dropping the original owner, edits after registry reload, drop of the final snapshot releasing the owner, and concurrent readers. Include typed checks and rule-label access, not just source text. Observe failure before adding the adapter.
- [ ] Refactor language ownership/lifetimes at the core boundary so runtime snapshots retain owned language tables. Keep borrowed views scoped to the snapshot; audit `ParseSnapshot::materialize`, `NodeView::children`, `SnapshotOccurrences` and arena labels for static references that could escape. Review the process analysis-cache key and retention policy for runtime generations.
- [ ] Validate static/generated and runtime trees, diagnostics and edit results against fresh flat parsing. Run `cargo test --locked -p nitride-cli --test runtime_snapshots --test shared_syntax` plus impacted core tests.
- [ ] Obtain independent ownership review; resolve findings before integrating with LSP. Commit the ownership milestone separately.

## Task 3: Incremental LSP synchronization and parsing

**Files:** `crates/nitride-lsp/src/server.rs`, `analysis.rs`, `lines.rs`, `registry.rs`; create `document.rs` for transactional changes and cached syntax ownership; tests `lines.rs`, `lifecycle.rs`, `diagnostics.rs`, `navigation.rs`, `hints.rs`, and new `incremental.rs`.

**Interfaces:** `LineIndex::checked_offset(line: u32, column: u32) -> Option<u32>` rejects invalid edit boundaries while existing clamped navigation APIs remain available. Document changes stage all edits against evolving text, then commit version/text/syntax together. Analysis uses the Task 2 adapter and recomputes existing semantic/index outputs.

- [ ] Add failing UTF-16/CRLF, multiple ranged changes, full replacement, stale version and invalid-transaction tests. Assert a failed transaction leaves version, text and syntax unchanged.
- [ ] Implement ordered transactional changes and advertise incremental synchronization (`textDocumentSync: 2`). Preserve compatibility with full-text notifications.
- [ ] Cache recovering snapshots for workspace programs, update them with compatible edits and invalidate on loaded-language identity or source-signature changes. Retain the full frontend analysis path where necessary.
- [ ] Differentially compare diagnostics/codes/spans/repairs, navigation and values against fresh analysis after valid-to-invalid-to-valid edits. Assert untouched subtree reuse on a multi-declaration fixture and owner release on close/reload.
- [ ] Run `cargo test --locked -p nitride-lsp` and relevant CLI LSP tests. Benchmark initial analysis, edit parsing, materialization/semantic work and end-to-end diagnostic latency at 256 KiB/4 MiB; report costs separately.
- [ ] Update editor documentation with actual behavior/limits and commit.

## Task 4: Explicit trivia policy and strict JSON

**Files:** `languages/project/Project.ngr`, `crates/nitride-prepare/src/nproj.rs`, `project.rs`, `project_seed.rs`; `crates/nitride-core/src/grammar.rs`, parser/lexical/recovery and dispatch modules; `crates/nitride-codegen/src/lib.rs`, `runtime.rs`, `lexical.rs`; `languages/json/project.json` and `.nproj` if present; project/parser/recovery tests and JSON harness conformance cases.

**Interfaces:** Core `TriviaPolicy::{Nitrogen, Json}` on `Language`; absence defaults to `Nitrogen`. `.nproj` accepts `trivia json;` or `trivia nitrogen;`; JSON manifests accept `"trivia": "json"|"nitrogen"`. Preserve the policy in prepared metadata and language identity. Unknown names and duplicate policy declarations are errors.

- [ ] Write failing project round-trip/invalid-policy tests and strict JSON tests rejecting comments and non-JSON whitespace while accepting space/tab/CR/LF. Test those characters inside string tokens separately.
- [ ] Extend project grammar/decoder and JSON preparation; propagate through code generation/runtime loading and initialize all in-repo language constructors. Regenerate the project seed with the fixed-point tooling.
- [ ] Apply the policy consistently in flat/shared/generated recognizers, dispatch start sets, lexical fast paths, recovery scanning, quotes and editor-loaded projects. Keep `strict_comments` as the independent unterminated-comment control for the default policy.
- [ ] Enable JSON policy for the shipped JSON profile. Run all 53 conformance cases, generated-versus-runtime and snapshot-edit parity, plus strict-policy recovery/lookahead tests. Add an attributed pinned external corpus when available.
- [ ] Run bootstrap fixed points, core/prepare/codegen/LSP focused tests and optional scanner features. Verify current default-language fingerprints remain unchanged. Re-run JSON measurements and document strictness/resource-limit boundaries; commit.

## Task 5: Complete Calendar planning workflows

**Files:** `languages/calendar/Calendar.ngr`, `Calendar.ncatalog`, `sample.calendar` and new documented workflow samples; Calendar integration tests under `crates/nitride-cli/tests`; `crates/nitride-lsp/tests/hints.rs` and related navigation fixtures; create `languages/calendar/README.md`.

**Interfaces:** Add scalar/scalar multiplication/division and duration/scalar scaling with existing engine `mul`/`div` handlers; unary scalar/duration negation uses `neg`. `weeks` rewrites to multiplication by 7 days; `fortnights` to multiplication by 2 weeks. Preserve postfix precedence 30, multiplicative precedence 20, additive precedence 10 and conversion precedence 5; prefix negation uses precedence 25 so `-2 weeks` negates the duration.

- [ ] Add failing workflows covering leap/year boundaries, negative/fractional durations, weeks/fortnights, Mondays/count/nth, nominal-type mismatch, division by zero and invalid dates. Assert deterministic values and coded source diagnostics where supported.
- [ ] Implement grammar/catalog changes with typed rewrites and engine overloads; reserve new keywords. Do not change host-interface versions or rebuild a component for unrelated contract changes.
- [ ] Run the deterministic workflow through embedded/fixture Rust, process bindings and existing component-enabled execution; compare values and conformance. Assert no handler invocation on preflight failures and correct origin chains through rewrites.
- [ ] Exercise LSP diagnostics/navigation/hints after edits using the same sample. Document clock grants, native-only component limitations, and exact CLI/embedding commands.
- [ ] Run focused Calendar/bindings/rewrite and LSP tests; commit the complete example.

## Task 6: Profile-guided storage experiment

**Files:** Start with `crates/nitride-core/src/parser/shared/facts.rs`, `incremental/dependencies.rs`, `syntax/arena.rs`, `syntax/memory.rs`, `syntax/shared.rs`; touch only the measured winning representation. Record experiments in `docs/benchmark-2026-11-07.md` and raw reports.

**Interfaces:** Preserve the logical `FactLog`, dependency-index and syntax-view contracts. Packed values use checked bounds and a lossless wide fallback; accounting counts actual capacities and shared ownership without double counting language tables.

- [ ] Measure byte layouts, counts, capacities, retained-history storage and heap peaks on baseline JSON/C# workloads. Select the largest actionable contributor; record the hypothesis before changing its layout.
- [ ] Add failing equivalence/overflow regressions for the selected representation, including maximum depth/lookahead, repeated facts, eviction, cross-piece edits and retained old snapshots.
- [ ] Implement one safe-Rust representation experiment, then run tree/diagnostic/recovery fingerprints and standard/adversarial edit traces before timing.
- [ ] Run paired baseline/candidate measurements with alternating order. Target at least 10% reduction in the affected heap component. Treat under-5% timing differences as inconclusive until replicated; reject a reproducible over-5% parse/edit slowdown unless separately approved with evidence.
- [ ] Keep only demonstrated wins. Record rejected attempts and exact limitations; commit code only if justified by evidence.

## Task 7: Documentation, aggregate validation and handoff

**Files:** `README.md`, `docs/status.md`, `docs/progress.md`, `docs/host-binding.md`, `docs/json-comparison.md`, editor README and final evidence report.

- [ ] Replace stale install examples with version 0.3.0 or a verified version-neutral command. Label the progress ledger as historical and link the current status page. Reconcile host-binding draft text with completed phases.
- [ ] Link current measurements without overwriting historical claims; describe runtime ownership, actual incremental editor behavior, trivia opt-in and complete Calendar workflow.
- [ ] Run `cargo fmt --all --check`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, bootstrap checks, optional scanner tests and native-only CLI/process/LSP tests.
- [ ] Run version/release-tooling and extension compile/tests. Review complete branch diff independently; reproduce and repair actionable findings.
- [ ] Report each milestone's result and measurement, rejected experiments, manual VS Code checks not performed, and cross-platform checks not yet run. Do not claim publication or full Nitrogen/DateCalc parity.

## Execution status

Design reviewed independently; no important unanswered planning questions found.
Implementation is pending concrete design/plan review. Milestones are intentionally
separate commits so measurements and behavior can be reviewed or reverted alone.
