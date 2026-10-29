# Generated parse label cache experiment

> **For agentic workers:** Use superpowers:executing-plans inline and TDD.

**Goal:** Avoid repeated label interning HashMap lookups without changing label identity, numbering or semantics.
**Architecture:** Private direct-mapped32-slot cache with full static string pointer/length identity and nonzero u16 ID. Allocate once only for generated cold parsing; no cache for edits, default parsing or recovery probes. Existing HashMap remains canonical, IDs assigned lazily in the same order. Account actual cache size in the private bound and drop under reduced budgets. No codegen/API/dependency changes. Retain only repeatable three-size full-parse gains; otherwise restore exact Stage22.
**Spec:** User Next continues approved measurement-led optimization scope; concrete small-cache direction announced in chat.

## Global constraints
Preserve prior source/artifacts and local uncommitted scope. Preserve full pointer/length identity, zero empty label, u16 overflow diagnostics, logical work and read dependencies, recovery work semantics, cache ownership and bounds. No label renumbering/pre-registration. No default cutover or parity claim.

## Review focus
- Equal text at distinct static pointers must not alias, nor slices with a shared pointer and different lengths.
- Collisions must miss safely and preserve canonical IDs on fallback.
- Empty label remains0; overflow does not publish/cache an invalid ID.
- Tiny probe budgets must not retain the label cache; edited/default/probe modes stay uncached.
- Cache hits must leave parser work, furthest/read tracking and label order unchanged; budget-driven memo eviction may still need investigation at large inputs.

## Task1: Private cache prototype
- [x] RED pointer/length/collision/empty-ID/bounded-size tests and private parser budget/mode/overflow regression checks.
- [x] Implement LabelCache, generated-only allocation and exact budget accounting; GREEN focused tests.
- [x] Build uninstrumented Release and run three-round alternating Stage22/candidate full parses at256KiB/4MiB/16MiB. Repeat only if initial gains are credible; reject unrepeatable results.

## Task2: Admission or restoration
- [x] Retained: full suite/clippy/fmt/bootstrap/Python, full4334/1286 differential traces, recovered edit and bounded reader windows. Rejected: archive prototype, restore exact Stage22 source/binary, run restored full local gates and reuse full corpus evidence only by exact identity.
- [x] One independent final review of owned outcome/evidence. No integration; preserve experiment checkpoints and raw results.

## Result
Prototype rejected after the initial all-size capture:16MiB elapsed improvement~0.55% with~9x eviction events and changed work. Cache code archived and removed; active source and rebuilt Release exactly Stage22. Fresh restored310 tests and all local gates passed; prior full admission reused only by exact identity. Independent review confirmed restoration and recorded one deferred archive-only budget edge. No repeated/full candidate admission claimed. See docs/stage23-evidence.md.
