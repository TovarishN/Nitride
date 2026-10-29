# Full C# 4 parsing: Nitride, Roslyn and Nitrogen

Measured 2026-10-27 on one macOS ARM64 host:
- **Nitride** `f23dca6`: the generated parser with recovery; on valid files, strict mode measured the same within 1%.
- **Roslyn** 5.0.0.
- **Nitrogen** 0.19.0, through the `Nitride.Reference` generated C# 4 modules.

Both .NET programs ran on .NET 10.0.0, Release builds, workstation GC.

These are full parses from source; no earlier tree and no edits. Input is the generated C# 4 corpus (`scripts/csharp4_scaling.py`), damaged with `scripts/recovery_scaling.py`:
- **One error:** one semicolon removed.
- **Dense errors:** every `return x;` loses its semicolon.
- **Junk:** 64 `@ ` pairs inserted.

The 16 MiB valid input matches the stage15 input byte for byte. Dense errors stop at 4 MiB, since a 16 MiB dense parse takes minutes per run in Nitride.

## Method

What each harness times:
- **Nitride:** `parse_snapshot_generated` in recovering mode.
- **Roslyn:** `CSharpSyntaxTree.ParseText` plus `GetRoot`.
- **Nitrogen:** `Language.Parse`, a fast pass followed by recovery when it fails. Building its language (about 0.05 s) is timed separately and excluded.

Process start and source I/O are excluded throughout.

- **Cold:** the first parse in a fresh process, median of 3 processes. It includes JIT for Roslyn and Nitrogen, and Nitride's per-parse dispatch analysis.
- **Warm:** the median of 5 parses after 2 warm-ups in one process, each result dropped before the next. The slowest Nitride cases (4 MiB dense errors, 16 MiB one error and junk) use 1 warm-up and 2 parses.
- **Peak:** maximum resident set size of the cold process (`/usr/bin/time -l`). On a 32-byte file the processes start at 3.5 MiB for Nitride, 60 MiB for Roslyn and 70 MiB for Nitrogen.
- **Kept:** what the finished result holds, measured differently per parser:
  - **Nitride:** its own storage accounting (`physical_retained_bytes`).
  - **Roslyn:** managed heap growth with the tree alive, after full collections.
  - **Nitrogen:** its tree arrays at their rented length plus the UTF-16 text, because pooled arrays don't register as heap growth.

Nitrogen stops at the first error. It reports the error and "expected end of input" and keeps the tree parsed so far: about 50% of the valid tree's nodes when the error is mid-file, and 15 nodes on the dense input, whose first error is near the start. Its error rows are marked * and are **not comparable**. Nitride and Roslyn recover the whole file and report the same error counts for one error (1) and dense errors (729 and 11,331). On junk they group errors differently: Nitride reports 3–4 and Roslyn 64.

## Time (seconds)

| Input | Nitride cold | Nitride warm | Roslyn cold | Roslyn warm | Nitrogen cold | Nitrogen warm |
|---|---:|---:|---:|---:|---:|---:|
| 256 KiB valid | 0.078 | 0.077 | 0.071 | 0.035 | 0.341 | 0.109 |
| 256 KiB one error | 0.131 | 0.131 | 0.078 | 0.035 | 0.323* | 0.112* |
| 256 KiB dense errors | 4.24 | 4.24 | 0.079 | 0.036 | 0.049* | 0.001* |
| 256 KiB junk | 0.606 | 0.601 | 0.081 | 0.036 | 0.325* | 0.109* |
| 4 MiB valid | 1.26 | 1.25 | 0.631 | 0.144 | 1.35 | 0.808 |
| 4 MiB one error | 2.06 | 2.04 | 0.631 | 0.135 | 1.31* | 0.788* |
| 4 MiB dense errors | 90.2 | 90.5 | 0.679 | 0.144 | 0.049* | 0.001* |
| 4 MiB junk | 9.25 | 9.25 | 0.692 | 0.134 | 1.33* | 0.788* |
| 16 MiB valid | 5.16 | 5.14 | 1.23 | 0.629 | 4.25 | 3.37 |
| 16 MiB one error | 8.84 | 8.96 | 1.29 | 0.675 | 3.93* | 3.23* |
| 16 MiB junk | 37.3 | 37.1 | 1.25 | 0.673 | 3.92* | 3.26* |

## Memory (MiB)

| Input | Nitride peak | Roslyn peak | Nitrogen peak | Nitride kept | Roslyn kept | Nitrogen kept |
|---|---:|---:|---:|---:|---:|---:|
| 256 KiB valid | 22 | 64 | 96 | 4.4 | 4.5 | 4.2 |
| 256 KiB one error | 34 | 67 | 86* | 6.0 | 4.4 | 2.8* |
| 256 KiB dense errors | 42 | 67 | 72* | 6.4 | 4.6 | 0.5* |
| 256 KiB junk | 38 | 67 | 86* | 5.3 | 4.5 | 2.8* |
| 4 MiB valid | 268 | 124 | 396 | 68.0 | 72.8 | 65.4 |
| 4 MiB one error | 412 | 126 | 283* | 92.4 | 72.7 | 36.9* |
| 4 MiB dense errors | 430 | 129 | 87* | 98.5 | 76.0 | 8.0* |
| 4 MiB junk | 516 | 127 | 283* | 82.6 | 72.7 | 36.9* |
| 16 MiB valid | 1,017 | 296 | 1,465 | 264.8 | 328.8 | 257.8 |
| 16 MiB one error | 1,600 | 298 | 1,002* | 363.4 | 328.7 | 145.5* |
| 16 MiB junk | 1,929 | 298 | 1,002* | 324.8 | 328.8 | 145.5* |

## Reading

- **Valid code, cold:** Nitride is 1.1× Roslyn at 256 KiB, 2.0× at 4 MiB and 4.2× at 16 MiB. It is level with Nitrogen at 4 MiB and 1.2× slower at 16 MiB, but 4.4× faster at 256 KiB, where .NET start-up and JIT dominate.
- **Valid code, warm:** Nitride gains nothing from warming up, while Roslyn gets 2–4× faster. The warm gap is 2.2× at 256 KiB and about 8.5× from 4 MiB up. Nitrogen also improves warm (0.81 s against Nitride's 1.25 s at 4 MiB).
- **Kept trees:** about the same size in all three (65–73 MiB at 4 MiB valid). Nitride's peak is 2–3× Roslyn's on large valid files and 3–6× with errors, because of what recovery holds alive while it works. Below 4 MiB Nitride has the smallest peak, as the .NET runtime overhead dominates.
- **Recovery:** this is Nitride's main gap. Against Roslyn cold, one error is 1.7–6.8×, junk 7–30×, and dense errors 54× at 256 KiB and 133× at 4 MiB.

Limits:
- **Repetitions:** cold medians come from 3 processes each, and the slow warm cases from 2 parses.
- **Memory methods differ** by runtime, as described above.
- **Scope:** edit (incremental) parsing is not measured here, and the results cover this generated corpus on one host, not all C# workloads.

## Reproduce

The harness sources are in [stage30-harness](stage30-harness):
- `nbench`: a Cargo crate against this repository.
- `rcmp`: Roslyn.
- `ncmp`: Nitrogen; it references `tools/Nitride.Reference`.
- `run.py`: the driver.

They contain this host's absolute paths: the crate and project references, and the input directory in `run.py`. Edit those before running.

```sh
(cd docs/stage30-harness/nbench && cargo build --release)
dotnet build -c Release docs/stage30-harness/rcmp
dotnet build -c Release docs/stage30-harness/ncmp
python3 docs/stage30-harness/run.py
```

[Raw results](stage30-comparison.json): every sample, peak RSS, kept bytes, diagnostic counts and Nitrogen node counts.
