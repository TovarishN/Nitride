# Full and incremental C# 4 parsing: Nitride, Roslyn and Nitrogen, refreshed

This page repeats the [stage30 comparison](stage30-comparison.md) after stages 31–44, on the same corpus, harnesses and host. Stage30 remains the historical baseline. It also adds the incremental comparison with Roslyn from [stage34](stage34-evidence.md).

Measured 2026-10-28 on one macOS ARM64 host:

- **Nitride** `0954c58`: the generated parser with recovery. On valid files, strict mode measured the same within 1%.
- **Roslyn** 5.0.0.
- **Nitrogen** 0.19.0, through the `Nitride.Reference` generated C# 4 modules.

Both .NET programs ran on .NET 10.0.0, Release builds, workstation GC.

## Method

Unchanged from stage30.

- **Input:** the generated corpus (`scripts/csharp4_scaling.py`), damaged with `scripts/recovery_scaling.py`. The inputs were regenerated for this run and match the stage30 sizes.
- **Cold:** the first parse in a fresh process, median of 3 processes.
- **Warm:** the median of 5 parses after 2 warm-ups in one process. The slowest Nitride cases use 1 warm-up and 2 parses.
- **Peak:** the cold process's maximum resident set size. Processes start at 3.7 MiB for Nitride, 60 MiB for Roslyn and 71 MiB for Nitrogen.
- **Kept:** what the finished result holds, measured per parser as in stage30.

Nitrogen stops at the first error and keeps the tree parsed so far. Its error rows are marked * and are **not comparable**; on dense input it stops near the start of the file.

Nitride and Roslyn recover the whole file and report the same error counts for one error (1) and dense errors (729 and 11,331). On junk they group errors differently: Nitride reports 3–4 and Roslyn 64.

## Time (seconds)

| Input | Nitride cold | Nitride warm | Roslyn cold | Roslyn warm | Nitrogen cold | Nitrogen warm |
|---|---:|---:|---:|---:|---:|---:|
| 256 KiB valid | 0.050 | 0.050 | 0.068 | 0.033 | 0.323 | 0.108 |
| 256 KiB one error | 0.063 | 0.062 | 0.074 | 0.033 | 0.306* | 0.083* |
| 256 KiB dense errors | 0.358 | 0.355 | 0.075 | 0.034 | 0.047* | 0.000* |
| 256 KiB junk | 0.063 | 0.063 | 0.076 | 0.033 | 0.309* | 0.113* |
| 4 MiB valid | 0.783 | 0.781 | 0.646 | 0.134 | 1.30 | 0.759 |
| 4 MiB one error | 0.975 | 0.963 | 0.641 | 0.136 | 1.17* | 0.738* |
| 4 MiB dense errors | 7.73 | 7.93 | 0.656 | 0.142 | 0.047* | 0.001* |
| 4 MiB junk | 0.974 | 0.971 | 0.676 | 0.121 | 1.26* | 0.732* |
| 16 MiB valid | 3.16 | 3.18 | 1.19 | 0.600 | 4.06 | 3.15 |
| 16 MiB one error | 3.94 | 3.94 | 1.22 | 0.610 | 3.75* | 3.08* |
| 16 MiB junk | 3.91 | 3.93 | 1.27 | 0.628 | 3.65* | 3.08* |

## Memory (MiB)

| Input | Nitride peak | Roslyn peak | Nitrogen peak | Nitride kept | Roslyn kept | Nitrogen kept |
|---|---:|---:|---:|---:|---:|---:|
| 256 KiB valid | 18 | 65 | 96 | 4.5 | 4.5 | 4.2 |
| 256 KiB one error | 31 | 67 | 86* | 6.0 | 4.4 | 2.8* |
| 256 KiB dense errors | 38 | 67 | 72* | 6.5 | 4.6 | 0.5* |
| 256 KiB junk | 31 | 67 | 86* | 6.0 | 4.5 | 2.8* |
| 4 MiB valid | 190 | 124 | 392 | 69.3 | 72.8 | 65.4 |
| 4 MiB one error | 381 | 126 | 283* | 92.9 | 72.7 | 36.9* |
| 4 MiB dense errors | 414 | 130 | 87* | 98.4 | 76.0 | 8.0* |
| 4 MiB junk | 373 | 127 | 282* | 92.9 | 72.8 | 36.9* |
| 16 MiB valid | 736 | 296 | 1,466 | 269.5 | 329.2 | 257.8 |
| 16 MiB one error | 1,366 | 298 | 1,002* | 365.3 | 328.7 | 145.5* |
| 16 MiB junk | 1,366 | 298 | 1,011* | 365.3 | 329.5 | 145.5* |

## Change since stage30

Nitride cold time, and its ratio to Roslyn cold:

| Input | Stage30 cold s | Now cold s | Speedup | Stage30 / Roslyn | Now / Roslyn |
|---|---:|---:|---:|---:|---:|
| 256 KiB valid | 0.078 | 0.050 | 1.6x | 1.1x | 0.73x |
| 256 KiB one error | 0.131 | 0.063 | 2.1x | 1.7x | 0.85x |
| 256 KiB dense errors | 4.24 | 0.358 | 11.8x | 53.9x | 4.79x |
| 256 KiB junk | 0.606 | 0.063 | 9.6x | 7.4x | 0.83x |
| 4 MiB valid | 1.26 | 0.783 | 1.6x | 2.0x | 1.21x |
| 4 MiB one error | 2.06 | 0.975 | 2.1x | 3.3x | 1.52x |
| 4 MiB dense errors | 90.2 | 7.73 | 11.7x | 132.8x | 11.79x |
| 4 MiB junk | 9.25 | 0.974 | 9.5x | 13.4x | 1.44x |
| 16 MiB valid | 5.16 | 3.16 | 1.6x | 4.2x | 2.66x |
| 16 MiB one error | 8.84 | 3.94 | 2.2x | 6.8x | 3.24x |
| 16 MiB junk | 37.3 | 3.91 | 9.6x | 29.8x | 3.08x |

## Incremental edits against Roslyn

Single-character edits, 60 per script, alternating one digit at the named position. Each Roslyn edit applies one text change, then calls `WithChangedText` and `GetRoot`, after an untimed JIT pass over the script, with three timed passes. Nitride runs `nitride-profile --edits`, two processes. The edit scripts were regenerated for this run, so absolute times differ from stage34.

| Edit script | Roslyn median ms | Roslyn p90 ms | Nitride median ms | Nitride p90 ms | Nitride / Roslyn |
|---|---:|---:|---:|---:|---:|
| 256 KiB valid, middle | 1.16 | 1.21 | 0.18 | 0.19 | 0.16 |
| 4 MiB valid, early | 11.09 | 21.22 | 1.74 | 1.79 | 0.16 |
| 4 MiB valid, middle | 11.34 | 21.53 | 1.72 | 1.79 | 0.15 |
| 4 MiB valid, late | 10.88 | 21.11 | 1.69 | 1.78 | 0.16 |
| 4 MiB one error, early | 11.01 | 21.12 | 8.13 | 8.39 | 0.74 |
| 4 MiB one error, middle | 11.44 | 21.50 | 7.76 | 7.96 | 0.68 |
| 4 MiB one error, late | 10.99 | 21.11 | 8.03 | 8.26 | 0.73 |
| 16 MiB valid, middle | 52.13 | 87.10 | 8.55 | 9.39 | 0.16 |
| 16 MiB one error, middle | 54.51 | 90.80 | 32.03 | 32.71 | 0.59 |


## Reading

- **Edits are Nitride's strength.** On valid files it reparses an edit in about 0.16x Roslyn's time at every size. In a file that already has an error it takes 0.59–0.74x, where stage34 found it 1.5–1.9x slower. Roslyn's p90 is about twice its median, from GC; Nitride's p90 is within 10% of its median.
- **Cold parsing of small files.** At 256 KiB Nitride is faster than Roslyn for valid, one-error and junk input: 0.73–0.85x. On valid input it is 6.5x faster than Nitrogen, where .NET start-up and JIT dominate.
- **Cold parsing of large files.** Roslyn's runtime amortizes, and Nitride is 1.2x Roslyn at 4 MiB valid and 2.7x at 16 MiB. Against Nitrogen, Nitride is 1.7x faster at 4 MiB valid and 1.3x faster at 16 MiB.
- **Warm parsing.** Nitride gains nothing from warming up. Roslyn gets 2–5x faster, so the warm gap is 1.5x at 256 KiB and about 5–6x from 4 MiB up.
- **Recovery.** Since stage30, one error is 2.1–2.2x faster, junk 9.5–9.6x and dense errors about 12x. Against Roslyn cold:
  - one error: 0.85x at 256 KiB, 1.5x at 4 MiB, 3.2x at 16 MiB;
  - junk: 0.83x, 1.4x, 3.1x;
  - dense errors: 4.8x at 256 KiB and 12x at 4 MiB.

  The dense remainder is the repair search itself: about 630 rule probes and 2,200 candidate states per error ([stage44](stage44-evidence.md)).
- **Memory.** Kept trees are about the same size in all three: 65–73 MiB at 4 MiB valid. Nitride's peak on large valid files has fallen to 1.5x Roslyn's at 4 MiB (190 MiB against 124 MiB) and 2.5x at 16 MiB. It was 2.2x and 3.4x in stage30. With errors it is 3x at 4 MiB and 4.6x at 16 MiB, because recovery holds the failed strict attempt and its facts alive while it works.

Limits:

- **Repetitions:** cold medians come from 3 processes each, and the slow warm cases from 2 parses.
- **Memory methods differ** by runtime, as in stage30.
- **Scope:** the results cover this generated corpus on one host, not all C# workloads. The edit scripts change one digit; larger or structural edits are not measured.

## Reproduce

The harnesses are those in [stage30-harness](stage30-harness), with `rinc` and `incremental.py` for edits. They contain this host's absolute paths; edit those before running.

```sh
(cd docs/stage30-harness/nbench && cargo build --release)
dotnet build -c Release docs/stage30-harness/rcmp
dotnet build -c Release docs/stage30-harness/ncmp
dotnet build -c Release docs/stage30-harness/rinc
python3 docs/stage30-harness/run.py
python3 docs/stage30-harness/incremental.py <nitride-profile> <rinc> <out.json>
```

[Raw full-parse results](stage45-comparison.json) and [raw incremental results](stage45-incremental.json).
