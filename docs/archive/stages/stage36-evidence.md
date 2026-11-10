# Stage36 recovery probe filtering

Dense-error input was Nitride's widest gap with Roslyn. On 256 KiB with 729 errors it took 1.38 s against Roslyn's 0.079 s, and on 4 MiB 22.5 s against 0.68 s. A profile of the 256 KiB case put 41% of the time in `recognized_end`, the recovery search's scan for the longest token at a position. At every failure and deletion, it ran a strict probe of every lexical rule in the grammar.

- **First-character filter.** Only a non-empty match can raise the recognized end, so a lexical rule that cannot consume the next character is no longer probed. Whether it can comes from the dispatch analysis's first-character sets, which the strict parser already uses to skip alternatives. That analysis is conservative: a rule with unknown starts stays eligible. At the end of the input no lexical rule is probed, because none could consume anything. The flat recovery path keeps probing every rule.
- **Start test.** The search's grammar-derived start test allocated a std `HashSet` hashed with SipHash on every call. It now marks visited codes in a table stamped with an epoch and reused across calls.

Logical work falls where probes are skipped: by 35% on 256 KiB dense and 22% on 4 MiB dense. Diagnostics are unchanged. Every recovered snapshot in both trace corpora is validated against the flat reference recovery, which does not filter, and all cases pass. The flat recovery output is byte-identical to stage35 on 256 KiB dense, junk and one-error input.

Each search state still issues many probes: about 4,900 per error on 256 KiB dense, almost all answered from the probe cache. The probe budget check alone runs 3.6M times there. Reducing that means changing how the search explores repairs, which would change its results, so it is not part of this checkpoint.

## Timings

Stage35 (`0cd1f41`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Before s | After s | Speedup | Work before → after |
|---|---:|---:|---:|---|
| 256 KiB valid | 0.053 | 0.054 | 0.99x | equal |
| 4 MiB valid | 0.837 | 0.835 | 1.00x | equal |
| 256 KiB one error | 0.078 | 0.076 | 1.03x | 1.73M → 1.69M |
| 4 MiB one error | 1.191 | 1.179 | 1.01x | −0.1% |
| 16 MiB one error | 4.94 | 4.87 | 1.01x | −0.03% |
| 256 KiB dense errors | 1.384 | 0.859 | 1.61x | 55.2M → 35.9M |
| 4 MiB dense errors | 22.5 | 18.5 | 1.21x | 876M → 684M |
| 256 KiB junk | 0.527 | 0.527 | 1.00x | equal |
| 4 MiB junk | 8.14 | 8.13 | 1.00x | equal |
| 4 MiB truncated | 0.863 | 0.861 | 1.00x | −0.1% |

Peak RSS is equal or lower, apart from about 1 MiB of noise on valid 4 MiB and 4 MiB truncated.

Median single-character edits:

| Script | Before ms | After ms |
|---|---:|---:|
| 4 MiB valid, middle | 2.05 | 1.97 |
| 4 MiB one error, early | 10.3 | 9.5 |
| 4 MiB one error, middle | 10.2 | 9.5 |
| 4 MiB one error, late | 10.1 | 9.4 |
| 16 MiB valid, middle | 11.8 | 11.8 |
| 16 MiB one error, middle | 39.6 | 37.1 |

Against the [stage30 Roslyn rows](stage30-comparison.md), dense input remains 11x Roslyn at 256 KiB and 27x at 4 MiB. Junk input is unchanged at 6.5x Roslyn at 256 KiB and 12x at 4 MiB.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `e8c4805d1cbe51f39b2339efa19de4a10b2a4f17d59e5bbe3aad227b1b3a3c1a`.

[Paired timings](stage36-paired.json), [standard traces](stage36-differential.json), [adversarial traces](stage36-adversarial.json).
