# Projection Walk Record Reuse Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reduce repeated arena-record reads and leaf traversal overhead without changing editor results or work budgets.

**Architecture:** Pending traversal frames retain copied immutable node records; leaves consume their exit budget inline. Test-only read counters prove the cost reduction. The existing paired benchmark records complete per-file provenance.

**Tech Stack:** Rust and Python standard libraries; existing native benchmark harness.

**Spec:** docs/superpowers/specs/2026-11-09-projection-walk-design.md

## Global Constraints
- Preserve callback order, exact editor trees, errors, partial output and work-unit semantics.
- Retain wide-root scheduling checks before allocation/expansion.
- No unsafe code, dependencies, production counters or main-workspace changes.
- Keep all existing typed, metadata and rule-check fallbacks.
- Use prebuilt matched binaries with no tests/builds during measurement; no GUI or typed speed claim.

## Review Focus
- A cached record must belong to its queued ID, including shared placements and changed chunks.
- Inline leaf completion must charge its exit before emitting and return the same budget error.
- Wide scheduling must reject before reading/allocating child frames.
- Postorder child IDs and prefixes must remain equal for every insufficient budget.
- Source provenance must distinguish an absent historical file from an invalid revision.

### Task 1: Traversal cost and equivalence
- [x] Add a test-only record-read counter and RED wide-tree cost bound.
- [x] Add budget-prefix/error equivalence coverage against the legacy traversal.
- [x] Implement cached records and inline leaf completion, run focused and full Rust verification.

### Task 2: Benchmark provenance and evidence
- [x] Add RED per-file hash tests, including absent files and invalid revisions.
- [x] Record projection module and owning sources; run tooling tests.
- [x] Build matched native binaries and run paired cases; retain only on measured improvement.

### Task 3: Review and integration
- [x] Document results and update merged status.
- [x] Complete one independent whole-branch review and regression-backed Important fixes.
- [x] Commit and open a PR with validation and limits.

## Results

The record-read bound failed on the merged visitor and passes after the change.
The independent budget oracle passes before and after. Verification: 751 workspace
test executions (three ignored), 226 native-only LSP tests, five native CLI/runtime
tests, 37 Python tests and strict clippy/format/patch checks passed.

Both matched rounds use identical binaries. Full-run large-valid medians improve
by 5.2% (four of five pairs); the independent primary repeat improves by 9.4%
(all five). All samples, including the regression, are retained. New per-file
provenance closes the prior projection-module hash minor. Independent review
found no actionable issues; no deferred minors remain for this change.
