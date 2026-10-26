# Generated syntax execution and native trivia evidence

Implemented and locally verified as an opt-in API; Roslyn parity remains unmet. Structural callbacks replace ordinary sequence, repeat, label, literal and reference interpretation. Owner-bearing bodies and infix tails preserve precedence context and the original root-sequence tick omission. Existing runtime code still selects Pratt alternatives/winners, allocates nodes, handles aliases/exclusions and publishes indexes. Unsupported expressions use bounded static Expr fallbacks. Eligibility is capped at depth64,4096 expression nodes per rule and1,000,000 globally without expanding syntax references.

`parse_snapshot_generated(&nitride_cli::csharp4::PARSER, source, limits, mode)` enables generated syntax and Stage18 lexical scanners. The Stage18 `parse_snapshot_compiled` API remains lexical-only. Default APIs, edited ropes and recovery probes keep their interpreted path. No callback scratch state is retained by snapshots. SnapshotStats gains body/tail/trivia activation counters; exhaustive external struct literals can require updating.

Native-mode trivia additionally skips impossible comment-prefix comparisons after reading a non-slash character. It explicitly retains the original two-byte observed reach, logical work and comment/error policy. Tests compare ASCII, Unicode, EOF, whitespace, comments and errors; the optimization is gated off for compatibility parsing and recovery probes.

## Cold valid full parses

Seconds, medians of three fresh processes, ordinary allocator and byte-identical inputs. The phase includes source construction, metadata, syntax and index. Stage18/original shared/Roslyn are preceding same-host frozen captures, not interleaved; host timing drift is possible. Roslyn times ParseText plus GetRoot, excluding input I/O/process launch.

| Target | Original shared | Stage18 | Current | Total speedup | Roslyn | Current / Roslyn |
|---|---:|---:|---:|---:|---:|---:|
| 0.25 MiB | 0.720 | 0.156 | 0.124 | 5.79x | 0.0646 | 1.93x |
| 4 MiB | 11.052 | 2.407 | 2.032 | 5.44x | 0.6181 | 3.29x |
| 16 MiB | 44.038 | 9.798 | 8.275 | 5.32x | 1.2889 | 6.42x |

The final alternating comparison uses three fresh processes per backend per size, with no concurrent builds/tests/captures:

| Target | Stage18 lexical | Generated syntax + trivia | Speedup |
|---|---:|---:|---:|
| 0.25 MiB | 0.153 | 0.130 | 1.17x |
| 4 MiB | 2.444 | 2.099 | 1.16x |
| 16 MiB | 9.865 | 8.469 | 1.16x |

All final cold/paired samples preserve Stage18 logical work counts exactly. Logical checks are not CPU instruction counts. The generated C# source contains 255 body callbacks, 33 tail callbacks and 59 lexical scanners in 1,320,522 bytes. The profiler executable grows from 2,542,640 to 3,155,088 bytes; static code/tables and OS mappings are outside dynamic storage counters.

## Attribution and next costs

A preceding alternating exploration separated structural generation from native trivia: at4MiB, frozen Stage18 took2.3205s, structural-only2.2325s, same-build lexical-only2.2954s and syntax+trivia1.9858s. These are intermediate checkpoints, separate from the final measurements above. Their binaries/source identities and raw samples are preserved.

A separate pre-review structural-only CPU sample used symbols and frame pointers. Hashing, string comparisons, trivia, Pratt/rule handling and arena reference bookkeeping appear prominently in the flat sample ranking. This is diagnostic evidence, not a CPU-time percentage breakdown or a timing benchmark; its checkpoint/build differs from the final optimized executable. It motivated the trivia change and points toward token/trivia sharing and memo lookup costs before broader code generation.

## Validation and review

The corrected source passes 305 workspace test executions (60 core), strict all-feature clippy, formatting, the regenerated bootstrap fixed point, nine Python driver tests and diff whitespace checks. The entire previous grammar/lexical bootstrap prefix is unchanged; syntax callbacks are appended. Frozen C# strict trees, precedence/right associativity, all parse modes, labels/aliases, low work/depth limits, repeat rollback/error cleanup, actual stack guard accounting, bounded eligibility, compiled-initial edits and old readers have focused coverage.

All 4,334 standard and 1,286 adversarial edit traces pass with generated cold parsing requested, with every resulting edit validated against independent legacy recovery. One fresh read-only whole-delta review found no actionable defects; the reviewer did not run verification or assess numerical admission. No deferred minor findings.

## Recovery and retained readers

Cold generated errors retry the established parser to preserve diagnostics and recovery search budgets. The single damaged16MiB cold sample is 72.091s versus the preceding Stage18 single sample 74.163s; these are not a new recovery distribution or a general recovery speedup claim. Duplicate invalid-input recognition remains a cost of this opt-in path.

The validated first recovered edit took 100.491ms, copied 1,537 source/syntax bytes, recomputed63 coverage bytes and used no fallback or maintenance. This is one sample, not p95. The existing interpreted edit path passes Release100,000-edit zero/eight-reader checks: latest129,096 bytes, eight-reader union137,704 bytes, bound8,549,536 bytes. Compiled-initial edit/old-reader coverage is tested separately; edited ropes do not execute generated callbacks.

[Full-parse samples](stage19-full-parse.json), [final alternating comparison](stage19-paired.json), [standard traces](stage19-differential.json), [adversarial traces](stage19-adversarial.json), [first recovered edit](stage19-first-edit.json), [lifetime](stage19-lifetime.json), [admission audit](stage19-admission.json), [structural exploration](stage19-structural-exploration.json), [trivia exploration](stage19-trivia-exploration.json), [diagnostic CPU sample summary](stage19-cpu-sample.json).

All final timing/corpus/edit source and executable hashes agree. Exploratory/diagnostic checkpoints are intentionally separate. Changes remain local and uncommitted; no default cutover, integration, hosted CI or cross-platform performance claim is made.
