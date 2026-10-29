# Arena current-chunk lookup implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline and TDD.

**Goal:** Avoid repeated chunk-directory hashing after slot recycling while preserving stable identities and ownership.
**Architecture:** Add a private current-chunk identity check after the existing dense lookup and before the HashMap fallback. No stored cache, new ownership, ID renumbering or new dependencies. Measure against frozen Stage21; retain only repeatable gains. If the initial placement loses, test the same check before the dense path, then reject if neither placement gains reliably.
**Tech stack:** Rust 1.85/std. **Spec:** User's Next continues the approved benchmark-led parser optimization scope; this change's description is in chat.

## Global constraints
Preserve unrelated source, prior milestones and local artifacts; no commits/push/default cutover. Full chunk ID must match; a recycled vector slot cannot identify an old chunk. Empty/freed current slots, other live chunks and published chunks must retain existing behavior. Logical work, reference counts, cache ownership, allocation/budget accounting and concurrency contracts remain unchanged.

## Review focus
- Recycled slots must reject stale IDs even when their slot matches current.
- A freed or empty current slot must not panic or return a wrong identity.
- Other live chunks must retain the established directory fallback.
- Published chunk IDs must resolve through the published arena.
- ID exhaustion and independent/concurrent builders must retain existing limits and identity isolation.

## Task 1: Lookup prototype and performance gate
- [x] Write RED tests for the private current-chunk check on slot recycling, freed current slots and other/published chunks.
- [x] Implement the identity check and integrate into find_new; run GREEN and existing core/arena tests.
- [x] Build uninstrumented Release; two three-round alternating comparisons against Stage21 at 256KiB/4MiB/16MiB if the first pass gains. If negative, test alternate placement or restore exact Stage21.

## Task 2: Admission or restoration
- [x] Retained: full workspace/clippy/fmt/bootstrap/Python checks, 4334/1286 differential traces, recovered edit and both 100000-edit lifetime windows. Rejected: restore exact baseline and run restored checks.
- [x] Fresh final read-only review, one TDD fix pass for Important findings, evidence and exact source-overlay/binary audit.
- [x] Keep local artifacts; no integration and no Roslyn parity claim.

## Result
Retained the after-dense identity check; both full-parse batches improved every size. Full admission and independent review passed without blocking findings. A recovered-edit latency control did not reproduce the larger first-edit slowdown; its median was1.14% slower and is documented. One Minor non-current recycled fallback test gap is deferred. Source/binary exactness audited; see docs/stage22-evidence.md. All changes remain local.
