# Cooperative Parse Cancellation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Release the single LSP worker promptly when a document parse/recovery job becomes obsolete.

**Architecture:** Scoped cancellation context, captured by batched pollers and inherited on spill threads, propagates a distinct cancellation outcome through parse/recovery/fallback boundaries. The existing LSP generation/flag checks remain the commit barrier.

**Tech Stack:** Existing Rust standard-library ownership/atomics and native benchmark tooling; no new dependencies or Limits changes.

**Spec:** docs/superpowers/specs/2026-11-09-cancellation-design.md

## Global Constraints
- Preserve existing APIs, uncancelled outputs, limits, stack-safety and typed behavior.
- No partial snapshots/results or cancellation diagnostics published to the client.
- Request cancellation alone must leave required diagnostics running.
- Nested scopes restore on unwind; spill threads inherit cancellation.
- Poll by batched work, not per byte; document uninterruptible operations honestly.
- Keep user main workspace unchanged; use matched prebuilt binaries without competing tests/builds.

## Review Focus
- A cancelled strict/recovery attempt must not restart flat fallback or return a partial success.
- Child spill threads must carry every enclosing token, while unrelated threads remain independent.
- Large virtual work charges and recovery resynchronization cannot indefinitely skip polling.
- Cancelling one version must not poison future edits or invalidate old immutable readers.
- A cancelled waiter must not stop a job still needed for diagnostics or another request.

### Task 1: Core control and parser/recovery checkpoints
- [x] RED scope/restore/poll tests and deterministic strict/recovery/edit cancellation regressions.
- [x] Implement token/context/poller, parser/recovery checkpoints and spill inheritance.
- [x] Verify cancellation propagates through fallback and snapshots stay transactional.

### Task 2: LSP integration
- [x] RED controlled real-parser worker cancellation followed by latest-job success.
- [x] Scope obsolete-job flags and prevent expensive post-cancel fallback; retain generation barriers.
- [x] Verify waiter cancellation and existing protocol/lifecycle suites.

### Task 3: Measurement and integration
- [x] Measure gated signal-to-return latency and matched normal/false-token overhead.
- [x] Run full suites, strict quality checks and one independent whole-branch review.
- [x] Fix Important findings with RED→GREEN regressions, document limitations, commit and open PR.

Measured tradeoff: 18/18 cancellation trials passed (0.08–0.66 ms). Five-pair
rounds show +10.6%/+11.8% cached and +6.9%/+6.0% fresh analysis time.
All raw evidence is retained; reducing polling overhead remains a follow-up.
Three Important review findings received RED→GREEN fixes; no deferred findings.
Final-source verification: workspace767 pass/3 ignored, native LSP228, native
CLI6, required Python39, strict Clippy/format/version/patch checks all pass.
