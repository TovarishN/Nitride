# Trivia cache experiment: rejected

No cache was retained in active code. Four layouts were implemented and measured against the frozen Stage19 executable: a bounded direct-mapped successful-scan cache, an inline replay variant, a compact array plus terminal fast slot, and a final16-byte terminal-position marker. None demonstrated a reliable full-parse speed benefit on the admitted generated C# workload.

## Measurements

All runs used uninstrumented Release executables on identical generated inputs. Each size/backend has three fresh processes; order alternates each round, with no concurrent builds/tests/sampling. Earlier and later rounds have different host timing conditions, so compare within a round.

| Trial | Target | Stage19 | Candidate | Stage19 / candidate |
|---|---|---:|---:|---:|
| Initial array | 0.25 MiB | 0.121032s | 0.123107s | 0.983x |
| Initial array | 4 MiB | 1.979400s | 1.994674s | 0.992x |
| Inline replay | 0.25 MiB | 0.120021s | 0.126609s | 0.948x |
| Inline replay | 4 MiB | 1.942995s | 1.996855s | 0.973x |
| Terminal fast slot | 0.25 MiB | 0.120373s | 0.122029s | 0.986x |
| Terminal fast slot | 4 MiB | 1.987942s | 1.982023s | 1.003x |
| 16-byte marker | 0.25 MiB | 0.140328s | 0.140369s | 1.000x |
| 16-byte marker | 4 MiB | 2.167163s | 2.179634s | 0.994x |
| 16-byte marker | 16 MiB | 8.962828s | 9.125740s | 0.982x |

The initial array achieved roughly95.5% cache hits and used32KiB of entry storage, but that did not translate into elapsed-time savings; additional lookup/call/bookkeeping costs are a likely explanation. The compact terminal-array variant reduced entry storage to24,592 bytes; the final marker used16 bytes without heap allocation. High hit rates were not a substitute for measured elapsed-time gains. These results reject these layouts for this workload, not every possible trivia-cache design or grammar.

The experiments preserved logical recognition work in every generated-mode comparison. Focused tests exercised source-local read reach, EOF/UTF-8, strict/permissive comments, speculative work rollback, near-budget failure positions, physical guards, disabled compatibility/recovery/rope modes, budget shrinkage, comment-boundary edits and old readers. The array versions additionally tested collisions and maximum packed work costs.

Because performance admission failed, full4,334/1,286 candidate trace and100,000-edit lifetime gates were not run for the cache prototypes. No production-quality/cache-admission claim is made. The final active code exactly matches the already validated Stage19 source fingerprint; its complete compatibility/retention evidence remains applicable. Fresh restored-tree verification passed:305 workspace test executions, strict all-feature clippy, formatting, bootstrap, nine Python driver tests and an ordinary-allocator Release rebuild. Both source and rebuilt executable hashes exactly match Stage19. One independent read-only outcome/restoration review found no actionable issues.

## Preserved evidence

[Raw comparisons, counters and executable hashes](stage20-trivia-cache-experiment.json). Frozen executables and the hot-array/final-marker source deltas are preserved in `.superpowers/sdd/2026-10-26-trivia-cache/`. Intermediate binaries are experimental and must not be confused with the restored active parser.

The active optimized implementation remains [Stage19](stage19-evidence.md), whose recorded16MiB median is8.275s versus the preceding same-host Roslyn capture1.289s. The Roslyn gap remains unmet. The next evidence-backed candidates are memo-key hashing and repeated rule/alternative bookkeeping, which were prominent in the earlier CPU sample. No optimization gain is claimed for this experiment.
