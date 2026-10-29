# Generated Syntax Execution Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline and TDD.

**Goal:** Remove structural Expr interpretation from eligible syntax bodies/tails while preserving current tree/Pratt semantics.
**Spec:** docs/superpowers/specs/2026-10-26-generated-syntax-design.md; user Proceed.
**Files:** core parser/shared/generated.rs owns context/table runtime; codegen/syntax.rs owns bounded callback emission; shared.rs integrates callbacks; profiler/script select them; generated seed stays fixed-point.

## Task1: Generation and integration
- [x] RED generation table/owner/tail/bounds regressions; generate bodies/tails without changing Expr data.
- [x] RED API/tree/precedence/mode/limit/repeat/edit tests; implement SyntaxContext and guarded dispatch using the existing tree builder.
- [x] RED profiler selection/count regression; opt in explicitly and keep default/lexical modes intact.
- [x] Rebuild bootstrap, focused tests and clean alternating Release exploration; retain only measured improvement.
- [x] Follow CPU evidence with a RED native-only trivia fast path: skip impossible comment comparisons while preserving original read reach/work; compare separately to structural-only generation.

## Task2: Verification and evidence
- [x] Complete suite, strict clippy, formatting, bootstrap and driver tests.
- [x] Single fresh whole-delta review; one TDD fix pass for Important findings, then full verification.
- [x] Freeze corrected profiler, three full cold parses per size and alternating comparisons to Stage18; complete corpora, recovered edit and Release reader windows.
- [x] Final source/executable audit, evidence, remaining Roslyn gap and exhaustive rulings. Preserve local work; no integration.

## Execution result

Locally complete and verified; Roslyn parity false. See docs/stage19-evidence.md. One whole-delta review, no actionable findings; all final gates/captures passed. CPU-guided trivia addition measured separately and together. No commits or integration.
