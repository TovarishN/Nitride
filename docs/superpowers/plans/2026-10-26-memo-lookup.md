# Memo lookup optimization implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline and TDD.

**Goal:** Reduce full-parse memo hashing overhead without changing parse results, logical work, cache ownership or budgets.
**Architecture:** First replace tuple hashing with a private same-size MemoKey hashed as one fixed eleven-byte buffer. Retain randomized standard HashMap and full-field equality. If performance fails, evaluate a keyed prehash shared across both generations, subject to the same budget and admission gate; otherwise restore exactly Stage19.
**Tech Stack:** Rust 1.85/std; no new dependencies.
**Spec:** User-approved next-step description in chat: trace hashing, optimize memo lookup, benchmark all three sizes and retain only repeatable gains.

## Global constraints
All work stays in this managed worktree, uncommitted. Preserve prior checkpoints. All key bits (u16 rule, u32 position, u32 minimum, u8 context) remain significant. Preserve recovery and interpreted paths, deterministic logical limits and bounded memory. No default cutover or Roslyn parity claim.

## Review focus
- Lexical namespace 0x80 must stay separate from Pratt contexts 0..2.
- Full-width positions/minimums must not truncate or collide semantically.
- Deliberate hash collisions must preserve full equality.
- Generation clearing must release the same node ownership and memory bound must reflect actual key size.
- Recovery and low-budget limits must keep diagnostics and equivalence.

## Task 1: Memo key experiment
- [x] Write private key tests for identity, namespace, full-width values, collisions and storage compatibility; run RED.
- [x] Implement private MemoKey and substitute in two memo maps and both lookup/insert paths; run GREEN and focused CLI tests.
- [x] Build uninstrumented Release and alternate frozen Stage19/candidate parses at 256 KiB, 4 MiB and 16 MiB. Preserve raw data and hashes; reject unrepeatable gains.

## Task 2: Admission or restoration
- [x] If retained: workspace tests, strict clippy, fmt, bootstrap, Python tests, complete standard/adversarial differential traces, recovered edit and reader lifetime checks. If rejected: restore exact Stage19 source and binary and run restored gates.
- [x] One fresh final review of owned delta and evidence, including all review-focus cases. Fix Important findings with regression checks.
- [x] Publish scoped evidence and remaining gap; final source/binary audit. Keep local checkpoints and scratch artifacts.

## Result
Retained same-size contiguous key hashing after both paired batches improved every size. All admission checks and final review passed. Prehash expansion was unnecessary. Source and active Release binary match frozen measured artifacts. See docs/stage21-evidence.md. Local scope preserved.
