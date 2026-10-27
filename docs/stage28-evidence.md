# Stage28 generated recognition after the first error

Stage26 reused the failed strict attempt's facts, but those facts end at the first error. Everything after it was still recognized by interpreted recovery. Profiling a 256 KiB one-error file showed where that work went. Rule probes cost about 76K work. One lookahead in the recovery VM, `Predicate(Ref(NamespaceMember))`, was evaluated at every following namespace for about 69K work each, roughly 25M of the 26.8M total.

A cold seed from a generated attempt now also carries the generated callbacks and the contiguous source. When a recovery probe, or a predicate that is a single syntax reference, has no stored fact for its (rule, position, power), it first recognizes that call with the generated parser. That pre-recognition is not charged to the recovery budget. It only records successful facts; failures leave nothing behind. The interpreted call then reuses the fact exactly as it reuses a seeded fact, so only that reuse is charged. Failed pre-recognitions are decided again by the interpreted probe, as before.

Recovery after edits, the interpreted `parse_snapshot` path and lexical rules are unchanged.

## Timings

main (`1cffb0e`) against this change, `--native-syntax --snapshot-recover`, uninstrumented Release. Each value is the median of three fresh processes, in alternating order, same host. This includes stage26 and stage27.

| Target | Pattern | main s | Now s | Speedup | Peak MiB main / now |
|---|---|---:|---:|---:|---:|
| 256 KiB | strict | 0.114 | 0.095 | 1.20x | 25 / 24 |
| 256 KiB | one | 0.938 | 0.152 | 6.15x | 32 / 35 |
| 256 KiB | dense | 5.155 | 4.238 | 1.22x | 40 / 45 |
| 256 KiB | junk | 1.397 | 0.617 | 2.26x | 47 / 40 |
| 256 KiB | truncated | 1.150 | 0.453 | 2.54x | 34 / 46 |
| 4 MiB | strict | 1.795 | 1.518 | 1.18x | 268 / 269 |
| 4 MiB | one | 14.401 | 2.397 | 6.01x | 344 / 432 |
| 4 MiB | dense | 106.250 | 88.522 | 1.20x | 378 / 439 |
| 4 MiB | junk | 21.353 | 9.346 | 2.28x | 479 / 517 |
| 4 MiB | truncated | 12.454 | 1.582 | 7.87x | 322 / 363 |
| 16 MiB | one | 74.637 | 10.162 | 7.34x | 1168 / 1609 |

Against stage27 alone, measured separately: 4 MiB one-error goes from 7.88 s to 2.40 s, 3.28x. Diagnostic and fallback counts are unchanged in every row. At 16 MiB with one error, counted work falls from 4,334,567,498 to 130,006,153.

Junk inputs, and the 256 KiB truncated input, still end in legacy fallback on a resource limit. They gain because the shared recovery attempt that precedes the fallback is cheaper. With dense errors, pre-recognition often fails at damaged positions, so the gain is small.

Main's dense work count differs between captures of the same binary and input: 1,809,201,868 in stage26 and 2,106,628,576 here. Randomized map ordering changes cache eviction under memo pressure. This variance predates this change.

The +38% peak memory at 16 MiB from stage26 seeding remains. Retained snapshot storage is unaffected.

## Validation

- 437 workspace test executions, plus strict all-target/all-feature clippy and fmt.
- A new regression in `incremental_recovery.rs` puts the error first in the file. It was RED with no generated calls during recovery and GREEN after, matching full legacy recovery for tree, diagnostics, repairs, coverage and damage.
- All 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, including the cold-snapshot check, with zero mismatches.
- Profiler SHA-256: `f9174a75ee102163383ca73cf9edf489a36402127d02d49e0893cc4d353e6a25`.

[Paired timings](stage28-paired.json), [standard traces](stage28-differential.json), [adversarial traces](stage28-adversarial.json).
