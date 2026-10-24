# Error Recovery (Milestone B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Every representable input produces a complete recovered syntax tree, local diagnostics, and continued semantic analysis where supported, with recovery quality measured against Roslyn on CSharp4.

**Architecture:** Keep the current strict parser as the zero-repair reference. Add a separate recovery interpreter over the same grammar, using explicit continuations and bounded, minimum-cost local repair search. Retain original text, missing elements and skipped regions; extend semantic analysis with error propagation and prevent execution of programs carrying diagnostics. Measure damage using original-to-edited source maps and each parser's own undamaged trees.

**Tech Stack:** Existing stable Rust workspace, edition 2024, no third-party core dependencies; Python 3 measurement scripts; development-only .NET 10 and pinned Microsoft.CodeAnalysis.CSharp 5.0.0 (the version used by CSNitrogen's oracle).

**Spec:** `docs/superpowers/specs/2026-10-24-csharp-recovery-incremental-design.md`, Milestone B. The user's 2026-10-24 instruction to proceed supplies the milestone scope. This plan's algorithm, public interfaces and quantitative gates are builder choices proposed for review, rather than earlier user decisions.

## Global Constraints

- CSharp4 syntax only; no preprocessing, C# binding/type checking, editor integration or incremental parsing in this milestone.
- All existing strict observations remain identical, including invalid-input diagnostics, HIR, origins, execution effects, preparation parity and the bootstrap fixed point.
- Recovery is grammar-derived. No C# keywords, rule names, synchronization lists or precedence special cases enter the Rust engine.
- Ordinary native builds and runtime never invoke .NET. The core has no third-party dependencies.
- An error damages only nearby code; unaffected syntax and supported semantics must survive.
- Programs with diagnostics never invoke host operations or built-in value providers.
- Search, syntax storage, grammar analysis and semantic work are bounded. Limits produce structured diagnostics and preserve the source in the returned recovery tree.
- Work in the existing clean `codex/csharp-recovery` checkout. No sibling changes, commits, push, merge or publication are implied by executing this plan.
- Record per-task RED/GREEN results and rulings in `docs/stage14-progress.md`. Record final admission and limitations in `docs/stage14-evidence.md`.

## Review Focus

1. Optional/repeated constructs must not hide a broken member by ending the list early; later members remain discoverable (Tasks 2, 3, 5).
2. Recovery cannot turn failed lookahead into a successful branch, consume inside token internals, or change ordered choice/Pratt ranking on valid input (Tasks 1–3).
3. UTF-8, supplementary characters, unterminated comments/strings and EOF retain every original byte and report valid boundaries (Tasks 1, 3).
4. A recovered declaration shadows correctly, but an unrecoverable name creates no fabricated symbol; dependencies on damaged values propagate unknown without cascades (Task 4).
5. Dense errors and deep input cannot turn repeated whole-file retries, recursive reconstruction or unbounded search into quadratic work or stack overflow (Tasks 2, 3, 6).

## Algorithm and Representation Decisions

These are builder choices implementing the existing design, not an additional product scope.

- **Do not repair a copy of the file and parse the entire copy after each edit.** That loses original-source identity and risks quadratic cost with the number of errors.
- **Use explicit grammar continuations.** Compile `Expr` into a recovery-only instruction graph with sequence/choice/repetition continuations, rule entry/exit and Pratt precedence state. A state identifies instruction, original byte offset, continuation, lexical/predicate context and immutable output history. Deduplicate equivalent states; do not merge states with different predicates, precedence or continuation context.
- **Strict matching remains authoritative.** Zero-repair rule/lexical results are reusable in the recovery interpreter. First prove the interpreter's zero-repair behavior against the strict parser on all fixtures, including ordered choices, transparent alternatives and shared operators. Valid files return the ordinary strict tree without recovery metadata allocations.
- **Search repairs at lexical boundaries.** A terminal is an authored nonlexical literal or the outermost lexical rule. Token interiors and lookahead are strict; a missing lexical rule is represented as missing syntax of that category, never a fabricated identifier/number spelling. Failed lookahead can reject a candidate; it never emits repairs itself.
- **Choose locally by cost, then continuation viability.** Insertion of one expected terminal and deletion of one recognized token cost one each; replacement costs two. Unknown source consumes one Unicode scalar with a deletion cost. Dijkstra search chooses the least-cost candidate that resumes strict parsing through a grammar-derived enclosing continuation/list boundary or EOF. Equal costs prefer fewer skipped bytes, fewer missing elements, then authored alternative order. Use a stable final serial number; hash-map iteration must not decide a repair.
- **State the minimum-cost boundary honestly.** Search proves minimum cost only within its configured local domain when it completes. It does not claim a globally optimal file repair. Exhaustion triggers explicit resynchronization through grammar-derived continuation expectations, preserving skipped source and recording `search_exhausted` in statistics. Evidence includes every fallback.
- **A bounded search cannot promise perfect locality on arbitrary text.** Preserve complete source/tree coverage even when limits force a larger error region, and report that degradation. Do not hide it in quality aggregates.
- **Use original byte spans throughout.** The existing node/edge arenas retain grammar rule identities and postorder. Recovery metadata is optional and private on `SyntaxTree`; strict trees pay no sidecar allocation. A separate coverage stream partitions the source into retained token/literal, trivia and skipped spans, plus zero-width missing elements. This avoids reserving an invalid rule ID or pretending original punctuation already has nodes.
- **Keep partial semantics visibly non-executable.** Add recovery diagnostics to private `Program` state and check them before any host preflight work. Error HIR uses a distinguished kind/type rather than a valid Scalar or a synthetic constant. Analysis returns a program for inspection even when it contains errors; every execution entry point still rejects it.

## Task 1: Recovered Tree Contract and Zero-Repair Compatibility

**Files:** Create `crates/nitride-core/src/recovery/mod.rs`, `crates/nitride-core/src/recovery/tree.rs`, `crates/nitride-core/tests/recovery.rs`, `crates/nitride-cli/tests/recovery.rs`; modify `syntax.rs`, `lib.rs`, `semantic.rs` only as required by the contract.

**Interfaces:**
- `parse_recovering(language: &'static Language, source: &str, limits: Limits, options: RecoveryOptions) -> RecoveredParse`.
- `RecoveredParse { tree: SyntaxTree, diagnostics: Vec<Diagnostic>, stats: RecoveryStats }`.
- `RecoveryOptions { max_local_cost: u32, max_states: usize, max_local_bytes: usize }`, initially `4`, `4096`, `4096`; explicitly provisional until quality/scaling measurements.
- `RecoveryStats` exposes work, visited search states, repair count and fallback count.
- `SyntaxTree::recovery() -> Option<&RecoveryData>` exposes immutable repairs and source coverage. Missing elements identify expected literal or lexical rule and insertion byte offset; skipped elements retain their exact source span. No fake text is added to `source`.

- [x] Write `valid_recovery_is_identical_to_strict`, comparing every node, edge, span and root for hand-built literal, class, choice, repetition, predicate and Pratt grammars; metadata and diagnostics are absent.
- [x] Write `arbitrary_source_retained_on_limit`, using zero search/work limits, Unicode and nonempty junk. Assert a reachable root, exact original source, UTF-8 boundaries, complete coverage and an explicit limit diagnostic.
- [x] Run `cargo test -p nitride-core --test recovery`; observe missing API failures, then implement the tree contracts and strict success/structured fallback paths.
- [x] Write and run `strict_compile_rejects_recovery_metadata`: a tree carrying syntax damage is rejected before semantic lowering. Strict invalid-source observation remains unchanged.
- [x] Run the core recovery tests and existing workspace tests. Record which contracts are scaffolding only; do not claim recovery admission yet.

## Task 2: Grammar Continuations and Bounded Repair Search

**Files:** Create `recovery/grammar.rs`, `recovery/search.rs` and `crates/nitride-core/tests/recovery_search.rs`; modify `parser.rs` for crate-private strict rule matching/checkpoints, without changing public strict behavior.

**Interfaces:**
- Recovery grammar and search types stay private; the only public entry remains Task 1's `parse_recovering`.
- A strict checkpoint includes rule, byte offset, precedence, lexical/predicate context and immutable output reference; it may be reused only for the same complete context.
- Search returns selected grammar output, repair events, endpoint/continuation and statistics. It does not mutate source text.

- [x] First add zero-repair differential tests covering every `Expr` variant, longest-match/precedence alternatives, transparent rules, keyword boundaries, negative lookahead, shared postfix operators and nested lexical rules. Observe failures before implementing continuation recognition.
- [x] Implement the instruction graph and iterative worklist; check nullable and FIRST/continuation facts with a bounded fixed point. Preserve strict ordered choice and Pratt decisions; no repairs during predicate evaluation.
- [x] Add `missing_delimiter`, `stray_token`, `replacement_costs_two`, `equal_cost_is_deterministic`, `no_repairs_inside_lookahead_or_tokens` and `nullable_cycle_is_bounded`. Assert selected repair kind, cost, original spans and surviving suffix structure.
- [x] Observe failures; implement stable minimum-cost local search and equivalent-state dominance. Charge all failed probes and graph transitions to budgets. Storage for speculative outputs must be shared/arena-backed rather than copied per state.
- [x] Exhaustively enumerate tiny bounded grammars/inputs and compare local repair cost to a brute-force insertion/deletion oracle. Include a case where the first plausible insertion fails its continuation and a later candidate succeeds.
- [x] Run `cargo test -p nitride-core --test recovery_search` and the core suite; record RED/GREEN and local-optimality scope.

## Task 3: Complete Source Recovery and CSharp4 Locality

**Files:** Modify `recovery/mod.rs`, `recovery/tree.rs`, `recovery/search.rs`; extend both recovery test files; add small reviewed cases under `fixtures/recovery/`.

**Interfaces:** `parse_recovering` now drives strict matching, local repair and bounded grammar-derived fallback to a complete original-source tree. Recovery diagnostics group a contiguous repair episode, with explicit events retained for inspection.

- [x] Add failing cases for missing `;`, `)`, `]`, `}`, missing operand, extra/duplicated token, swapped neighbors, truncated input, malformed lexical input and several separated errors. Use both small non-C# grammars and real CSharp4.
- [x] For `class C { int A() { return 1 } int B() { return 2; } }`, require one local diagnostic and an unchanged `B` subtree after mapping offsets. Add missing class-body closer, nested block damage and broken list-element tests; assert later sibling preservation, not merely successful return.
- [x] Add coverage reconstruction tests for leading/trailing trivia, comment/string delimiters, supplementary characters and all-junk input. Recovery treats unterminated comments as damage even though ordinary strict parsing has a permissive comment policy; that existing strict policy stays unchanged.
- [x] Implement recovery at required terminals and enclosing continuation/list boundaries, preserving competing optional/repetition paths long enough to choose a local repair. Emit one primary diagnostic per selected local repair episode, without post-hoc hiding of unrelated semantic errors.
- [x] Add deterministic randomized UTF-8 inputs and a small-stack deep-input test. Require bounded termination, valid spans, reachable error regions and no dropped suffix. Preserve partial output on exhaustion and mark the remainder skipped explicitly.
- [x] Run all recovery tests and all frozen syntax parity tests. Record every unexpected locality/cascade failure as an open admission failure.

## Task 4: Continued Binding, Types and HIR; Execution Barrier

**Files:** Create `crates/nitride-core/src/recovery/semantic.rs`; modify `semantic.rs`, `execution.rs`, `templates.rs`; extend typed fixture recovery tests in the CLI test crate.

**Interfaces:**
- `compile_recovering(language: &'static Language, parsed: &RecoveredParse, limits: Limits) -> RecoveredAnalysis`.
- `RecoveredAnalysis { program: Option<Program>, diagnostics: Vec<Diagnostic> }`. Syntax-only profiles return no semantic program. Ordinary user errors in a typed profile return inspectable partial HIR; language/storage failures return explicit diagnostics.
- Add an error HIR kind and an internal error type, propagated through operations, calls, references and templates. `Program::diagnostics() -> &[Diagnostic]` provides immutable access to its private execution barrier.

- [x] Add failing tests: a broken statement between two valid declarations preserves the later declaration/reference's binding, type and HIR; `let x = ; x + 1; 2 + 3;` in Inference preserves `x` as damaged and the independent sum as healthy, with no dependent type/unresolved-name cascade.
- [x] Add shadowing/namespace tests: recoverable damaged names shadow prior values in their own namespace; missing names introduce no invented symbol; initializers keep sequential visibility.
- [x] Observe failures; refactor shared binding/lowering to support an explicit recovery policy. Carry error values through checks/overloads/templates, retaining concrete HIR and origins for healthy regions. Avoid recompiling every statement from the beginning of the document.
- [x] Add tests with invalid unused function bodies, missing template arguments, rewrite holes, checked dates and independent genuine semantic errors. Suppress only diagnostics caused by error values; preserve independent errors in source order.
- [x] Add handler/value-provider counters and invoke every preflight/execute variant with a recovered program whose first statement is valid. All calls fail before any counter increments, even if consumers ignore `RecoveredAnalysis.diagnostics`.
- [x] Run `cargo test --workspace --locked`; compare unaffected typed fixture observations with undamaged baselines. Document any required private-field API changes.

## Task 5: Damage Corpus, Roslyn Oracle and Measurable Quality Gate

**Files:** Create `scripts/recovery_corpus.py`, `scripts/recovery_compare.py`, `scripts/test_recovery_metrics.py`, `tools/Nitride.RecoveryReference/{Nitride.RecoveryReference.csproj,Program.cs}`, `fixtures/recovery/manifest.json`; modify CLI `lib.rs`, `main.rs` and recovery tests.

**Interfaces:**
- CLI: `nitride --recover <profile> <source-path>` outputs tree observations, explicit repairs, diagnostics, partial semantics and work statistics; diagnostics cause exit code 1 while retaining JSON output. Existing commands are unchanged.
- Roslyn tool: JSON-lines stdin/stdout for baseline and damaged source, returning UTF-8-normalized tokens/items/diagnostics and parse-only elapsed time. Reference only `Microsoft.CodeAnalysis.CSharp` version 5.0.0; no changes to sibling CSNitrogen.
- Damage manifest records profile, baseline identity/hash, operation, exact old/new byte ranges and seed. Baselines include every accepted frozen case and each deterministic generated case from the existing fixture generators.

- [x] Write metric tests using hand-checked before/after trees: insertion shifts, deletion-boundary overlap, zero-width damage, duplicate item names, unaffected nested members and UTF-16-to-UTF-8 diagnostic offsets. Observe failures before implementing mappings.
- [x] Generate six deterministic single-edit variants per eligible nonempty baseline: deletion, stray insertion, duplication, adjacent swap, mid-line truncation and closing-delimiter deletion. Record inapplicable operations explicitly; never silently omit a case. Use every eligible token position on the small reviewed regression corpus.
- [x] Add seeded multi-error sequences separately, and token-boundary damage in every admitted fixture including syntax-only XML/composition and grammar authoring. Freeze concrete inputs before adjusting recovery based on their results.
- [x] Implement the pinned Roslyn oracle with CSharp4 parse options. Preserve Roslyn baseline diagnostics and source-policy differences explicitly; do not silently remove difficult cases from the denominator.
- [x] Compute each parser's damage relative to its own original tree, comparing structural item fingerprints and source-mapped spans. Paired comparisons use common original item intervals at statement/member/top-level granularity; also report each parser's full self-relative denominator so common-item selection cannot hide losses.
- [x] Report preserved-item fraction, primary/error diagnostic count, distance to the edit, skipped-byte extent, recovery elapsed time, repair/fallback counts and failures by operation/profile. Typed fixtures additionally compare bindings, types and HIR of untouched units.
- [x] Gate the reviewed local-error corpus on complete coverage, deterministic output, preserved unrelated siblings and one primary syntax diagnostic per isolated error. Gate the broad C# corpus on paired preservation at least Roslyn's and diagnostic cascades no worse than Roslyn's, both overall and per damage family; publish case-level regressions. Resolve failures before calling Milestone B admitted; any changed threshold requires a documented ruling, never silently moving the bar.
- [x] Run `python3 scripts/test_recovery_metrics.py`, the frozen Rust recovery tests and `python3 scripts/recovery_compare.py`. Record Roslyn version, baseline counts and all failures in the report.

## Task 6: Scaling, Compatibility, Native Deployment and Review

**Files:** Create `scripts/recovery_scaling.py`, `docs/stage14-evidence.md`, `docs/stage14-recovery-quality.json`, `docs/stage14-recovery-scaling.json`; modify `crates/nitride-cli/src/bin/profile.rs`, `.github/workflows/ci.yml`, README and progress notes.

- [x] Add failing work-bound tests for 256 KiB and 4 MiB generated sources with one error, fixed-density errors, long invalid token runs and truncated nested constructs. Assert work per byte does not exceed twice the smaller case for the same damage pattern. Measure 16 MiB in release mode and report peak/retained storage and parse time separately from oracle startup/JSON serialization.
- [x] Repair any repeated-prefix reparsing or speculative history copying these tests expose. Keep counters deterministic; wall-clock measurements supplement rather than replace work bounds.
- [x] Run `cargo fmt --all --check`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, the rebuilt grammar bootstrap check, all existing live-reference/differential scripts, preparation parity and recovery-quality gates.
- [x] Build a clean source-only copy offline without .NET on PATH. Prepare CSharp4 and exercise recovery plus typed analysis with the release executable. Confirm zero third-party core dependencies; record CLI footprint and older phase allocation/retention changes.
- [x] Wire frozen recovery/coverage/semantic tests into the existing three-platform Cargo CI path. Hosted platform results are pending until actually observed; local macOS results alone do not establish three-platform success.
- [x] Review the whole diff against the spec and this plan, including private execution barriers, predicates, optional/repeat decisions, source coverage and search exhaustion. Record reviewer identity and unresolved findings honestly. No automatic publication.
- [x] Update admission documentation only after checking every gate. Separate complete-tree capability, measured locality, Roslyn comparison, physical scaling and editor integration claims. Milestone C remains unimplemented.

## Plan Self-Review and Reconnaissance

- The current strict parser uses mutable postorder arenas and backtracking marks, memoizes selected Pratt points, and drops failure context except for a furthest offset. Task 2 introduces the continuation/search machinery instead of pretending a diagnostic-position retry suffices.
- Existing syntax nodes omit literal punctuation and trivia. Task 1's explicit coverage stream is necessary to test complete-source recovery without changing strict node observations.
- `compile_checked` reports the first binding/type failure and `HirKind` has no error value. Task 4 owns continued semantic analysis; successful syntax recovery alone does not satisfy the milestone.
- Existing host preflight is centralized in `preflight_with_values`; every execution path must pass the new diagnostics check there. A wrapper flag checked only by the CLI is insufficient.
- Roslyn's [SyntaxParser](https://github.com/dotnet/roslyn/blob/main/src/Compilers/CSharp/Portable/Parser/SyntaxParser.cs) explicitly creates missing tokens and attaches skipped syntax. Its [parser guidelines](https://github.com/dotnet/roslyn/blob/main/docs/compilers/Design/Parser.md) explain why diagnostic placement and ambient context affect later incremental reuse. These support the representation/context constraints; the proposed grammar-derived search is our implementation choice.
- The [Nitra repository](https://github.com/rsdn/nitra) was located, but its recovery implementation was not fetched successfully during this planning pass. Studying and pinning its actual recovery source remains a Task 2 reconnaissance step; no Nitra algorithm parity or port is claimed.
- Spec coverage: complete trees and explicit regions (1–3), grammar-only recovery/local cost search (2–3), semantic continuation and effect barrier (4), Roslyn floor/damaged corpora (5), bounds/native builds/compatibility (6). Incrementality and LSP remain explicit non-goals.
- All five review-focus classes have tests assigned. Initial local search defaults are measured choices, not admitted quality claims. The most consequential risk is that bounded local search may fail the Roslyn floor; the evidence gate exposes that instead of accepting a weak fallback as milestone completion.

## Completion record

Implemented inline on `codex/csharp-recovery`, adapted to merged slim-tree base
`3784a91`. Search tests are consolidated in `crates/nitride-core/tests/recovery.rs`.
All local gates and the independent review/fix pass are recorded in
`docs/stage14-progress.md` and `docs/stage14-evidence.md`. Implementation initially ended uncommitted. The user separately authorized
merge and push on 2026-10-25; see the integration entry in the progress ledger. Existing Cargo CI discovers the new tests; hosted
results for these local edits remain pending.
