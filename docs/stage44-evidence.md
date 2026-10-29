# Stage44 start-set filter for recovery probes

Counting probes on 256 KiB dense input, with 729 errors, after stage43:

| Measure | Count |
|---|---:|
| Rule probes | 458K (about 630 per error) |
| Answered by the probe cache | 106K |
| Generated pre-recognition runs | 310K |
| Pre-recognition runs that failed | 294K |
| Runs whose rule's start set excludes the first character | 200K |

A pre-recognition run averaged about 58 logical steps and reached about 11 bytes past its start, so most of its cost was the fixed setup per run. None of the runs excluded by the start set succeeded.

Rule probes, and predicates over a single syntax reference, now check the start set first. The check is the one a rule reference sees: the dispatch analysis's start set, with leading trivia added for a syntax rule. A call that cannot match there returns its failure at once, charged one step, as reusing a recorded failure is. A rule with unknown or nullable starts always runs.

Reading the character records no observation, because no fact is built from it. The set is conservative, so the filtered failures are failures the probe would have reported anyway. Diagnostics are unchanged on every input. Both trace corpora validate every shared snapshot against the flat recovery, which does not filter, and pass with zero mismatches.

## Timings

Stage43 (`dfe9aeb`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host. The inputs and edit scripts were regenerated for this stage from `scripts/csharp4_scaling.py` and `scripts/recovery_scaling.py`; the inputs are byte-identical in size to earlier stages. The edit scripts toggle one digit near the middle of the file.

| Input | Before s | After s | Speedup | Work before → after |
|---|---:|---:|---:|---|
| 256 KiB valid | 0.051 | 0.051 | 1.00x | equal |
| 4 MiB valid | 0.794 | 0.795 | 1.00x | equal |
| 256 KiB one error | 0.065 | 0.064 | 1.01x | −0.2% |
| 4 MiB one error | 0.977 | 0.988 | 0.99x | −0.2% |
| 16 MiB one error | 4.02 | 3.97 | 1.01x | −0.2% |
| 256 KiB dense errors | 0.395 | 0.360 | 1.10x | 10.4M → 9.9M |
| 4 MiB dense errors | 8.57 | 7.93 | 1.08x | 167M → 160M |
| 256 KiB junk | 0.065 | 0.065 | 1.00x | −0.2% |
| 4 MiB junk | 0.985 | 0.986 | 1.00x | −0.2% |
| 4 MiB truncated | 0.743 | 0.750 | 0.99x | equal |

Median single-character edits:

| Script | Before ms | After ms |
|---|---:|---:|
| 4 MiB valid | 1.79 | 1.83 |
| 4 MiB one error | 8.03 | 7.64 |
| 16 MiB valid | 10.3 | 8.7 |
| 16 MiB one error | 34.9 | 31.5 |

Edits in files with an error are 5–10% faster. The 16 MiB valid edit runs no recovery, so its difference is not attributed to this change and is treated as run-to-run variation.

Against the [stage30 Roslyn rows](stage30-comparison.md), dense input is 4.6x Roslyn at 256 KiB (0.360 s against 0.079 s) and 12x at 4 MiB (7.93 s against 0.68 s).

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `189da7cbe1711bf60ab272e6e63932951715f266af16ba5296331da8d10bb7d7`.

[Paired timings](stage44-paired.json), [standard traces](stage44-differential.json), [adversarial traces](stage44-adversarial.json).
