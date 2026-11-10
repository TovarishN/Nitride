# Stage26 seeded cold recovery

A cold recovering parse first attempts a strict parse. Before this change a syntax failure discarded that attempt: the generated path reran an interpreted strict attempt and then recovered the whole file with interpreted probes. Now the failed attempt's recognition facts become a seed. Each fact root becomes an owner, every other node is reclaimed and the seed is compacted. Recovery probes reuse the seed exactly as an edit reuses a previous snapshot's facts. Seeded facts still reachable from the recovered root stay in the published index for later edits.

The generated path no longer runs a second interpreted strict attempt after a syntax failure. Non-syntax errors (limits, stack) keep the existing fallback. The interpreted `parse_snapshot` recovering path is seeded the same way. Probe recognition is otherwise unchanged. A reused fact costs a single work step instead of its full recognition, so recovery uses less of its search budget, as warm edits already do.

## Full-parse timings

Seconds: main (1cffb0e) against this change, `--native-syntax --snapshot-recover`, uninstrumented Release allocator. Each value is the median of three fresh processes, in alternating order. Same host, generated C# inputs and damage patterns as earlier stages. Peak memory is the process maximum resident set.

| Target | Pattern | main | Seeded | Speedup | Peak MiB main / seeded |
|---|---|---:|---:|---:|---:|
| 256 KiB | strict | 0.114 | 0.114 | 1.00x | 23 / 23 |
| 256 KiB | one | 0.937 | 0.537 | 1.74x | 29 / 37 |
| 256 KiB | dense | 5.141 | 5.305 | 0.97x | 39 / 42 |
| 256 KiB | junk | 1.391 | 1.000 | 1.39x | 42 / 42 |
| 256 KiB | truncated | 1.147 | 0.464 | 2.47x | 38 / 37 |
| 4 MiB | strict | 1.794 | 1.791 | 1.00x | 272 / 270 |
| 4 MiB | one | 14.403 | 8.556 | 1.68x | 344 / 416 |
| 4 MiB | dense | 105.653 | 107.376 | 0.98x | 376 / 396 |
| 4 MiB | junk | 21.330 | 15.427 | 1.38x | 478 / 512 |
| 4 MiB | truncated | 12.448 | 1.777 | 7.01x | 337 / 364 |
| 16 MiB | one | 74.569 | 34.559 | 2.16x | 1165 / 1612 |

Reuse covers the source before the first error. Dense input has its first error near the start, so it does not improve. Junk input and the 256 KiB truncated input still fall back to legacy recovery on a resource limit. Their gain comes only from skipping the second strict attempt. Diagnostic counts and fallback counts are unchanged in every row. At 16 MiB with one error, counted work falls from 4,339,914,006 to 1,665,863,922.

Peak memory rises because the seed's syntax stays alive while recovery builds and compacts the new tree: +38% at 16 MiB one-error and +21% at 4 MiB one-error. Retained snapshot storage is unaffected, because the seed is dropped after publication.

## Validation

- 433 workspace test executions, strict all-target/all-feature clippy, fmt and nine Python driver tests.
- Two new regressions in `incremental_recovery.rs`, one interpreted and one generated, were RED before the change (no reuse) and GREEN after. Both compare against full legacy recovery: tree, diagnostics, repairs, coverage and damage. The generated test also checks that seeded facts serve a later edit.
- All 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- The driver previously validated only edits. `--edits` now also validates the cold snapshot when validation is requested, and the driver fails on a cold mismatch. All 5,620 cold snapshots match legacy recovery.
- `rg` was unavailable on the capture host. For these runs only, the source-digest `rg --files` call used an equivalent `git ls-files --cached --others --exclude-standard` shim on PATH.

Executables (SHA-256): seeded `6d2ad788ecbc518ece29a23eb4109d64b6257a351c788836deb5f11e94a1e8ba`, main `d723355cf152c58125af03288b8edd88969dc069fc790557c6dd56265cc6f1da`. These measurements cover one host and corpus. No Roslyn rerun or cross-platform claim is made.

[Paired timings](stage26-paired.json), [standard traces](stage26-differential.json), [adversarial traces](stage26-adversarial.json).
