# Stage39 predicate failures and scanned token ends

After stage38, dense-error input took 0.57 s on 256 KiB and 12.9 s on 4 MiB. On 4 MiB, predicate probes took 27% of the time and the recognized-end scan another 26%. Both were mostly interpreted.

- **Predicate probes.** A recovery predicate over a single syntax reference pre-recognizes that rule with the generated parser. Stage38 left the same gap here as in rule probes: when the stored-failure cap left a generated failure unrecorded, the predicate was interpreted again. Pre-recognition now reports that failure. The predicate returns false and charges one step, as reusing a recorded failure does.
- **Scanned token ends.** The recognized-end scan needs only the end of each lexical rule's match. It used to run a full probe for each one: interpreted lexing, a leaf node, a borrowed handle with its reference counting, and two budget checks. It now asks for the end alone. A cached probe result answers first; otherwise the grammar's compiled scanner runs over the contiguous source and charges its logical work, which the scanners match with the interpreter by construction. Scanned ends are cached beside the probe cache, cleared with it and counted in its bound. The ordinary probe still decides whenever no scanner accepts the call.

A debug build compared every scanned end with the interpreted probe's end. Across 256 KiB dense, junk, one-error and truncated input, all 37,452 comparisons agreed.

Logical work on dense input falls 21% on 256 KiB and 22% on 4 MiB. Work rises slightly where a token's end is scanned and later probed in full, because the two are charged separately. Diagnostics are unchanged on every input.

## Timings

Stage38 (`22e34a9`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Before s | After s | Speedup | Work before → after |
|---|---:|---:|---:|---|
| 256 KiB valid | 0.055 | 0.055 | 1.00x | equal |
| 4 MiB valid | 0.860 | 0.859 | 1.00x | equal |
| 256 KiB one error | 0.076 | 0.076 | 1.01x | +0.2% |
| 4 MiB one error | 1.197 | 1.206 | 0.99x | +0.01% |
| 16 MiB one error | 4.90 | 4.98 | 0.98x | +0.002% |
| 256 KiB dense errors | 0.572 | 0.447 | 1.28x | 15.8M → 12.4M |
| 4 MiB dense errors | 12.9 | 10.0 | 1.28x | 294M → 229M |
| 256 KiB junk | 0.076 | 0.076 | 1.01x | +0.2% |
| 4 MiB junk | 1.191 | 1.210 | 0.98x | +0.01% |
| 256 KiB truncated | 0.388 | 0.388 | 1.00x | equal |
| 4 MiB truncated | 0.860 | 0.878 | 0.98x | +0.01% |

The 1–2% differences on one-error, junk and truncated 4–16 MiB are within run-to-run spread; their work is unchanged. Peak RSS is equal or lower. Edit timings are unchanged within noise.

Against the [stage30 Roslyn rows](stage30-comparison.md), dense input is now 5.7x Roslyn at 256 KiB (0.447 s against 0.079 s) and 15x at 4 MiB (10.0 s against 0.68 s). Since stage35 the dense cases have gone from 1.41 s to 0.45 s and from 23.2 s to 10.0 s.

## Probe accounting

The probe's recognizer used to be kept interpreted because probe work counts against the recovery budget. Stages 38 and 39 change that accounting in the same direction as successful pre-recognition did before them. A generated failure is charged as a reused failure. A scanned end is charged the scanner's logical work, which equals the interpreter's. Recovery outcomes stay equivalent: both trace corpora validate every shared snapshot against the flat recovery, whose probes are always interpreted.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `0aa7f1e2c8f158cfdf2b8ec46fd7fe228ac633af7331d341b4933fea25053d85`.

[Paired timings](stage39-paired.json), [standard traces](stage39-differential.json), [adversarial traces](stage39-adversarial.json).
