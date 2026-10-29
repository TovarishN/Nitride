# Generated lexical execution evidence

Implemented and locally verified as an opt-in API; Roslyn parity remains unmet. Generated scanners use the admitted grammar's ordered choices, possessive repeats, predicates and exclusions. Eligibility is transitive, acyclic and bounded; owner-bearing and Pratt rules remain interpreted. ASCII character classes use bitsets and Unicode ranges use normalized interval lookup. The ordinary Expr grammar and flat compatibility parser are retained.

`parse_snapshot_compiled(&nitride_cli::csharp4::LEXERS, source, limits, mode)` enables scanners for cold contiguous input. Edited ropes and token_children mode use the existing interpreter. There is no scanner scratch state or global cache retained by a snapshot. SnapshotStats adds native call/work counters; this is an additive API change that can require updating external exhaustive struct literals.

## Full-parse timings

Seconds, medians of three fresh processes, ordinary allocator and identical input hashes. The phase includes source construction, metadata preparation, syntax and index. Stage17, original shared and Roslyn are preceding same-host captures, not interleaved. Roslyn times ParseText plus GetRoot; process launch and input I/O are excluded. Host timing drift is possible.

| Target | Original shared | Stage17 | Native | Total speedup | Roslyn | Native / Roslyn |
|---|---:|---:|---:|---:|---:|---:|
| 0.25 MiB | 0.720 | 0.164 | 0.156 | 4.61x | 0.0646 | 2.42x |
| 4 MiB | 11.052 | 2.644 | 2.407 | 4.59x | 0.6181 | 3.90x |
| 16 MiB | 44.038 | 11.431 | 9.798 | 4.49x | 1.2889 | 7.60x |

A separate final comparison alternates the frozen Stage17 and corrected native executables, three fresh processes per backend per size:

| Target | Stage17 | Native | Speedup |
|---|---:|---:|---:|
| 0.25 MiB | 0.176 | 0.151 | 1.17x |
| 4 MiB | 2.830 | 2.427 | 1.17x |
| 16 MiB | 11.956 | 9.895 | 1.21x |

The C# generated source is 1,073,816 bytes and contains 59 scanner functions. The profiler executable grows from 1,452,560 to 2,542,640 bytes. Static code/table bytes and OS mappings are outside the dynamic storage counters. Generated execution can perform more logical checks than Stage17's choice pruning while still taking less elapsed time; work counters are not CPU instruction counts.

## Compatibility and limits

The corrected source passes 295 workspace test executions (57 core), strict all-feature clippy, formatting, the regenerated bootstrap fixed point, nine Python driver tests and diff whitespace checks. All 4,334 standard and 1,286 adversarial edit traces pass with native cold parsing explicitly requested and every resulting edit validated against independent legacy recovery. Frozen C# strict parsing, all parse modes, Unicode/supplementary and escaped identifiers, exclusions, low limits and compiled-initial edits have dedicated tests.

One independent read-only review found a P2 limit defect: native scanning could publish a deep failed-alternative fact that later interpreted edits did not need. The regression failed before the fix and passed after cached over-depth facts were declined in favor of ordinary recognition. Real execution still enforces limits. Full verification and measurements followed the fix; there was no second independent post-fix review. No deferred minor findings.

## Recovery and retained readers

A compiled parse error retries the established entry point to preserve error ordering and recovery search budgets. This has a cost: the single damaged 16 MiB cold capture took 74.163 s versus the preceding Stage17 single capture of 67.164 s. These are single samples, not a new recovery distribution. This step optimizes valid cold parsing; it does not claim faster cold recovery.

The validated first recovered edit took 102.607 ms, copied 1,537 source/syntax bytes and recomputed 63 coverage bytes, with no fallback or maintenance. This is one sample, not p95 latency. Edited ropes stay interpreted. The existing Release 100,000-edit zero/eight-reader checks retain 129,096 bytes for the latest snapshot and 137,704 bytes for the eight-reader union under the 8,549,536-byte bound. This long lifetime run exercises the interpreted edit path; the compiled-initial boundary/old-reader test is separate.

[Full-parse raw samples](stage18-full-parse.json), [alternating comparison](stage18-paired.json), [standard traces](stage18-differential.json), [adversarial traces](stage18-adversarial.json), [first recovered edit](stage18-first-edit.json), [lifetime](stage18-lifetime.json), [admission audit](stage18-admission.json).

All final source/executable hashes agree. Local changes remain uncommitted and opt-in. Hosted CI, cross-platform measurements and default-backend integration are not claimed. Remaining overhead includes syntax-rule interpretation, speculative alternative execution and tree/index construction; closing the Roslyn gap needs broader specialization than lexical scanning alone.
