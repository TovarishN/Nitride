# C# parser assessment after PGO and ASCII batching

This assessment supplements the historical [October 28 three-way comparison](stage45-comparison.md)
with fresh Apple M4 Pro measurements of Nitride `fa9b5c6` and the existing .NET
comparison harnesses. It also incorporates the separate [Windows compiler audit](compiler-optimization-audit.md)
and [AVX2 scanner experiment](ascii-scanner-runs.md). Platform results remain separate.

## What is being compared

Nitride supplies generated C# 4 syntax parsing, bounded recovery and syntax
snapshots. It does not implement the C# type system, overload resolution,
assembly emission or modern C# language coverage. Nitrogen is its .NET language
workbench/reference engine; Roslyn is a complete C#/VB compiler platform. These
measurements compare parser operations, not compiler throughput or equivalent
compiler functionality.

## Fresh native compiler configurations

Measured October 30 on Apple M4 Pro, macOS ARM64, Rust 1.99.1 (Homebrew),
with LLVM 23.1.3; `llvm-profdata` 23.1.4. Release builds use level 3, full LTO,
one codegen unit, aborting panics and the ordinary allocator. Valid inputs use
strict mode; damaged inputs use recovery. Five fresh processes per cell, plus
one untimed process, rotating/reversing variant order. Milliseconds, lower is better.

“Batch” enables `simd-ascii-runs`, which selects scalar batching on this host.
Base PGO and feature-enabled PGO are trained separately on valid/one-error/
junk/dense 64 KiB inputs. Native + PGO shares the base profile.

| Input (target size / pattern) | Portable | Native CPU | PGO | Native + PGO | Batch | Batch + PGO |
|---|---:|---:|---:|---:|---:|---:|
| 262144-valid | 53.525 | 53.164 | 34.956 | 35.444 | 52.201 | 35.739 |
| 262144-one | 67.811 | 68.004 | 45.663 | 44.298 | 67.269 | 45.130 |
| 262144-junk | 68.274 | 67.679 | 45.987 | 45.557 | 67.707 | 46.035 |
| 262144-dense | 376.841 | 376.619 | 255.400 | 259.066 | 384.247 | 263.729 |
| 4194304-valid | 811.962 | 809.069 | 533.428 | 532.427 | 810.440 | 535.812 |
| 4194304-one | 990.613 | 990.973 | 664.805 | 656.340 | 996.429 | 668.489 |
| 4194304-junk | 993.257 | 996.141 | 667.732 | 663.593 | 999.098 | 673.520 |
| 16m-valid | 3,385.016 | 3,335.545 | 2,240.396 | 2,233.124 | 3,431.713 | 2,274.771 |
| long-trivia | 1.100 | 1.098 | 1.142 | 1.146 | 0.954 | 1.022 |
| unicode | 0.816 | 0.862 | 0.838 | 0.846 | 0.801 | 0.837 |
| string-1m | 3.281 | 3.317 | 3.189 | 3.140 | 1.231 | 1.431 |

PGO reduces elapsed time by **32.2–34.7%** across the eight primary
CSharp4 cases (about 1.5× throughput). Native CPU targeting adds no consistent
gain. Batching changes mixed-code times by only a few percent, with both gains
and regressions. On the 1 MiB string, scalar batching is **2.66×** faster than
default; batching + PGO is 2.29× faster than default, and 16.2% slower than
batching alone. PGO training contains no such long string. The optimizations
are workload-dependent and their gains must not be multiplied.

**330 timed parses plus 66 untimed processes; zero cross-build/repeat tree and
diagnostic fingerprint mismatches.** This establishes configuration equivalence
on these inputs, not independent C# correctness. PGO builds emitted missing-profile
warnings for 112 functions (base/native) and 111 (batch); no profile hash-mismatch
warnings were found. Training coverage is incomplete. The tiny Unicode input
is primarily a setup/correctness check. All samples, hashes, logical work and
profile-warning counts are in [the native report](compiler-optimization-macos.json).

## Fresh comparison with Nitrogen and Roslyn

Roslyn 5.0.0 and Nitrogen 0.19.0 run on .NET 10.0.0, Release, workstation GC.
The installed SDK used for building the harnesses is 11.0.100-preview.2;
the harness target/runtime remains .NET 10. Nitrogen runtime identity is
`0.19.0+build.242.ed69ac8aaa3bbc7c8d44c97cf969d1b6ddd89ea2`.
The reused reference module DLL identifies Nitride revision `f95f70c8`;
its CSharp4 grammar files are unchanged at `fa9b5c6`. Reference and runtime DLL
hashes are recorded; the sibling checkout was not rebuilt for these timings.

Five first parses in fresh processes supply each cold median. Warm is one
process with two warmups and five timed full parses. The native and .NET batches
run sequentially on exactly the same hashed input files. Native PGO is the
base feature-disabled build above. Seconds, lower is better.

| Input | Nitride portable | Nitride PGO | Roslyn cold / warm | Nitrogen cold / warm |
|---|---:|---:|---:|---:|
| 262144-valid | 0.053525 | 0.034956 | 0.076933 / 0.041791 | 0.349967 / 0.112437 |
| 262144-one | 0.067811 | 0.045663 | 0.081287 / 0.036639 | 0.332935* / 0.108481* |
| 262144-junk | 0.068274 | 0.045987 | 0.083323 / 0.041514 | 0.329897* / 0.107082* |
| 262144-dense | 0.376841 | 0.255400 | 0.081285 / 0.038141 | 0.048014* / 0.000519* |
| 4194304-valid | 0.811962 | 0.533428 | 0.704427 / 0.150569 | 1.449700 / 0.807577 |
| 4194304-one | 0.990613 | 0.664805 | 0.713336 / 0.152762 | 1.417803* / 0.798125* |
| 4194304-junk | 0.993257 | 0.667732 | 0.679141 / 0.132744 | 1.334613* / 0.773732* |
| 16m-valid | 3.385016 | 2.240396 | 1.411034 / 0.777949 | 4.206760 / 3.271289 |
| string-1m | 0.003281 | 0.003189 | 0.029628 / 0.003629 | 0.020080 / 0.001014 |

*Nitrogen damaged-input results stop at the first error and retain a partial
parse; they measure different work and are not a recovery ranking. Native
first-parse timings above do not measure repeated warm parsing in one process.

- On valid 256 KiB and 4 MiB files, PGO Nitride takes 0.45× and 0.76× Roslyn's cold time.
- At 16 MiB valid, PGO Nitride takes 1.59× Roslyn's cold time. Roslyn's warm full parses
  are 3.54× faster at 4 MiB and 2.88× faster at 16 MiB.
- PGO Nitride is faster than Nitrogen's cold and warm valid-file parses at all three sizes.
  Against Nitrogen warm, it is 1.51× faster at 4 MiB and 1.46× at 16 MiB.
- At 4 MiB with one error, PGO Nitride takes 0.93× Roslyn's cold time; with junk it takes
  0.98×. Treat these modest margins cautiously. Against Roslyn warm it takes 4.35× and 5.03×.
- At 256 KiB dense errors, PGO Nitride still takes 3.14× Roslyn's cold time and 6.70×
  its warm time. The old 4 MiB dense-error row was not rerun and stays historical.
- On the auxiliary 1 MiB string, Nitrogen warm is 1.014 ms, Roslyn warm 3.629 ms,
  and Nitride batching is 1.231 ms (1.431 ms with PGO). Parser ranking depends on input shape.

[Raw .NET results and identities](compiler-comparison-macos.json) and the
[reproduction instructions](../scripts/compiler-audit/README.md) retain all
samples and harness details. Valid files parsed without syntax diagnostics in
all three; diagnostic counts were stable within each .NET case across cold/warm runs.

## Apple Silicon SIMD assessment

`nitride-core/simd-ascii-runs` enables ASCII batching, but the explicit vector
classifier in `crates/nitride-core/src/lexical/runs.rs` is compiled only for
`target_arch = "x86_64"` and selects AVX2 at runtime. On this ARM64 host it uses
the scalar batching implementation. The default `aarch64-apple-darwin` target
already reports `target_feature="neon"` with this rustc. Neither
`-Ctarget-cpu=native` nor PGO adds an explicit ARM classifier. LLVM may still
vectorize support operations; that is a separate claim from a vectorized
token-recognition algorithm.

The corresponding Apple Silicon implementation would use ARM Advanced SIMD
(NEON). A plausible port of the nibble-table classifier would process 16 bytes
per vector using [table lookup (`vqtbl1q_u8`)](https://doc.rust-lang.org/stable/core/arch/aarch64/fn.vqtbl1q_u8.html),
bit operations and [byte comparisons (`vceqq_u8`)](https://doc.rust-lang.org/stable/core/arch/aarch64/fn.vceqq_u8.html).
A [horizontal reduction (`vmaxvq_u8`)](https://doc.rust-lang.org/stable/core/arch/aarch64/fn.vmaxvq_u8.html)
can detect a rejected lane; finding its exact position requires an additional
mask/extraction strategy or a bounded scalar check. This is an implementation
proposal inferred from the available intrinsics, not measured Nitride behavior.
Two 16-byte vectors could cover an AVX2-sized block, but width alone predicts
neither throughput nor end-to-end speedup. Such a port must preserve the current
UTF-8, observations, failure, work-budget and depth contracts and pass the same
differential tests before its timings are meaningful.

The Windows experiment measured a 1 MiB string at 6.214 ms by default,
2.174 ms with scalar batching and 1.801 ms with AVX2. The total 3.45× gain
includes batching; AVX2's additional gain over batching is 1.21×. Mixed C#
files did not show a consistent gain. That result supports a targeted scanner
experiment, not an assumption that the C# parser becomes 3.45× faster, and
cannot be assigned to an unimplemented NEON path.

## Boundaries of the reassessment

The generated corpus favors repeated namespace/class/method shapes. Training
uses separate 64 KiB files from the same generator family, so held-out sizes
do not establish workload generalization. Timings exclude source reading,
process launch, result destruction and native fingerprint/export work; Nitrogen
language construction is also excluded. The first .NET parse includes JIT;
warm results use repeated full parsing, not edits. The .NET runs use default
runtime optimizations, with no custom offline training; two warmups do not
guarantee that tiered JIT/PGO has reached its eventual steady state. Nitrogen
stops at its first syntax error, making damaged-input timings unequal work compared with whole-file
recovery. Diagnostic grouping can also differ between Roslyn and Nitride.

No new comparative memory, incremental-edit, semantic-analysis or
full-compilation study is made here. The .NET harness still emits its retained
memory observations, but no matching native memory measurements were collected.
The October 28 memory and edit results remain historical evidence for that revision; PGO and scanner gains
must not be applied to them arithmetically. Native and .NET measurement batches
run separately on the same host, without affinity pinning. Percent-level
changes need replication before guiding defaults.

## Deployment conclusion

Keep ordinary Cargo release settings portable and leave ASCII batching opt-in.
The [native release pipeline](releases.md), added after the measured revision,
separately applies PGO/LTO and Apple M1 tuning on macOS. Its release binaries
and broader training workload were not benchmarked here.

For a parser consumer, training PGO on representative inputs is the strongest demonstrated
compiler-setting optimization. Train and measure the actual deployed executable:
this standalone parser harness excludes the CLI's WebAssembly runtime and does
not establish an LSP or complete application speedup. PGO also changes code size;
its harness executable grows from 3,107,312 to 3,858,512 bytes (24.2%). This is
not a shipping CLI size measurement or a new memory-use measurement.

A NEON scanner is a reasonable next experiment for workloads with long runs,
with scalar batching as its comparison baseline. Recovery search, tree building
and general parser control flow remain outside that scanner's scope. Broader
C# compiler parity would require semantics and language coverage in addition to
parser optimizations.
