# Measured failed-alternative optimization plan

> **For agentic workers:** Use superpowers:executing-plans inline and TDD.

**Goal:** Reduce node construction in expensive failed syntax trials while retaining actual grammar recognition and Roslyn as target.
**Architecture:** Generic two-reference/lexical-marker selector -> bounded heuristic -> recognition-only virtual syntax -> replay real construction on success. Existing lexical payload and parsing decisions remain real.
**Spec:** docs/superpowers/specs/2026-10-26-predictive-alternatives-design.md. **Tech:** Rust1.85/std, current generator/runtime APIs.

## Constraints and files
No hard-coded C# rule IDs/names, no dynamic cache or private budget growth. Only generated/default-limit cold mode. Preserve lexical ownership, stack guards, choices, labels, canonical shapes and diagnostics; virtual handles cannot persist. Keep local workspace/checkpoints, no commit/push.
Core shared.rs owns mode/selection/trial scope and protects facts/memos; children.rs adds a separate first-handle counting sink; arena.rs defines/respects reserved virtual handle; generated.rs adds default-compatible descriptor metadata. syntax/shared.rs and profiler expose trial counters. Codegen syntax/trials.rs selects metadata and syntax.rs emits it. Tests in core/generation/CLI plus regenerated grammar_seed.rs through bootstrap.

## Review focus
- Virtual syntax cannot enter real lexical payload, memo success, index facts or final trees.
- Empty/transparent aliases and nested Pratt must retain bool/position/count behavior despite virtual shapes.
- Trial failure uses actual grammar furthest/read/depth; success restores position/trail/checkpoint and builds actual syntax, with all work charged.
- Custom limits and nondefault AST modes disable trials; normal probes/edited ropes remain unchanged.
- Marker absence is a heuristic only; comments, long prefixes and UTF8 cannot falsely decide recognition.

## Task1: Baseline and diagnostic spike
- [x] Freeze Stage24 and refresh paired Roslyn/cold+warm controls at three sizes.
- [x] Separate temporary counters by rule/choice/Pratt with inclusive work/node/release scope. Verify recognition counters, archive overlay and restore exact Stage24.

## Task2: Small production experiment
- [x] Fresh builder check of concrete design; settle implementation calls before code.
- [x] RED virtual handle/sink/fact/memo safety and recognition-mode parity, then minimal implementation and GREEN.
- [x] RED generic marker metadata and trial failure/replay/error/activation tests; wire descriptor and counters, GREEN whole-parser focused comparisons.
- [x] Bootstrap seed regenerate/check, strict clippy/fmt and uninstrumented paired benchmark at all sizes; repeat promising gains. Reject/restore if no convincing result.

## Task3: Admission and final review
- [ ] Full workspace/local gates, complete4334/1286 traces, recovered first edit, bounded reader windows. Compare edit latency if a material new regression appears.
- [ ] One independent final owned-delta/evidence review and one regression fix pass for Important findings.
- [ ] Publish scoped raw/summary evidence, exact source overlay/binary audit, current Roslyn gap and all rulings/deferred minors. Preserve local work without integration.

## Actual outcome
Task2 rejected after two paired timing batches; complete source/executable experiment archived and exact Stage24 restored. Candidate admission gates in Task3 were intentionally skipped, not passed. Final review supports rejection; fresh restored local gates and exact restored hashes (frozen executable copied back; rebuild byte difference documented) recorded in stage25-admission.json. No integration requested.
