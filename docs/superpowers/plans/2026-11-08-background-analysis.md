# Background Analysis Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Keep the normal LSP event loop responsive during document analysis, with fresh results and prompt cancellation of waiting requests.

**Architecture:** One worker executes owned jobs; main-thread state validates generation stamps, publishes results and resolves bounded deferred requests. Previous analyses and loaded languages are shared with Arc. Zero debounce stays synchronous.

**Tech Stack:** Rust std threads/channels/atomics, existing LSP and runtime snapshots.

**Spec:** `docs/superpowers/specs/2026-11-08-background-analysis-design.md`

## Global Constraints

- One outstanding worker job; at most 256 deferred requests; no new dependencies.
- Only main thread writes RPC and mutates documents/registry.
- No copied flat tree for a job; retain previous analysis by Arc.
- Reject stale results by document incarnation and workspace generation.
- Cancellation does not preempt an in-flight parser; preserve finite core limits.
- Preserve zero-debounce scripts, supported request behavior, and shutdown barrier.
- Keep separate from the completed milestone PR; no merge or release.

## Review Focus

- Close/reopen with reused version must not accept old results.
- Invalid/stale changes must not cancel a valid analysis or waiting request.
- Language changes must invalidate results even before reload finishes.
- EOF/exit, worker panic and cancellation must not strand requests or deadlock.
- Unserved languages and workspace requests must settle without retry loops.

## Task 1: Owned job and completion boundary

**Files:** create `crates/nitride-lsp/src/background.rs`; modify `lib.rs`, `registry.rs`, `server.rs` analysis map.

**Interfaces:** `Job` owns ticket/stamp, path/text, Registry, buffers, previous Arc<Analysis>, cancellation flag. `Completion` carries ticket/stamp/URI and analyzed/cancelled/failed outcome. `Worker::spawn(callback)` owns a single job channel. `submit` is nonblocking under one-outstanding invariant. Worker catches panics and always reports completion.

- [ ] Write failing gated tests for job execution, cancellation before start, panic completion, and retained prior analysis/language owners.
- [ ] Derive Clone for registry routing/state containers; store cached analyses as Arc without cloning tree payloads.
- [ ] Implement worker and owned job execution, compare results with synchronous analysis and test thread-safety.
- [ ] Verify and commit the worker boundary.

## Task 2: Versioned background document scheduling

**Files:** `server.rs`, `background.rs`, new server background tests.

**Interfaces:** Input event adds completion and explicit EOF. Positive debounce starts worker; generations and one active ticket guard publications. `flush` prepares registry synchronously then marks batch ready and pumps one worker job. Completion validates, commits/publishes and pumps next ready job. Synchronous `analyze` delegates to the same commit path. Diagnostics include version for open documents.

- [ ] Add gated RED tests proving unrelated responses during blocked analysis and preventing old publications after edit/close/reopen/reload.
- [ ] Add document/workspace generations, active cancellation and ready-batch scheduling; avoid spinning on an expired deadline while worker is active.
- [ ] Handle cancelled/failed completions and explicit EOF; prove old owner release after workers finish.
- [ ] Verify scheduling/incremental/LSP wire tests and commit.

## Task 3: Deferred freshness barriers, cancellation and shutdown

**Files:** `server.rs`, optionally focused `requests.rs` if dispatch bookkeeping needs separation; background server tests.

**Interfaces:** Supported document methods defer on pending/in-flight analysis; workspace/symbol waits for the whole batch. Queue entries hold full request, target URI and generation. RequestCancelled=-32800, ContentModified=-32801, ServerCancelled=-32802. Normal response dispatch replays settled requests without a synchronous parse. Shutdown drains accepted work before acknowledging.

- [ ] Write RED gated tests for cancellation exactly once, string/numeric ID distinction, current-output parity, document/workspace invalidation, unserved language, 256-request bound, shutdown and worker panic.
- [ ] Implement barriers and invalidation using the existing request handlers; unknown methods remain immediate.
- [ ] Ignore post-shutdown changes and reject new requests; allow exit and complete accepted work before shutdown response.
- [ ] Verify default/native LSP and real CLI binary integration; commit.

## Task 4: Aggregate review and documentation

**Files:** status/editor README/scheduling docs; plan and evidence notes.

- [ ] Run formatting, strict clippy, both LSP feature configurations, CLI stdio and packaging tests.
- [ ] Obtain one independent complete-change review; reproduce and fix actionable findings, then rerun affected suites.
- [ ] Record exact scope, synchronous remaining phases, cancellation semantics and test evidence; avoid a GUI latency claim.
- [ ] Check PR #43 hosted validation, keep fixes there if needed, and leave the follow-up as reviewable commits.
