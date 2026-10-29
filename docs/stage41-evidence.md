# Stage41 arena slot table, and where the valid path stands

The private arena found an unpublished node's chunk by assuming chunk ids map to slots as `id - first_id`. Freed chunk slots are reused for new ids from the process-wide counter, so after the first reuse that assumption failed, and most lookups fell through to a hashed id directory. On valid 16 MiB, `find_new` took 4.6% of samples, most of it in that fallback.

The builder now keeps a dense table from `id - first_id` to slot. It is updated when a chunk starts and cleared when one is freed. The id directory remains for ids that other arenas have spread too far apart for the table, and a table miss still consults it. Lookups return the same slots.

## Timings

Stage40 (`0d51c8d`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host. Logical work and diagnostics are identical on every input except 4 MiB dense, whose work count varies between runs of the same binary (stage34).

| Input | Before s | After s | Speedup |
|---|---:|---:|---:|
| 256 KiB valid | 0.054 | 0.053 | 1.02x |
| 4 MiB valid | 0.844 | 0.825 | 1.02x |
| 256 KiB one error | 0.068 | 0.068 | 1.00x |
| 4 MiB one error | 1.045 | 1.031 | 1.01x |
| 16 MiB one error | 4.19 | 4.14 | 1.01x |
| 256 KiB junk | 0.068 | 0.067 | 1.02x |
| 4 MiB junk | 1.035 | 1.024 | 1.01x |
| 256 KiB dense errors | 0.434 | 0.429 | 1.01x |
| 4 MiB dense errors | 9.61 | 9.51 | 1.01x |
| 4 MiB truncated | 0.795 | 0.784 | 1.01x |

Edit timings are unchanged within noise.

## The valid path after stage41

On valid 16 MiB, the profile no longer has a dominant function. Measured by area:

| Area | Share |
|---|---:|
| Arena bookkeeping: reference adjustment, node addition, chunk lookup, rollback and commit | ~17% |
| Lead-token reads (`token_end`, `lead_failure`) | ~15% |
| Child-list building | ~10% |
| Trivia replay | ~8.5% |
| Publication | ~4.5% |

The rest is generated recognition itself.

Several candidates were measured this round and dropped:

| Candidate | Finding |
|---|---|
| Skipping memo tables | All memo upkeep together is 3–4% (stage34). |
| Whole-lead first-byte mask | Would reject 5% of lead checks. |
| Predicate cache | 24% of predicate calls repeat. |
| Fact reuse at nested probe calls | No measurable gain. |

Closing the remaining gap with Roslyn on valid code needs structural work, not tuning:

- **An append-only arena for cold strict parses.** Roll back by truncation and set reference counts once at publication, removing most of the 17% bookkeeping.
- **Generated child lists.** The generated parser would write children straight into node storage for rules whose shape is fixed.
- **For dense errors, a cheaper repair search.** About 290 predicate probes per error remain interpreted, because they test grammar fragments rather than whole rules. Compiling fragment recognizers, or exploring fewer repair hypotheses, would change that. The second option changes results.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `b84f2fbe0ed48169095512b74561b73474fc3308701ed4f40615fe52a9323431`.

[Paired timings](stage41-paired.json), [standard traces](stage41-differential.json), [adversarial traces](stage41-adversarial.json).
