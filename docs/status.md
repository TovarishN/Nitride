# Current project status

Updated November 10, 2026. The published checkpoint is
[**0.4.0**](https://github.com/TovarishN/Nitride/releases/tag/v0.4.0).
[Release notes and verification](release-0.4.0.md) cover the three native platforms.
Engine milestones, background analysis, independent document scheduling and the
release-training fix were merged through [PR #43](https://github.com/TovarishN/Nitride/pull/43),
[PR #44](https://github.com/TovarishN/Nitride/pull/44),
[PR #45](https://github.com/TovarishN/Nitride/pull/45) and
[PR #46](https://github.com/TovarishN/Nitride/pull/46), all with passing hosted CI.
The [November 9 editor baseline](benchmark-2026-11-09-editor-responsiveness.md)
adds packaged VS Code extension-host acceptance and checked syntax/typed phase
measurements, merged in [PR #47](https://github.com/TovarishN/Nitride/pull/47)
with all hosted checks passing. Visual acceptance and painted-frame latency remain unverified.

## Language-author workflow

[PR #53](https://github.com/TovarishN/Nitride/pull/53) merged the versioned lexical
library, standalone starter projects and bounded project fixes, with all platform
checks passing. [Authoring documentation](authoring.md) and
[packaged acceptance](authoring-acceptance.md) describe the delivered scope.
The current language-testing feature adds native [`.ntest` suites](language-testing.md),
phase/diagnostic/value assertions, generated starter tests and VS Code Testing
using frozen inputs and separate cancellable native workers. The published
engine release remains 0.4.0. [Packaged API acceptance](language-testing-acceptance.md)
records the tested workflow and its focus/visual limitations.

## Unreleased Rust embedding

The development tree adds a [`nitride` facade](embedding.md), optional Cargo
build-script support, and a [standalone calculator](../examples/embedded-calculator/)
that owns its grammar and catalog. Default runtime dependencies are limited to
`nitride-core`; preparation and code generation stay in build dependencies.
Parse/check/preflight/execute errors retain diagnostics and invocation traces,
and whole-program preflight precedes host effects. The guide documents the
`0.x` source compatibility contract and generated-code version boundaries.
The facade is not part of the published 0.4.0 checkpoint and has not been
published to crates.io. Local macOS checks and an external copied-consumer
acceptance exercise the recipe; hosted cross-platform validation remains pending.

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

## Merged cancellation milestone

[PR #50](https://github.com/TovarishN/Nitride/pull/50) merged with all hosted checks
passing. [Cooperative cancellation](benchmark-2026-11-09-cancellation.md) adds opt-in
batched polling through parsing, recovery and spill threads. Obsolete LSP jobs
stop at checkpoints without publishing partial snapshots or cancellation
problems; cancelling a waiting request still preserves diagnostic work.
Semantic/host calls and bulk maintenance remain explicit interruption limits.
The [polling follow-up](benchmark-2026-11-09-polling.md) specializes traversal
by scope. The subsequent [active-polling layout change](benchmark-2026-11-09-active-polling.md)
improves active cached analysis by 2.5% and fresh analysis by 5.9% against that
build. Unscoped fresh analysis also improves 4.2%, with a 5.2% unscoped cached
regression in the measured round. Final local cancellation trials return in
0.07–0.64 ms; a direct pre-cancellation comparison shows 5.1% cached / 0.3% fresh
active-scope overhead. Further work should address cached-analysis cost while
preserving cancellation responsiveness. This milestone ships in 0.4.0.

## Packaged release acceptance

The [0.4.0 acceptance checkpoint](editor-acceptance-0.4.0.md) passes actual editor
typing, recovery, multi-document fairness, rapid-edit freshness and grammar reload
in disposable VS Code sessions. Full-language coverage includes a 4 MiB C# source.
Ordinary diagnostic observations are around 44–48 ms; large-file edit/feature
checks are 235–465 ms. Native PGO packaging and extracted Calendar process smoke
checks pass locally. The three-platform PGO rehearsal and release promotion both
passed; all 13 published files match the verified artifacts. The exact CI-built
macOS package also passes both desktop sessions and Calendar process checks.
Manual visual and Windows/Linux GUI acceptance remain unverified.

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
