# Current project status

Updated November 9, 2026. The published checkpoint remains **0.3.0**.
Engine milestones, background analysis, independent document scheduling and the
release-training fix were merged through [PR #43](https://github.com/TovarishN/Nitride/pull/43),
[PR #44](https://github.com/TovarishN/Nitride/pull/44),
[PR #45](https://github.com/TovarishN/Nitride/pull/45) and
[PR #46](https://github.com/TovarishN/Nitride/pull/46), all with passing hosted CI.
The [November 9 editor baseline](benchmark-2026-11-09-editor-responsiveness.md)
adds packaged VS Code extension-host acceptance and checked syntax/typed phase
measurements, merged in [PR #47](https://github.com/TovarishN/Nitride/pull/47)
with all hosted checks passing. Visual acceptance and painted-frame latency remain unverified.

## Implemented in merged development

- Owned runtime syntax snapshots retain language tables safely through edits,
  concurrent readers and reloads. The final reader releases its snapshot slot.
  The core remains dependency-free; codegen uses `self_cell` for the owner/session.
- The LSP accepts ordered UTF-16 ranged edits transactionally, rejects stale
  versions and reuses recovering syntax snapshots for workspace programs.
  Binding, structural checks, navigation indexes and hints still recompute for
  the whole document; frontend authoring files retain full analysis.
  [Background analysis and scheduling](lsp-scheduling.md) use one worker in
  positive-debounce mode, retain fresh request barriers, support cancellation
  of waiting requests and reject obsolete results by document/workspace
  generation. Independent quiet periods keep edits in one ordinary document
  from postponing another; target requests expedite only their file. Shared
  language reloads and global freshness barriers remain explicit.
- Projects can select [trivia policies](trivia.md). Strict JSON accepts only its
  four whitespace characters and no comments outside tokens. Generated and
  runtime parsers pass the pinned JSONTestSuite acceptance/rejection corpus.
- [Calendar](../languages/calendar/README.md) provides a complete planning
  workflow, duration scaling/negation, weeks and fortnights, with Rust, process
  and component execution. Existing host interface contracts are unchanged.
- A retained dependency-index storage change uses smaller records with a
  lossless wide fallback. Paired benchmarks document the savings and costs.
- Snapshot materialization avoids per-node temporary child vectors and repeated
  chunk-directory searches through a local borrowed reader. Follow-up matched
  editor benchmarks record the allocation and update-time improvements.

## Measured projection improvements

[PR #48](https://github.com/TovarishN/Nitride/pull/48) merged with all hosted checks
passing. Its direct snapshot-to-editor projection avoids temporary full-tree
export for eligible syntax-only workspace languages while preserving the dense
editor tree. [Matched measurements](benchmark-2026-11-09-editor-projection.md)
reduced cached valid 4 MiB C# analysis from 38.5 to 31.4 ms (18.6%, five pairs).

[PR #49](https://github.com/TovarishN/Nitride/pull/49) merged with all hosted
checks passing. Its [record-reuse follow-up](benchmark-2026-11-09-projection-walk.md) retains
node records within each walk and completes leaves inline while preserving
logical budgets. Large-valid medians improve by 5.2% and 9.4% in two rounds;
nine of ten pairs improve, with all samples retained. Smaller-case evidence is
exploratory. The final editor tree is still dense.

## Current cancellation milestone

[Cooperative cancellation](benchmark-2026-11-09-cancellation.md) adds opt-in
batched polling through parsing, recovery and spill threads. Obsolete LSP jobs
stop at checkpoints without publishing partial snapshots or cancellation
problems; cancelling a waiting request still preserves diagnostic work.
Semantic/host calls and bulk maintenance remain explicit interruption limits.
Local 4 MiB C# trials returned cancellation in 0.08–0.66 ms, with
6–7% fresh-analysis and 11–12% cached-analysis overhead in matched runs.
Reducing polling overhead is a concrete follow-up. This milestone is unreleased.

## Evidence

The [November 7 measurement report](benchmark-2026-11-07.md) separates the initial
44-case checkpoint, paired storage experiments and end-to-end editor analysis.
The [November 8 materialization follow-up](benchmark-2026-11-08-materialization.md)
records the next editor optimization and its fresh verification.
The [local arena-reader follow-up](benchmark-2026-11-08-arena-reader.md) measures
the subsequent reduction in chunk lookup costs against that optimized baseline.
Historical [C#](compiler-assessment-2026-10-30.md) and
[JSON](json-comparison.md) comparisons retain their original dates and limits.

The approved [design](superpowers/specs/2026-11-07-engine-next-milestones-design.md)
and [implementation plan](superpowers/plans/2026-11-07-engine-next-milestones.md)
record scope, compatibility constraints and acceptance gates. The old
[progress ledger](progress.md) describes the initial stages, rather than the
current feature set.

## Boundaries and subsequent directions

Nitride's goal is to advance as an independent language engine. Nitrogen is a
source of ideas and optional comparisons; compatibility is a nice-to-have, not
a roadmap goal or completeness requirement. Existing reference fixtures remain
useful regression evidence without defining which features Nitride must adopt.

CSharp4 remains syntax-only. Reproducing DateCalc or arbitrary C# semantic helpers
is not a target. Modern C# semantics, embedded targets and no_std are outside the
current scope. Distributed native binaries omit the component runtime;
process-only Calendar is the native deployment example.

Direct snapshot projection is the first measured step for syntax-only workspace
documents; fully snapshot-native feature queries and retained projections remain
possible follow-ups because dense editor arrays still rebuild after each edit. Typed indexing,
cold multi-language preparation and narrower reload invalidation remain separate follow-ups. Larger language
coverage and automatic bindings from non-component metadata are separate
projects. Packaged extension-host checks, manual visual acceptance and hosted cross-platform
validation remain distinct evidence.
