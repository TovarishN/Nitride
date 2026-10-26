# Fused lexical execution implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline and TDD.

**Goal:** Execute first-byte-determined ASCII lexical decisions with exact work/depth/read metadata and conservative fallback.
**Architecture:** Bounded compile-time evaluator -> immutable per-rule ASCII decision table -> LexicalContext replay after existing root enter guard. No dynamic cache or grammar change. **Tech Stack:** Rust1.85/std/serde_json, existing generated scanner API.
**Spec:** docs/superpowers/specs/2026-10-26-fused-lexical-design.md, user Proceed.

## Global constraints
Preserve user-approved scope, actual stack entry guards, logical limits, observations, UTF8/EOF/Unicode behavior, exclusions and diagnostic fallback. Only first-byte-determined zero/one-byte traces, no executed runtime refs, evaluator depth64/node4096 bounds. Keep private budgets and default/edit/probe paths unchanged. Preserve prior work and local artifacts, no commits/push/default integration.

## Files and interfaces
- New codegen src/lexical/fused.rs: private Decision metadata, plan(&Value)->Option<[Decision;128]>, bounded evaluator and pure tests.
- Core src/lexical.rs: opaque public AsciiDecision and LexicalContext::ascii_decision(&[AsciiDecision;128],u32)->Option<bool>, exact replay/fallback tests.
- Codegen src/lexical.rs: emit per-rule immutable table and root expression fast/fallback branch after c.enter(d).
- Codegen tests/generation.rs: conservative rule eligibility and emitted tables/guards/determinism.
- Prepare src/grammar_seed.rs: regenerate only through the bootstrap example after generator output changes.
- CLI tests/native_lexers.rs: ASCII, Unicode escapes/supplementary, keyword exclusions, modes, low limits and edit-boundary read reach parity.

## Review focus
- Failed literal read reach can extend beyond one ASCII byte and through EOF; reads must match original clipped reach.
- Choices/predicates and optional/failed repeats must preserve exact short-circuit work/depth/order and restoration.
- Tight work/frame/u64 arithmetic must decline before touching state, then old expression reaches the exact original failure.
- No skipped physical entry guards; executed refs and suffix-dependent/multi-byte matches must decline.
- Tiny/malformed callback metadata, non-ASCII/EOF and compilation resource bounds must remain safe.

## Task1: Profile and decision compiler/runtime
- [x] Freeze Stage22 and refresh symbolized sample; restore ordinary Release exactly before source edits.
- [x] Fresh builder check of written design; settle implementation calls and record rulings.
- [x] RED evaluator tests: ordered choices, classes/literals, predicates, repeat cases, unsupported refs/suffixes, limits. Implement bounded Decision/plan and GREEN.
- [x] RED runtime replay/reference comparison across128 bytes, read clipping, EOF/Unicode, work/frame boundaries and invalid metadata. Implement opaque metadata/replay and GREEN.

## Task2: Generated integration and performance gate
- [x] RED generation and full-parser regressions; emit tables and unchanged fallback with guards/exclusions. GREEN focused core/codegen/CLI tests.
- [x] Build uninstrumented Release; three-size alternating comparison and repeat if promising. Report counters and code/table size; no gain means exact restoration and local verification rather than full candidate admission.

## Task3: Admission and evidence
- [x] Retained candidate: full workspace, strict clippy/fmt/bootstrap/Python, complete standard/adversarial corpora, recovered edit and both lifetime windows; frozen final source/executable hashes.
- [x] One independent final review of owned delta and evidence; one fix pass for Important findings with meaningful regression verification.
- [x] Save raw/summary evidence, all scope rulings/deferred minors and exact audit; preserve all local checkpoints, no integration.

## Result
Retained eighteen static CSharp4 ASCII decision tables after two paired batches improved all sizes. All321 workspace executions, strict local gates, full4334/1286 corpora, recovered edit and both100000-edit windows passed; independent review found no issues. Source/binary hashes exactly match captured candidate. Nitrogen target not yet met. Evidence: docs/stage24-evidence.md. Local checkpoints preserved, no integration.
