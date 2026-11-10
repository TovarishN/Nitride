# Nitride documentation

Start with [current status](status.md) for what is implemented and verified, or
[architecture](architecture.md) for the crate map and data flow. The
[root README](../README.md) contains quick-start commands and dated comparisons.

## Build a language

- [Authoring guide](authoring.md): project files, lexical library, standalone starters and editor support.
- [Language testing](language-testing.md): native `.ntest` suites and VS Code Testing.
- [Catalog language](catalog-language.md): supported types, operations and semantic metadata.
- [Rust embedding](embedding.md): facade API, standalone example, build scripts and compatibility.
- [Host bindings](host-binding.md): explicit execution interfaces, process and component hosts.
- [Trivia policies](trivia.md), [scoped parse context](scoped-parse-context.md) and [structural patterns](structural-patterns.md).
- [Calendar example](../languages/calendar/README.md): a complete typed language and host workflow.

## Understand and develop the engine

- [Architecture](architecture.md): crate responsibilities, preparation, parsing, semantics and editor analysis.
- [LSP scheduling](lsp-scheduling.md): background work, freshness barriers and cancellation.
- [Incremental fuzzing](../fuzz/README.md): CI corpus replay, sanitizer campaigns and limits.
- [Parser optimization](optimizing-generated-parsers.md): generated parser strategy and measurement constraints.
- [Releases](releases.md): native packaging, PGO training and promotion.
- [Current engine milestone design](superpowers/specs/2026-11-07-engine-next-milestones-design.md) and [plan](superpowers/plans/2026-11-07-engine-next-milestones.md): dated implementation scope.

## Verification and measurements

Evidence applies to the revision, platform and workload named in each report.
Passing automated checks, packaged editor checks and manual visual acceptance
are separate claims; see [current status](status.md) for remaining limits.

- [0.4.0 release](release-0.4.0.md) and [packaged editor acceptance](editor-acceptance-0.4.0.md).
- [Authoring acceptance](authoring-acceptance.md) and [language-testing acceptance](language-testing-acceptance.md).
- [Editor responsiveness](benchmark-2026-11-09-editor-responsiveness.md), [projection](benchmark-2026-11-09-editor-projection.md) and [projection walks](benchmark-2026-11-09-projection-walk.md).
- [Cancellation](benchmark-2026-11-09-cancellation.md), [polling](benchmark-2026-11-09-polling.md) and [active polling](benchmark-2026-11-09-active-polling.md).
- [Storage baseline](benchmark-2026-11-07.md), [materialization](benchmark-2026-11-08-materialization.md) and [arena reader](benchmark-2026-11-08-arena-reader.md).
- [C# parser assessment](compiler-assessment-2026-10-30.md) and [strict JSON comparison](json-comparison.md).

## Historical records

The initial [design](design.md), [plan](plan.md) and [progress ledger](progress.md)
record the original milestone. `stage2-*` through `stage45-*`, the earlier memory
reports and comparison JSON files are historical implementation and measurement
evidence, not the current roadmap. Stage reports name their associated inputs,
samples and limits; [stage 45](stage45-comparison.md) is the final numbered
comparison checkpoint. [The harness](stage30-harness/) remains reproducible code.

Historical files remain at their original paths because reports, scripts and
external links refer to them. This index separates them from current guides
without deleting evidence or changing recorded measurements. Moving large
artifacts to release storage, or rewriting repository history to reduce clone
size, requires a separate migration that preserves those references.
