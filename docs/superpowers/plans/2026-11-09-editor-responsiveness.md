# Editor Responsiveness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Establish packaged-extension acceptance and a fresh analysis-phase baseline that selects the next optimization.

**Architecture:** A standalone VS Code extension test module exercises an extracted VSIX in a disposable host. A Python driver prepares fixtures, launches the host with bounded lifetime and captures a report. Existing checked analysis benchmarks provide phase timings; add typed Portable cases without changing production analysis.

**Tech Stack:** Existing Python standard library, Node, VS Code extension-test API and Rust examples; no added runtime dependency.

**Spec:** docs/superpowers/specs/2026-11-09-editor-responsiveness-design.md

## Global Constraints

- Preserve unrelated main-workspace files and work only in the isolated worktree.
- No Nitrogen compatibility goal. No production engine changes in this evidence PR.
- Use the actual packaged native server, fresh VS Code state and bounded timeouts.
- Keep GUI, extension-host, protocol and in-process evidence distinct.
- Compare incremental/fresh correctness outside timing; keep raw samples/hashes.

## Review Focus

- A stale diagnostic must not satisfy a current edit's acceptance check.
- No PATH/configured server fallback may hide a broken packaged binary.
- Cold activation and steady-state edits must be reported separately.
- Temporary workspace edits and test-host processes must be cleaned up on failure.
- Typed input and recovery cases must not be mislabeled as syntax-only or GUI latency.

### Task 1: Packaged extension acceptance
- [x] Write failing tests for VSIX extraction/validation and fixture isolation.
- [x] Implement driver and extension-host checks with response predicates and timeouts.
- [x] Run against the packaged darwin-arm64 VSIX; record evidence and limitations.

### Task 2: Analysis baseline
- [x] Extend the existing benchmark runner with checked typed Portable cases and per-source identity.
- [x] Test case generation and report validation, build a portable native benchmark once.
- [x] Run measurements without competing work; preserve raw data.

### Task 3: Decision and validation
- [x] Summarize measured phase costs and the next optimization, update stale status links.
- [x] Run focused tests, formatting/patch checks and independent whole-branch review.
- [x] Commit and open a PR with validation and explicit limits.

## Results

Eight baseline cases completed (three processes × five checked updates each), plus two packaged-extension sessions. See `docs/benchmark-2026-11-09-editor-responsiveness.md` and raw JSON under `docs/benchmarks/2026-11-09/`. Source-open readiness is separate from activation wait; neither is painted-frame latency. The next candidate is snapshot-native syntax-only access, subject to matched measurements and full feature equivalence.
