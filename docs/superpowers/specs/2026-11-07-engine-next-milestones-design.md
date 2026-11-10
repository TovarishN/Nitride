# Nitride next milestones

## Intent and authorization

The user requested planning and work on all five directions from the November 7
assessment: current measurements, incremental LSP parsing, configurable trivia,
a substantial DSL workflow, and smaller snapshot storage, plus documentation
cleanup. This document makes the proposed behavior and completion gates explicit.
Approved by the user ("appoved"); implementation is complete in the companion
plan. See [final measurements and verification](../../benchmarks/reports/benchmark-2026-11-07.md)
for the November 8 results and remaining boundaries.

Product direction clarified November 8: Nitride should advance independently.
Nitrogen compatibility is a nice-to-have, not a project goal or acceptance gate.
References to comparisons and DateCalc-shaped examples below describe engineering
evidence and useful workflows, not a parity roadmap.

## Starting point

Version 0.3.0 is published. GitHub main `297b75f` merges the current checkout's
`65fb759`; the merge has identical tracked contents. Local verification on
November 7 passed 675 Rust test executions (3 ignored), all-feature strict
clippy, formatting, four bootstrap fixed points, 14 release-tooling tests, and
extension compilation and 18 tests. These are a checkpoint, not future results.
Untracked `.DS_Store`, `.claude/`, and `scripts/clean-build.sh` belong to the
existing workspace and must be preserved.

The LSP currently advertises full text synchronization, calls `parse_recovering`
and `compile_recovering`, and copies a flat tree for navigation. Shared parser
snapshots exist independently. Published C# measurements predate the November
optimizations; JSON measurements also predate them. Process-level language
analysis storage is now shared and must be accounted separately from snapshots.

## Delivery order

1. Reproducible current baseline and an accurate project status document.
2. Incremental document synchronization and snapshot parsing in the LSP.
3. Language-specific trivia, with a strict JSON profile.
4. A complete Calendar workflow using duration arithmetic and typed rewrites.
5. Measured snapshot-storage improvements and refreshed final evidence.

Each milestone must build and pass its focused checks independently. The baseline
precedes optimization so results have a useful comparison. Existing APIs and
tests remain the compatibility reference; do not introduce JSON-specific branches
in the parser or Calendar-specific branches in the semantic engine.

## 1. Measurement contract

Add a small orchestration script around the existing C# scaling/recovery and JSON
harnesses. Record source revision and diff digest, input hashes, OS/CPU/toolchain,
compiler flags, engine variant, repetitions, samples and medians. Distinguish
portable release, native distributed PGO artifacts, and independently trained
benchmark binaries; never attribute one variant's results to another.

Cover valid, one-error and dense-error C# at 64 KiB, 256 KiB and 4 MiB; JSON
records/numbers/strings at 64 KiB, 1 MiB and 16 MiB; and single-character edits
on 256 KiB and 4 MiB inputs. Use five measured repetitions after one untimed run.
Run before/after variants in alternating order for changes claimed as wins.
Keep first-use initialization and steady-state measurements separate.

Report retained syntax, reuse index, source, shared language analysis, parse heap
peak, and process peak with definitions. Unsupported counters must be marked
unavailable. Match tree/diagnostic fingerprints before comparing performance.
Fresh Nitrogen/Roslyn/STJ runs may reuse the sibling development tools, but missing
reference dependencies must not prevent collecting the native baseline; report
missing comparisons explicitly. A smoke run is not the full benchmark matrix.

## 2. Incremental editor parsing

Advertise incremental LSP text synchronization and accept both full replacements
and ordered ranged changes. Interpret ranges in UTF-16 through `LineIndex`;
reject invalid boundaries without partially mutating a document. A notification's
changes are transactional, and stale versions must not overwrite newer text.

Store the current parser snapshot with the document analysis. For supported
workspace programs, parse the first version into a recovering shared snapshot;
subsequent compatible edits use snapshot edit APIs. Materialize the snapshot for
existing binding, checks, navigation and value hints. Recompute semantics and
indexes for the whole document initially: syntax sharing is not incremental
binding. Existing frontend files may retain full analysis until their preparer
supports the same route without changes to authored diagnostics.

Snapshot reuse requires the same loaded language identity and compatible masked
source. Grammar/catalog/project reload, language signature changes, or an
unavailable language invalidate cached syntax. Closing a document releases its
snapshot. Recovery options and work limits remain bounded. Preserve diagnostic
codes, repairs, source spans and editor results by differential tests against
fresh analysis after each edit. Record actual reuse and full request/diagnostic
latency; materialization and semantic cost must be visible.

An ownership prerequisite must be solved first: shared snapshot APIs currently
require a static language, whereas `LoadedLanguage` owns and frees runtime tables.
Runtime snapshots must retain their language owner, and derived node/tree views
must borrow that retained owner. Do not leak reloaded languages or extend their
lifetimes with an unchecked cast. Static generated-language callers must keep
their existing ergonomic API. Add a drop/reload regression before LSP integration;
any lifetime implementation needs focused independent review.

Background analysis, cancellation and incremental semantic dependency tracking
are separate future work; this milestone does not promise them. Measure the
existing synchronous path before deciding whether further scheduling is needed.

## 3. Configurable trivia

Add a per-language trivia policy at the project/manifest level, propagated through
prepared metadata, generated and runtime-loaded languages, all parser paths,
recovery and editor preparation. This is a builder choice to keep syntax rules
unchanged and avoid immediately expanding the self-hosted grammar language.

The default is today's Unicode whitespace plus C-style comments, including its
existing unterminated-comment policy. Add a strict JSON policy accepting only
space, tab, CR and LF between tokens and no comments. Expose the named policies
consistently in `.nproj` and JSON manifests, reject unknown names with authored
diagnostics, and include policy in prepared identity and snapshot compatibility.
Policy names and syntax are finalized in the implementation plan after tracing
the manifest decoder; prefer one named field rather than multiple Boolean flags.

Apply strict trivia to `languages/json`. Reject comments, NBSP and other Unicode
whitespace outside tokens; accept them inside strings when JSON's string grammar
allows them. Preserve UTF-8 handling, EOF behavior, depth/work guards, generated
versus interpreted parity, and scoped parse-context behavior. Recovery may report
repairs but must not turn a rejected strict input into a successful validation.
The existing 53-case suite is a regression fixture; add a pinned, attributed
external JSON corpus if network access permits. Resource-limit errors remain
distinct from grammar rejection.

## 4. Calendar as a complete application example

Extend Calendar's existing native/process/component-backed capabilities without
changing the Gregorian or Clock interface contracts. Add scalar multiplication
and division, duration scaling, unary scalar/duration negation, `weeks`, and
`fortnights`. Prefer typed syntax rewrites for weeks/fortnights and existing
engine handlers for arithmetic. Reserve added keywords consistently.

Demonstrate a planning workflow with sequential declarations, leap-day and year
boundary arithmetic, Monday selection over a span, list indexing and counts,
duration conversions, and type errors. Clock access stays explicitly granted;
offline deterministic samples do not depend on today's date. Document the same
workflow for embedded Rust and process bindings and, in component-enabled builds,
the existing component. Native distribution limitations remain visible.

Acceptance is end-to-end execution and conformance, source diagnostics and editor
navigation/hints after edits, plus zero handler invocations on preflight failure.
Pin results for wrong nominal types, invalid dates, fractional-day results, divide
by zero, negative durations, and out-of-range indexing. This is a concrete
DateCalc-shaped milestone, not full parity with arbitrary C# helpers or the
upstream Math function library. No upstream project source is modified.

## 5. Snapshot storage

Profile the new baseline before selecting a representation. Inspect pending fact
records, dependency-index leaves, syntax nodes/edges and retained history. Prefer
the largest measured contributor for a narrowly scoped safe-Rust change.
Candidates are narrower transient fact records and denser syntax payloads; do
not assume both must ship if one has no demonstrated benefit.

Use checked conversions and a wide fallback whenever packing can overflow. Keep
rule, offsets, lookahead extents, depth and subtree identities lossless. Preserve
thread-safe immutable old snapshots, cross-piece edits, recovery facts, and
structural matching. Include oversized offsets/depth and mixed packed/wide
records in tests. Never trade away bounded failure behavior to improve a score.

Aim for at least a 10% reduction in the targeted heap component on structured
1 MiB/16 MiB JSON, with no material parse/edit regression on C# or JSON. This is
a measurement goal, not a promised result. Reject experiments that produce only
noise or regressions and record them. Completing an experiment with a justified
rejection is valid; claiming a memory reduction without evidence is not.

## Documentation and integration

Create `docs/status.md` as the current entry point, keeping `docs/progress.md` as
a historical stage ledger. Refresh README installation versions and evidence
links, structural/trivia/editor documentation, and host-binding status wording.
Keep historical benchmark tables dated; add a new report instead of rewriting
history into current claims. Every current claim links to reproducible evidence.

Work in an isolated branch/worktree after design approval. Preserve unrelated
files. No release, upstream changes, or publication is required by this request.
Use reviewable milestone commits, then a final independent review and aggregate
checks. Report automated verification separately from any unperformed manual
VS Code acceptance or hosted cross-platform checks.

## Choices left to implementation

The exact cache ownership and edit API adapter, measurement script structure,
packing candidate, and thresholds based on observed noise are engineering
choices. If measurements invalidate an architectural assumption, update this
document with evidence before expanding the scope. Full C# semantics, modern C#
admission, no_std, general async scheduling and full DateCalc parity are outside
these milestones.
