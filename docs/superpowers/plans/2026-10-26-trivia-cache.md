# Bounded Trivia Cache Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline and TDD.

**Goal:** Reuse source-position trivia scans in generated mode while preserving exact logical work, read dependencies and failure diagnostics.
**Spec:** docs/superpowers/specs/2026-10-26-trivia-cache-design.md, user Proceed.
**Files:** parser/shared/trivia.rs owns bounded entries/tests; shared.rs integrates hit/miss scanning; syntax/shared.rs and profiler expose telemetry; generated_syntax.rs tests whole-parser semantics.

## Task1: Cache and scanner integration
- [x] RED bounded map/collision/policy/resize tests; implement lazy direct mapping (archived prototype).
- [x] RED replay/read/limit/guard tests and focused integration checks; prototypes archived after performance rejection.
- [x] RED telemetry and whole-parser work/read/edit checks; prototype changes removed from active source.
- [x] Four measured layouts failed admission; no cache retained.

## Task2: Verification and evidence
- [x] Full restored-tree suite/clippy/fmt/bootstrap/Python/release checks passed; exact Stage19 source/binary restored.
- [x] One fresh outcome/restoration review found no actionable issues.
- [x] Frozen candidate/baseline alternating full parses completed at all sizes; full candidate corpus/recovered-edit/lifetime admission intentionally skipped after failed performance gate. Active Stage19 evidence remains valid by exact source/binary identity.
- [x] Final source/executable identity audit and experiment evidence complete; all checkpoints preserved without integration.

## Execution result

Experiment complete; cache rejected for lack of reliable elapsed-time benefit. Active source and rebuilt profiler exactly match Stage19. Focused prototype validation passed; full cache admission was not run. See docs/stage20-evidence.md. No production changes or integration.
