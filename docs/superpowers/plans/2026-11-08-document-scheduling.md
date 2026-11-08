# Per-document Scheduling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prevent one document's edits or feature requests from postponing or prematurely starting unrelated document analysis.

**Architecture:** A private timing unit owns per-document deadlines/ready times and a shared reload deadline. Server events update the relevant timing state; worker dispatch consumes oldest-ready entries. Global freshness barriers explicitly expedite all work.

**Tech Stack:** Rust std Instant/Duration and BTreeMap, existing single-worker LSP.

**Spec:** docs/superpowers/specs/2026-11-08-document-scheduling-design.md

## Global constraints and review focus

- One outstanding job and 256 deferred-request bound remain unchanged.
- Preserve zero-debounce scripts, version/incarnation protection, cancellation and lifecycle behavior.
- Shared registry reloads gate every affected document; ordinary changes affect only their target.
- A targeted request must not expedite not-yet-due neighbors or jump older ready work.
- No lost work after unsupported URI removal, close or registry reload; no expired-deadline busy loop.

## Task 1: Reproduce isolation failures and define timing behavior

**Files:** tests/debounce.rs, server/async_tests.rs, new server/schedule.rs under crates/nitride-lsp.

- [x] Add framed RED regression: repeated changes to B plus unrelated responses must not postpone A's error diagnostics.
- [x] Add RED server-state regression: completing a request for A leaves not-yet-due B pending.
- [x] Add fake-time tests for independent quiet periods, shared reload gating, oldest-ready ordering/ties, resetting ready entries, forgetting closed documents and target/all expediting.

## Task 2: Integrate independent readiness

**Files:** server.rs, server/jobs.rs, server/schedule.rs, existing scheduling unit tests.

- [x] Implement the private timing unit and observe its tests pass.
- [x] Replace the global deadline/ready flag with per-file timing and a shared reload deadline.
- [x] Split due-time flushing, targeted request expediting and global barrier flushing; select oldest ready work in pump.
- [x] Preserve unsupported URI draining, cancellation, reload and shutdown behavior; run focused regressions and complete LSP tests.
- [x] Commit the validated scheduling change.

## Task 3: Review, verification and documentation

**Files:** docs/lsp-scheduling.md, docs/status.md, editor README and this plan.

- [x] Obtain independent full-change review; reproduce/fix actionable findings.
- [x] Verify default/native LSP, typed protocol parity, real CLI stdio, strict clippy, formatting and staged packaging tests.
- [x] Document independent timing, shared-input behavior and remaining synchronous/preemption limits; preserve verification evidence.
- [x] Commit and leave the feature worktree reviewable without disturbing main's local files.

## Completion and engineering rulings

The shared-input rule required narrowing blanket `.json` classification: ordinary
JSON edits/watches must stay document-local, while declared JSON inputs remain
registered even when a language cannot resolve. Classification is cached on open/
reload, not recalculated with filesystem probes per keystroke. Missing-input path
identity resolves through existing ancestors and supports deleted/aliased paths.

Independent review found one cold-start recovery gap: missing JSON metadata can
prevent creation of any LanguageState. A directed regression reproduced it;
registry declarations are now retained independently of resolution, preserved
through malformed syntax and dropped when projects leave the workspace. No other
findings or deferred minor issues remain.

All tasks are complete. Full workspace744passed/3ignored, nativeLSP222, native
CLI stdio, strict clippy, formatting and 22 packaging/tooling tests passed. These
are local results; hosted checks are reported on the follow-up PR, and manual
GUI acceptance is unverified. No parser preemption, merge or release is included.
