# Stage38 generated probe failures

After stage37, dense-error input was the widest gap with Roslyn: 0.86 s on 256 KiB and 18.6 s on 4 MiB. Strict rule probes from the recovery search took 41% of the time, and their profile was mostly the interpreter, not generated code.

Each non-lexical probe first runs the generated parser to pre-recognize the fact; the interpreted probe then reuses that fact. Successes worked this way, but failures left no fact, and the interpreted probe decided them again from scratch. Counting on 256 KiB dense: of 264K non-lexical probes, 228K found no fact after pre-recognition. Every one of them was past the cap that bounds stored failed probes, so each failing probe ran twice.

- **Failure facts.** Pre-recognition now sets the probe's failure checkpoint, so a generated failure is recorded at the probe's own key, as the interpreted probe records its own.
- **Unrecorded failures.** When the generated run fails and the cap leaves the failure unrecorded, the probe now returns the failure directly. It charges one step, as reusing a recorded failure does. A generated run that ends in an error, such as a depth or work limit, still falls through to the interpreted probe.

Logical work falls by 56% on 256 KiB dense and 57% on 4 MiB dense, because failures are no longer interpreted. Diagnostics are unchanged on every input.

Equivalence: with `validate`, the profiler compares each shared snapshot with the flat recovery, whose probes are always interpreted. On 256 KiB dense, junk and one-error input, the cold snapshot and two edits are equivalent, with no fallback. Both trace corpora pass with zero mismatches.

## Timings

Stage37 (`1bd4a4c`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Before s | After s | Speedup | Work before → after |
|---|---:|---:|---:|---|
| 256 KiB valid | 0.054 | 0.054 | 1.00x | equal |
| 4 MiB valid | 0.848 | 0.854 | 0.99x | equal |
| 256 KiB one error | 0.075 | 0.075 | 1.01x | −1.6% |
| 4 MiB one error | 1.186 | 1.180 | 1.01x | −0.1% |
| 16 MiB one error | 4.91 | 4.88 | 1.01x | −0.03% |
| 256 KiB dense errors | 0.861 | 0.571 | 1.51x | 35.9M → 15.8M |
| 4 MiB dense errors | 18.6 | 12.8 | 1.45x | 681M → 293M |
| 256 KiB junk | 0.077 | 0.077 | 1.01x | −3% |
| 4 MiB junk | 1.185 | 1.187 | 1.00x | −0.2% |
| 256 KiB truncated | 0.390 | 0.388 | 1.00x | equal |
| 4 MiB truncated | 0.857 | 0.864 | 0.99x | −0.1% |

Peak RSS is unchanged within 3 MiB. Edit timings are unchanged within noise.

Against the [stage30 Roslyn rows](stage30-comparison.md), dense input is now 7.3x Roslyn at 256 KiB (0.571 s against 0.079 s) and 19x at 4 MiB (12.8 s against 0.68 s), down from 11x and 28x.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `e7c7365b3bcb3a3e39acb4f872aeda49a603a76ecb789a7a4ef233789fcabdc3`.

[Paired timings](stage38-paired.json), [standard traces](stage38-differential.json), [adversarial traces](stage38-adversarial.json).
