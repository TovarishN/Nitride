# Packaged Editor Release Checkpoint Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans for inline execution.

**Goal:** Validate the merged engine in a real packaged VS Code extension and prepare a reviewable 0.4.0 release checkpoint.
**Architecture:** Extend the existing disposable extension-host harness with typing, recovery and large-document freshness checks. Build the native PGO VSIX from a clean committed candidate; retain hashes and raw reports. Version, notes and evidence travel in a release PR.
**Tech Stack:** Existing Rust native release scripts, Python unittest, Node/VS Code extension-host APIs.
**Spec:** User-approved validation/release scope recorded below; no engine representation redesign.

## Global Constraints
- Start at merged PR #50 (`4d2be3f`), keep the main workspace and user VS Code profile untouched.
- Use disposable workspaces, extension directory and user profile for desktop tests.
- Validate current diagnostics/features, not an old marker or stale document incarnation.
- PGO package uses clean source provenance and the pinned Rust compiler.
- Prepare 0.4.0 as a minor checkpoint (builder choice); no tag/publish/merge in this task.
- Distinguish automated API observations from manual visual/painted-frame acceptance.
- Preserve all successful and failed evidence, with source/package/executable identities.

## Review Focus
- Large-document mode must explicitly require the repository languages; ordinary mode stays small.
- Reacquire documents after extension routing; assert final revisions with unique names or fold positions.
- Typed text must use the editor command path, and malformed text must recover after restoration.
- Timeout/interruption must reap only the harness-owned desktop process tree.
- Candidate packages must contain native PGO server/version metadata and omit component payloads.

### Task 1: Extend acceptance coverage
Files: scripts/editor_acceptance.py, scripts/test_editor_acceptance.py, editors/vscode/acceptance.cjs.
- [x] RED fixture tests for isolated >=4 MiB valid C# source, stable small default and invalid option pairing.
- [x] Add opt-in --large-document fixture/environment support and launch-to-readiness telemetry.
- [x] Add editor typing, broken/restore and large folding/diagnostic/final-revision checks.
- [x] Run Python/Node/extension checks; investigate actual failures before fixes.

### Task 2: Prepare and validate candidate
Files: Cargo.toml, Cargo.lock, editors/vscode/package*.json, README.md, docs/release-0.4.0.md.
- [x] Synchronize 0.4.0 and write release notes from merged changes; commit candidate for clean provenance.
- [x] Build/train/package darwin-arm64 PGO with existing scripts; run full/native/quality checks.
- [x] Run packaged Portable and all-language/large-document desktop acceptance sequentially after builds finish.
- [x] Record timings as observations, hashes, passed scenarios and explicit unverified boundaries.

### Task 3: Review and handoff
- [x] One independent whole-change review; reproduce/fix material findings with directed regressions.
- [ ] Commit evidence and open release PR; run non-publishing three-platform rehearsal if practical.
- [ ] Keep main workspace unchanged and branch/worktree clean; report concrete release readiness and outstanding gates.

Ruling: keep VS Code default5,000-fold cap; use a unique leading comment fold
for final large-file freshness, rather than an end-of-file fold excluded by the
client limit. Actual desktop attempt1 failed before correction; final two
profiles pass with the same harness hashes. Failed evidence retained. No product
regression observed. Local artifacts identify clean candidate6c90667; later
harness/evidence-only commits do not change its packaged engine. Final-head
nonpublishing three-platform rehearsal remains a separate PR gate.

Snapshot at the local-evidence source commit: implementation, local checks and
review are complete. The remaining PR/rehearsal operational gates are reported
in the PR body after that commit, keeping the native rehearsal source immutable.
