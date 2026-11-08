# Current project status

Updated November 8, 2026. The starting published checkpoint was **0.3.0** on
November 7. The engine
milestones on `codex/engine-next-milestones` are local, unreleased development work.

## Implemented in this development branch

- Owned runtime syntax snapshots retain language tables safely through edits,
  concurrent readers and reloads. The final reader releases its snapshot slot.
  The core remains dependency-free; codegen uses `self_cell` for the owner/session.
- The LSP accepts ordered UTF-16 ranged edits transactionally, rejects stale
  versions and reuses recovering syntax snapshots for workspace programs.
  Binding, structural checks, navigation indexes and hints still recompute for
  the whole document; frontend authoring files retain full analysis.
- Projects can select [trivia policies](trivia.md). Strict JSON accepts only its
  four whitespace characters and no comments outside tokens. Generated and
  runtime parsers pass the pinned JSONTestSuite acceptance/rejection corpus.
- [Calendar](../languages/calendar/README.md) provides a complete planning
  workflow, duration scaling/negation, weeks and fortnights, with Rust, process
  and component execution. Existing host interface contracts are unchanged.
- A retained dependency-index storage change uses smaller records with a
  lossless wide fallback. Paired benchmarks document the savings and costs.

## Evidence

The [November 7 measurement report](benchmark-2026-11-07.md) separates the initial
44-case checkpoint, paired storage experiments and end-to-end editor analysis.
Historical [C#](compiler-assessment-2026-10-30.md) and
[JSON](json-comparison.md) comparisons retain their original dates and limits.

The approved [design](superpowers/specs/2026-11-07-engine-next-milestones-design.md)
and [implementation plan](superpowers/plans/2026-11-07-engine-next-milestones.md)
record scope, compatibility constraints and acceptance gates. The old
[progress ledger](progress.md) describes the initial stages, rather than the
current feature set.

## Boundaries and subsequent directions

Nitrogen compatibility is partial. CSharp4 remains syntax-only; full DateCalc,
arbitrary C# semantic helpers, modern C# semantics, embedded targets and no_std
are unfinished. Distributed native binaries omit the component runtime;
process-only Calendar is the native deployment example.

The next engineering decisions should use measured editor phase costs:
avoiding repeated flat-tree materialization, background scheduling/cancellation,
incremental semantic dependencies, and
further tree/index storage work remain possible follow-ups. Larger language
coverage and automatic bindings from non-component metadata are separate
projects. Manual VS Code acceptance and new hosted cross-platform validation
must be distinguished from local automated tests.
