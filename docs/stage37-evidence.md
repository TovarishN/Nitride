# Stage37 exhausted searches stay shared

Junk input, a run of 64 stray `@` tokens inside a property, took 8.3 s on 4 MiB against Roslyn's 0.69 s. A profile showed the shared recovery finishing in 12% of that time. The other 87% went to `recover_after_strict`, a complete second recovery with the flat interpreter.

The reason was a compatibility rule. A shared result carrying any `limit` diagnostic was discarded, and recovery was rerun with the flat engine. The junk run makes the local search exhaust its states and resume at a grammar continuation, and that notice has category `limit`. Both engines then produced identical diagnostics: the same missing `}` and `class`, the same unexpected-source span and the same exhaustion notice.

That notice is decided by the search's state count, and both engines run the same search over the same probe answers. Other limits depend on engine-specific accounting, such as work budgets, metadata budgets and stack depth. So the fallback now applies to any limit except this one. The notice text is a shared constant used by both engines and the check.

## Equivalence

- **Validated snapshots:** with `validate`, the profiler compares each shared snapshot with the flat recovery. The cold snapshot and an edit are equivalent on 256 KiB junk, dense and one-error input and on 4 MiB junk, with no fallback.
- **Trace corpora:** both pass with zero mismatches. Fewer edits fall back to the flat engine: 264 → 158 in the standard corpus and 125 → 89 in the adversarial one. The 142 edits that now stay shared are each validated against the flat result.

A search that exhausts its global state limit still falls back; 256 KiB truncated input is one. There, recovery fails with "recovery search state limit exceeded", and where each engine stops can depend on its accounting.

## Timings

Stage36 (`e14512d`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Before s | After s | Speedup | Peak RSS before → after MiB |
|---|---:|---:|---:|---|
| 256 KiB valid | 0.054 | 0.055 | 0.99x | 18 → 19 |
| 4 MiB valid | 0.856 | 0.852 | 1.01x | 190 → 191 |
| 256 KiB one error | 0.078 | 0.077 | 1.01x | 31 → 32 |
| 4 MiB one error | 1.193 | 1.194 | 1.00x | 384 → 384 |
| 16 MiB one error | 4.99 | 4.95 | 1.01x | 1405 → 1390 |
| 256 KiB junk | 0.532 | 0.079 | 6.75x | 46 → 33 |
| 4 MiB junk | 8.30 | 1.20 | 6.89x | 560 → 388 |
| 256 KiB dense errors | 0.879 | 0.876 | 1.00x | 38 → 43 |
| 4 MiB dense errors | 19.0 | 18.9 | 1.00x | 381 → 383 |
| 256 KiB truncated | 0.395 | 0.398 | 0.99x | 37 → 43 |
| 4 MiB truncated | 0.862 | 0.873 | 0.99x | 308 → 277 |

Diagnostics are unchanged everywhere. On junk, the reported work is now the shared engine's (1.7M and 30.6M) instead of the flat fallback's (55.0M and 852M). Edit timings are unchanged within noise.

Against the [stage30 Roslyn rows](stage30-comparison.md), junk input is now 1.0x Roslyn at 256 KiB (0.079 s against 0.081 s) and 1.7x at 4 MiB (1.20 s against 0.69 s), down from 6.5x and 12x.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `73862840e83bb70ac72b54ff0fb23bfc95c93c07a304a75cb4e13f8924ed3da4`.

[Paired timings](stage37-paired.json), [standard traces](stage37-differential.json), [adversarial traces](stage37-adversarial.json).
