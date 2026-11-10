# Improvement tracker

Updated November 10, 2026. This tracks the eight suggestions from the project
review and the local follow-up on `codex/parser-structure-and-docs`. The recorded
implementation and documentation commits are in [PR #55](https://github.com/TovarishN/Nitride/pull/55),
with [passing Linux, macOS and Windows CI](https://github.com/TovarishN/Nitride/actions/runs/42887026311).
They have not been merged or released. The published version remains
0.4.0; [current status](status.md) separates merged development from local work.

## Original suggestions

| Suggestion | Disposition and evidence | Remaining scope |
|---|---|---|
| 1. Split parser/shared.rs | Implemented in `29ea6ca`: responsibility-based private modules preserve the public API and parser behavior. [Architecture](architecture.md), [completed plan](../superpowers/plans/2026-11-10-parser-structure-and-docs.md). | Further splitting recovery/search.rs or syntax/arena.rs remains optional. |
| 2. Add fuzzing and property checks | Implemented a differential incremental-edit fuzz target in `29ea6ca`, with sixteen seeds replayed by ordinary CI and a bounded local ASan campaign. [Oracle, commands and limits](../../fuzz/README.md). | Longer campaigns, more platforms and broader inputs remain open. This does not establish that parsing is panic-free. |
| 3. Audit core panics | Focused audit and demonstrated recursion fixes in `81f098f`: bounded catalog examples and iterative conformance inspection/evaluation before host effects. [Audit](../testing/panic-audit.md). | A complete panic/unsafe audit remains open; guarded internal invariants and abort-on-panic behavior remain. |
| 4. Prune and organize docs | Implemented in `80d64a7` and `53fdf7b`: stage archive, topic directories, migration manifests, integrity checks and generated reports under target/reports/. [Index](../README.md), [archive guide](../archive/README.md). | Raw evidence remains in-tree, so clone size is unchanged. External archival and history rewriting were not performed. |
| 5. Add changelog and architecture guide | Implemented in `29ea6ca` and `55854fc`. [Architecture](architecture.md), [changelog](../../CHANGELOG.md). | Maintain these as behavior and release state change. |
| 6. Define a Rust embedding API | Implemented in `29ea6ca`: facade, owned calculator, build helper and documented source compatibility contract. [Embedding guide](../guides/embedding.md). | Publication and copied-consumer recipe checks on additional platforms remain open. |
| 7. Check macOS CI cancellations | Investigated and documented in `55854fc`: runner-acquisition failures before checkout; the later main checkpoint passed. [Evidence](../testing/macos-runner-capacity.md). | No CI setting change was warranted by that finding. New failures require their own evidence. |
| 8. Test the small host crates | Implemented in `04c9b4e`: direct process/component tests and repairs for protocol, value-shape, component-type and binding-name failures. [Coverage and limits](../testing/host-bindings.md). | Process deadlines and asynchronous transport remain separate work; PR #55 adds passing hosted workspace checks to the retained local evidence. |

## Verification checkpoints

These are retained results from the named implementation slices, not fresh test
runs of every later documentation commit. Detailed commands and limits remain in
the linked plans and reports.

- Parser/fuzzing and embedding (`29ea6ca`): [parser plan](../superpowers/plans/2026-11-10-parser-structure-and-docs.md),
  [embedding plan](../superpowers/plans/2026-11-10-embedding-api.md). The final default
  workspace run recorded 824 passed, zero failed and three ignored. Fuzzing's
  final bounded ASan campaign recorded 4,738 executions in 61 seconds without a
  crash or oracle mismatch.
- Example bounds (`81f098f`): [audit plan](../superpowers/plans/2026-11-10-panic-audit.md)
  records 832 workspace tests passed, zero failed and three ignored.
- Host contracts (`04c9b4e`): [host-test plan](../superpowers/plans/2026-11-10-host-binding-tests.md)
  records 847 workspace tests passed, zero failed and three ignored; focused
  crate, CLI integration and WIT checks also passed.
- Topic layout (`53fdf7b`): [layout plan](../superpowers/plans/2026-11-10-docs-topic-layout.md)
  records 49 tooling tests and nine driver tests passed, 482 preserved local link
  destinations and byte-preserved raw reports (180 stage reports plus 19 topic
  reports). No Rust suite or historical benchmark was rerun for those moves.

## Next priorities

Incremental semantics is the next substantial investigation: identify reusable
binding/check/navigation/hint results and invalidation boundaries, then compare
incremental results against full analysis before changing editor behavior.
Whole-document semantics still recomputes after edits. This work has not started
in this improvement batch.

Longer fuzz campaigns and broader input coverage can continue independently.
Embedding adoption still needs package-name/publication work and copied-consumer
recipe checks on additional platforms.
The [ARM NEON follow-up](../benchmarks/scanners/neon-2026-11-10.md), stacked on
PR #55, now implements opt-in ARM classification with separate local measurements.
Recovery performance, a second editor, a browser playground and a larger typed
DSL remain candidate projects. None is implied complete by
this tracker, and Nitrogen compatibility does not define their scope.
