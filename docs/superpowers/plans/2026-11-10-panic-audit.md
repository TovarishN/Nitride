# Panic audit and bounded interface examples

**Goal:** Audit user-input boundaries and fix demonstrated unbounded recursion without weakening internal invariants.

The user requested committing completed work and continuing. Commit `29ea6ca` records parser/doc cleanup, fuzzing and embedding. Preserve unrelated local files.

Initial inspection distinguishes test assertions and guarded storage invariants from reachable failures. SourceText validates ranges before locating/slicing; semantic checking validates syntax storage; execution preflight validates HIR before indexed access. Interface-example extraction, effect scanning and evaluation instead recurse without a depth/work guard. Parsed catalogs may reach those traversals after the parser has safely spilled to a larger stack.

- [x] Add failing tests: deeply nested catalog examples reject with a source error; default conformance rejects deep expressions without host effects. Run red tests on a large stack so the assertion, rather than a process-aborting stack overflow, exposes the missing bound.
- [x] Cap catalog example extraction at 128 expression levels, matching the core semantic hard ceiling. Add `host::conformance_with_limits`; existing wrappers use default limits. Iteratively inspect both expression sides for depth/work limits and denied effects before iterative evaluation, removing recursive effect scans and host evaluation. Catalog extraction/lowering stays bounded to at most 128 levels. Work is per example, shared by both sides; limit failures are reported as failed conformance cases without host calls.
- [x] Verify boundary/work cases, bounded denied-effect skipping and deep cases on a small thread stack. Preserve existing interface contracts, result comparison and ordinary example behavior.
- [x] Record audit scope and remaining invariant/host-code boundaries; run focused/full tests, clippy, formatting and bootstrap checks. Request independent review and commit the continuation.

Verification: both missing-bound regressions failed before implementation. Seven core
example tests and 19 catalog tests pass, including source-span rejection, limits before
host effects, text inputs, evaluation order, per-case budgets, and a 64 KiB stack.
Full workspace: 832 passed / 0 failed / 3 ignored. All-feature clippy,
formatting, documentation link checks and all bootstrap fixed points pass. Independent
read-only review found no actionable issues. Follow-up is committed as `81f098f`, separately from
`29ea6ca`; unrelated local files are preserved.
