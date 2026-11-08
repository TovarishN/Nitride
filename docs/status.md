# Current project status

Updated November 8, 2026. The starting published checkpoint was **0.3.0** on
November 7. Engine milestones and background analysis were merged through
[PR #43](https://github.com/TovarishN/Nitride/pull/43) and
[PR #44](https://github.com/TovarishN/Nitride/pull/44), both with passing hosted CI.
Independent document timing is the next unreleased change on
`codex/lsp-document-scheduling`; local verification is recorded in the scheduling
guide. Manual VS Code acceptance remains unverified.

## Implemented in this development branch

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

The next engineering decisions should use measured editor phase costs:
avoiding repeated flat-tree materialization, cooperative parser interruption and language-scoped reloads,
incremental semantic dependencies, and
further tree/index storage work remain possible follow-ups. Larger language
coverage and automatic bindings from non-component metadata are separate
projects. Manual VS Code acceptance and new hosted cross-platform validation
must be distinguished from local automated tests.
