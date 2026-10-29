# Predictive recognition prototype implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline; verify with TDD.

**Goal:** Reduce full-parse recognition work toward the measured Roslyn baseline while preserving grammar behavior, old readers and Rust-only deployment.
**Architecture:** Compile conservative character-start and lexical metadata from prepared grammar expressions. Skip a rule only when its first character cannot match; unknown, nullable and non-ASCII ambiguity remain eligible. Keep flat recognition unchanged as an oracle. Cold planning costs and first edits remain measured.
**Spec:** Approved direction in this chat: lexical recognition and predictive dispatch first, preserving longest-match, precedence, ambiguity and predicates. No claim of Roslyn parity until measured.
**Tech Stack:** Existing Rust/std and Python; no new dependencies.

## Constraints and review focus
- Preserve all existing dirty shared-subtree work and immutable baseline reports; no staging, commits, push or merge.
- Test nullable alternatives, recursive starts, Unicode, comments, keyword boundaries, all parser modes and multi-piece edits; no unsafe early commitment.
- Track any metadata storage in physical lifetime accounting; avoid global unbounded caches or shifting cold cost into first edit.
- Recovery probes and limits must remain bounded; report any behavioral difference against the independent flat oracle.

## Task 1: Conservative prepared starts and lexical metadata
- [x] Add RED regression for reducing work on impossible alternatives, with exact flat-tree parity.
- [x] Add metadata tests covering literals, classes, complements, nullable sequences, predicates, references and recursion.
- [x] Implement private compiled metadata; retain it per snapshot and reuse across edits, with allocation accounting.
- [x] Integrate rule rejection after ordinary trivia handling; keep the flat parser unchanged.
- [x] Run focused tests, the workspace suite, formatting and strict all-feature clippy.

## Task 2: Measure and refine recognition
- [x] Freeze a new ordinary-allocator Release executable and compare identical full-parse inputs with the previous frozen executable.
- [x] Record parse work and allocation/source/index costs separately; use the existing Roslyn report only on matching hashes.
- [x] Apply lexical hot-path specialization supported by measurements, with RED-to-GREEN regressions.
- [x] Re-run independent standard/adversarial manifests and meaningful first-edit/lifetime checks.

## Task 3: Review and evidence
- [x] Obtain one whole-change review of only this prototype delta, address Important findings, and verify.
- [x] Publish raw reports and remaining Roslyn gap; leave all local work available in the managed worktree.

## Execution result

Prototype complete and locally verified; Roslyn parity remains unmet. See docs/stage16-evidence.md. One independent review finding and three additional concrete compatibility checks were addressed with RED-to-GREEN regressions. The rejected one-of4334 recovery mismatch and its raw evidence are preserved in the plan workspace. No commits or integration.
