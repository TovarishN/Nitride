# Nitride documentation

Start with [current status](project/status.md) for what is implemented and verified, or
[architecture](project/architecture.md) for the crate map and data flow. The
[root README](../README.md) contains quick-start commands and dated comparisons.

The [improvement tracker](project/improvement-tracker.md) records the review
suggestions, completed local commits and remaining priorities.

## Directory map

| Directory | Contents |
|---|---|
| `project/` | Current status and architecture |
| `guides/` | Language authoring, embedding and host APIs |
| `editor/` | LSP design and scheduling |
| `testing/` | Validation, acceptance and audits |
| `releases/` | Release workflow and dated release notes |
| `benchmarks/` | Compiler, scanner, JSON and editor/storage measurements |
| `archive/` | Initial milestones, experiments and stage evidence |
| `superpowers/` | Dated implementation plans and designs |
| `stage30-harness/` | Reproducible comparison source, retained at its original path |

The [archive guide](archive/README.md) explains preservation and legacy-path lookup.

## Build a language

- [Authoring guide](guides/authoring.md): project files, lexical library, standalone starters and editor support.
- [Language testing](guides/language-testing.md): native `.ntest` suites and VS Code Testing.
- [Catalog language](guides/catalog-language.md): supported types, operations and semantic metadata.
- [Rust embedding](guides/embedding.md): facade API, standalone example, build scripts and compatibility.
- [Host bindings](guides/host-binding.md): explicit execution interfaces, process and component hosts.
- [Trivia policies](guides/trivia.md), [scoped parse context](guides/scoped-parse-context.md) and [structural patterns](guides/structural-patterns.md).
- [Calendar example](../languages/calendar/README.md): a complete typed language and host workflow.

## Understand and develop the engine

- [Architecture](project/architecture.md): crate responsibilities, preparation, parsing, semantics and editor analysis.
- [LSP scheduling](editor/lsp-scheduling.md): background work, freshness barriers and cancellation.
- [Host-binding crate tests](testing/host-bindings.md): direct process/component boundaries and contract validation.
- [Panic audit](testing/panic-audit.md): input boundaries, bounded interface examples and retained invariants.
- [Incremental fuzzing](../fuzz/README.md): CI corpus replay, sanitizer campaigns and limits.
- [Parser optimization](benchmarks/optimizing-generated-parsers.md): generated parser strategy and measurement constraints.
- [Releases](releases/README.md): native packaging, PGO training and promotion.
- [Current engine milestone design](superpowers/specs/2026-11-07-engine-next-milestones-design.md) and [plan](superpowers/plans/2026-11-07-engine-next-milestones.md): dated implementation scope.

## Verification and measurements

Evidence applies to the revision, platform and workload named in each report.
Passing automated checks, packaged editor checks and manual visual acceptance
are separate claims; see [current status](project/status.md) for remaining limits.

- [Changelog](../CHANGELOG.md): published releases and unreleased work.
- [macOS CI investigation](testing/macos-runner-capacity.md): hosted-runner capacity failures and the later passing checkpoint.
- [0.4.0 release](releases/0.4.0.md) and [packaged editor acceptance](releases/0.4.0-editor-acceptance.md).
- [Authoring acceptance](testing/authoring-acceptance.md) and [language-testing acceptance](testing/language-testing-acceptance.md).
- [Editor responsiveness](benchmarks/reports/benchmark-2026-11-09-editor-responsiveness.md), [projection](benchmarks/reports/benchmark-2026-11-09-editor-projection.md) and [projection walks](benchmarks/reports/benchmark-2026-11-09-projection-walk.md).
- [Cancellation](benchmarks/reports/benchmark-2026-11-09-cancellation.md), [polling](benchmarks/reports/benchmark-2026-11-09-polling.md) and [active polling](benchmarks/reports/benchmark-2026-11-09-active-polling.md).
- [Storage baseline](benchmarks/reports/benchmark-2026-11-07.md), [materialization](benchmarks/reports/benchmark-2026-11-08-materialization.md) and [arena reader](benchmarks/reports/benchmark-2026-11-08-arena-reader.md).
- [C# parser assessment](benchmarks/compiler/compiler-assessment-2026-10-30.md) and [strict JSON comparison](benchmarks/json/json-comparison.md).
- [Shared-parser per-rule cost](benchmarks/parser/kept-path-2026-11-12.md): boxed child buffers, cold-parse fast paths, first-token rejection and in-place pass-through rules.
- [Recognition facts for the outer list levels](benchmarks/parser/fact-levels-2026-11-12.md): memory, edit latency and the Pratt memo depth fix.

## Historical records

The initial [design](archive/initial/design.md), [plan](archive/initial/plan.md) and [progress ledger](archive/initial/progress.md)
record the original milestone. The [stage archive](archive/stages/README.md) holds
stage 2–45 plans, ledgers and measurement artifacts, grouped behind a navigation
index instead of filling the top level of this directory. The [stage 45 comparison](archive/stages/stage45-comparison.md)
is the final numbered comparison checkpoint. The [harness](stage30-harness/)
remains reproducible code at its existing location.

The stage move preserved 180 raw JSON reports byte-for-byte; the topic layout
preserved another 19. Frozen stage Markdown changed only to rebase links; recorded
results, command lines and provenance remain historical. The
[stage manifest](archive/stages/manifest.json),
[topic-layout manifest](archive/layout-manifest.json) and CI integrity checks
preserve that distinction. New benchmark output defaults to ignored
`target/reports/`. No evidence or Git history was deleted, and clone size is
not reduced; see the archive guide for legacy-link migration and reproduction limits.
