# Nitride 0.4.0

[Nitride 0.4.0](https://github.com/TovarishN/Nitride/releases/tag/v0.4.0) was
published November 9, 2026 from verified commit
`4dcc669aa5ed0ed5985ce9221bbf09e3fd147898`. It advances Nitride as an independent
language engine.

## Changes since 0.3.0

- Owned runtime syntax snapshots retain their language safely through edits,
  concurrent readers and reloads. The LSP accepts ordered UTF-16 ranged edits
  transactionally and reuses recovering snapshots.
- Background analysis keeps the message loop available. Independent document
  quiet periods, bounded deferred requests and generation barriers preserve
  freshness and diagnostic fairness. Obsolete parsing/recovery can now cancel
  cooperatively, including spill threads, without publishing partial snapshots.
- Syntax-only editor projection avoids temporary full-tree export. Arena-record
  reuse and compiler-layout changes reduce measured analysis costs; retained raw
  reports also document tradeoffs and remaining cached-analysis overhead.
- Projects can configure trivia; strict JSON validates against the pinned corpus.
  Calendar completes duration/planning workflows across Rust, process and component
  execution. Existing native distributions continue to use process bindings.
- Native PGO training keeps LSP requests on stable document revisions, repairing
  the packaging failure during training. Packaging and CI checks are consolidated.

## Release validation

The checkpoint uses the real bundled native PGO server inside an extracted VSIX
with disposable VS Code profiles/workspaces. Acceptance covers routing, initial
readiness, editor typing, unique current diagnostics, malformed-source recovery,
neighbor-file fairness, rapid edits and unsaved grammar reload/restore. The
repository-language run additionally covers a generated 4 MiB C# document's
initial folds, missing-semicolon recovery, restoration and a uniquely positioned
leading comment fold after rapid edits. Reports retain package/server/harness/input hashes.

Launch-to-readiness includes VS Code and extension-test runner overhead. API
observations do not establish painted-frame latency or manual visual acceptance.
CSharp4 remains syntax-only. Cold preparation, typed whole-document recomputation,
semantic/host calls and bulk maintenance retain their documented limits. No
universal cancellation deadline is added. Native release packages omit component
support; process-only bindings remain available.

[PR #51](https://github.com/TovarishN/Nitride/pull/51) merged version synchronization,
release notes and validation evidence. The [three-platform PGO rehearsal](https://github.com/TovarishN/Nitride/actions/runs/42650947553)
passed on the exact release commit. The [tag workflow](https://github.com/TovarishN/Nitride/actions/runs/42658857118)
promoted those verified artifacts without rebuilding them and published the release.
All three VSIXs, CLI archives, metadata files and per-platform/combined checksums
were downloaded and verified; all 13 files match the rehearsal bytes exactly.
The [publication verification record](benchmarks/2026-11-09/release-040-published.json)
retains their hashes and source identity.

The [local packaged acceptance report](editor-acceptance-0.4.0.md) records passing
Portable and full-language/4 MiB sessions and extracted native Calendar smoke
checks. Ordinary diagnostics were observed in roughly 44–48 ms; large-file
feature checks took 235–465 ms after edits. Full local correctness/quality gates
passed. The exact CI-built macOS artifact was also validated in both desktop
profiles and through extracted native Calendar execution/conformance before
promotion. Its diagnostics were observed at about 43–45 ms; large-file edit/feature
checks took 246–476 ms. The byte-equivalent published package is covered by that
evidence; no new GUI claim is inferred for Windows or Linux.
