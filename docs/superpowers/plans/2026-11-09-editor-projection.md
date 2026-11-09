# Snapshot Editor Projection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove temporary full-tree construction/copying from eligible syntax-only editor analysis while preserving all editor outputs.

**Architecture:** A core postorder visitor projects occurrence nodes and child IDs; a scoped runtime callback retains ownership without holding its store lock. LSP eligibility guards retain metadata/check/typed fallback and build the existing owned editor Tree directly.

**Tech Stack:** Rust, existing standard-library and self_cell ownership machinery, existing Python matched benchmark runner.

**Spec:** docs/superpowers/specs/2026-11-09-editor-projection-design.md

## Global Constraints
- Existing public editor tree contents and all diagnostic/repair/feature results remain equal.
- No unsafe code, core dependencies, Nitrogen compatibility goal or main-workspace edits.
- Preserve bounded traversal and fallback, language ownership and reentrancy.
- Measurements use prebuilt matched binaries with no concurrent tests/builds.
- Keep dense editor output; do not claim fully snapshot-native feature queries.

## Review Focus
- Shared subtree placements and equal spans must preserve occurrence order/child IDs.
- Recovery repairs, skipped text and malformed Unicode-adjacent edits must match the oracle.
- Syntax-only rule checks or declaration/reference clauses must never bypass established checks/indexing.
- Scoped callbacks must permit reentrant runtime reads/edits without stale slots or deadlock.
- A projection limit failure must not commit a partial editor tree or alter diagnostics.

### Task 1: Bounded snapshot projection access
- [x] Add RED tests for node/child projection equivalence, placement offsets and work limits.
- [x] Implement core visitor and scoped runtime access; verify reentrant reads/edits.

### Task 2: Eligible editor analysis
- [x] Add RED eligible-path tests and exact legacy-oracle feature comparisons, with fallback guards.
- [x] Implement direct editor projection and repair/problem extraction with fallback.
- [x] Verify native/default feature suites and strict clippy/formatting.

### Task 3: Matched evidence and integration
- [x] Strengthen benchmark's outside-timing flat oracle, build matched baseline/candidate and run paired measurements.
- [x] Record scope/results, run one independent whole-branch review and fix Important findings with regressions.
- [x] Commit and publish a PR with measured benefits and limits.

## Final evidence

The Important wide-child scheduling finding was reproduced RED and fixed by
charging scheduling before expansion. Corrected verification passed 750 workspace
test executions (three ignored), 226 native-only LSP tests, five native CLI/runtime
tests, strict clippy, formatting, patch checks and 35 Python tests.

The refreshed matched valid 4 MiB comparison is 38.507 → 31.356 ms (18.6% lower),
with all five process pairs favoring the candidate. Other rows are exploratory
single pairs. No Critical findings remain. Deferred minor: the new projection
module lacks an individual source hash; the combined source fingerprint includes
it. See the measurement report for raw artifacts, scope, provenance and review
limits.
