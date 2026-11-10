# Ordinary-choice dispatch evidence

Implemented and locally verified; Roslyn parity remains unmet. This checkpoint adds conservative, ordered eligibility routes for ordinary choices with at least four alternatives. A parse-local cache holds at most 256 KiB of resident plans and is dropped before snapshot publication. Active plans may briefly outlive cache eviction; this is temporary memory, not retained snapshot storage. Recovery probes retain their original recognizer and search-budget accounting.

Only proven failures are skipped. Unknown, nullable, recursive and predicate-led starts remain eligible. Batched skipped checks preserve logical work/depth limits. Speculative trivia lookahead restores parser state; failed lookahead falls back to ordinary matching so limit and grammar errors retain their original ordering.

Seconds, medians of three fresh processes per size, uninstrumented Release profiler, identical input hashes. Timed phase includes cold preparation, source, syntax and index construction. Stage16, original shared and Roslyn are preceding same-host frozen captures, not interleaved; timing drift is possible. Roslyn uses ParseText plus GetRoot, excluding input I/O and process launch.

| Target | Original shared | Stage16 | Stage17 | Total speedup | Roslyn | Stage17 / Roslyn |
|---|---:|---:|---:|---:|---:|---:|
| 0.25 MiB | 0.720 | 0.226 | 0.164 | 4.38x | 0.0646 | 2.54x |
| 4 MiB | 11.052 | 3.573 | 2.644 | 4.18x | 0.6181 | 4.28x |
| 16 MiB | 44.038 | 15.500 | 11.431 | 3.85x | 1.2889 | 8.87x |

The corrected source passes 285 workspace test executions (54 core), strict all-feature clippy, formatting, bootstrap, nine Python driver tests and diff whitespace checks. All 4,334 standard and 1,286 adversarial traces match the independent flat parser. Source/executable hashes agree across final reports.

The first recovered 16 MiB edit matches full legacy recovery at 101.190 ms, copies 1,537 source/syntax bytes, recomputes 63 coverage bytes and uses no fallback or maintenance. This is one validated sample, not p95 latency or a refreshed cold-recovery matrix. Release 100,000-edit windows retain 129,096 bytes for the latest snapshot and 137,704 bytes for the eight-reader union, below the 8,549,536-byte bound; both windows perform 155 maintenance passes.

One independent read-only review found a P2 error-order defect in speculative trivia. A focused regression failed before the fix and passed afterward; the full corrected suite and captures above then passed. There was no second independent post-fix review. No deferred minor findings.

A preceding alternating three-run exploration measured routed choices 1.07x faster than linear filtering at 256 KiB and 4 MiB. That exploration used a pre-review source checkpoint and is kept separately from these final numbers.

[Full-parse samples](stage17-full-parse.json), [standard traces](stage17-differential.json), [adversarial traces](stage17-adversarial.json), [first edit](stage17-first-edit.json), [lifetime](stage17-lifetime.json), [admission audit](stage17-admission.json), [paired exploration](stage17-paired-exploration.json).

The flat parser remains the compatibility oracle. The optimized shared API remains opt-in; no integration, hosted CI or cross-platform performance claim is made. Generated lexical execution is the next larger candidate for removing generic expression-interpreter overhead.
