# Rust compiler optimization audit

Measured 2026-10-30 on 13th Gen Intel(R) Core(TM) i7-13700, Windows x86-64, rustc 1.99.1 (58cf8f9c3 2026-10-13). Source revision: `afda797587c249e947cd773df157257c2a2c4665`.

PGO reduces parse-phase time by 25.0–39.0% across the eight primary CSharp4 cases. Native CPU targeting ranges from 7.5% slower to 2.0% faster; it has no consistent advantage. Combining native targeting with PGO provides no consistent extra improvement. The PGO harness executable is 27.4% larger (3,076,608 → 3,920,896 bytes; these are harness binaries, not shipping CLI sizes).

## Parse-phase medians

Milliseconds; five fresh processes per cell. “PGO reduction” is elapsed-time reduction, not throughput increase.

| Input (target bytes / pattern) | Portable | Native CPU | PGO | Native + PGO | PGO reduction |
|---|---:|---:|---:|---:|---:|
| 262144-valid | 77.65 | 77.06 | 53.11 | 52.81 | 31.6% |
| 262144-one | 99.77 | 100.91 | 72.21 | 72.77 | 27.6% |
| 262144-junk | 100.79 | 102.96 | 72.49 | 74.69 | 28.1% |
| 262144-dense | 621.92 | 609.77 | 466.64 | 472.95 | 25.0% |
| 4194304-valid | 1204.89 | 1242.15 | 844.46 | 846.32 | 29.9% |
| 4194304-one | 1535.48 | 1516.83 | 1103.46 | 1110.03 | 28.1% |
| 4194304-junk | 1523.54 | 1532.89 | 1093.81 | 1093.50 | 28.2% |
| 16m-valid | 6319.81 | 6791.27 | 3854.52 | 4003.56 | 39.0% |
| long-trivia | 1.87 | 1.90 | 1.89 | 1.89 | -1.5% |
| unicode | 1.40 | 1.42 | 1.48 | 1.48 | -5.5% |

## Equivalence and method

200 timed executions, plus one untimed process per case/build. Tree/diagnostic fingerprint mismatches: **0**. Fingerprints cover node rules/spans/order, labeled edges, diagnostics and recovery records. All four builds use the same generated grammar, limits and ordinary allocator. Parse time includes parser setup and syntax construction, but excludes input I/O, export, fingerprinting, process launch and destruction. Logical work/native-call/fallback counters and every raw sample are retained in [the JSON report](compiler-optimization-audit.json).

Build order rotates and reverses between rounds. CPU affinity is not pinned on this hybrid-core host. Five samples and one host support the sizeable PGO result; percent-level native differences should not be treated as reliable wins. The 80-byte Unicode case is a correctness check, not a useful SIMD throughput benchmark. Long-trivia input has a large comment and string, but very little syntax.

PGO trains on separate 64 KiB valid/one-error/junk/dense files from the same generator family, including harness export/fingerprint validation. One portable profile is shared by both PGO builds. No missing-function or profile-mismatch warnings were emitted. Training does not establish generalization to other grammars or production workload distributions. Deployment PGO should train the actual deployed executable. This run does not measure incremental edits, peak memory or a new Roslyn comparison.

## SIMD findings

Both portable and native assembly contain 59 generated lexical functions with unwind frames. Neither set uses YMM registers or the inspected packed comparison instructions. Portable lexical code uses 446 XMM-register instruction sites, including 432 vector moves. Native lexical code has the same counts. This supports the source-level finding that generated character recognition remains scalar.

The native build does use YMM registers elsewhere: 10,720 static instruction sites overall, including 6,098 in functions associated with generated syntax. This demonstrates AVX register use, largely for support work; it does not demonstrate vectorized token recognition. Inspection counts static instruction sites, not CPU samples; inlined operations belong to their enclosing functions. It checks selected packed comparison mnemonics and does not prove the absence of every possible SIMD algorithm. See [portable assembly summary](compiler-optimization-assembly-portable.json) and [native summary](compiler-optimization-assembly-native.json).

The runtime already delegates comment-delimiter searches to standard string search. Generated lexical repetition still performs stateful per-character work, observation, depth and budget tracking. Enabling AVX2 does not transform that design into a bulk scanner. Explicit SIMD would require a new bulk-scanning path that preserves UTF-8 boundaries, observations, partial failures and resource-limit behavior.

## Result

Keep the portable release defaults. PGO is a demonstrated next deployment optimization, subject to production training and incremental-workload validation. CPU targeting alone does not justify a portability change here. No handwritten SIMD scanner is added from these measurements: they establish scalar recognition and a PGO opportunity, but do not measure an end-to-end gain from a SIMD replacement. Further SIMD work needs scanner-specific profiling and a compared prototype.

The only repository additions are the audit harness, inspection scripts and evidence. No production parser, generator or release defaults are changed. Reproduction instructions: [compiler-audit README](../scripts/compiler-audit/README.md).
