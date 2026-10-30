# Optional ASCII run batching and AVX2 scanning

The generated lexer can batch repeated ASCII traces instead of replaying each grammar expression per byte. The final pinned 1 MiB string case takes 6.21 ms with batching disabled, 2.17 ms with scalar batching and 1.80 ms with AVX2: 3.45x faster than the default on this workload. This is a long-string result, not a general parser speedup.

Both features are **off by default**. Mixed-code measurements include small regressions and timing variation, so the experiment does not justify a default cutover. The default method returns zero without replay, and the compiler removes the generated fast-path call. The default harness's PE text section is byte-identical to the frozen baseline (section hashes are in the report). All variants retain the same ordinary allocator, portable release flags and grammar.

## Implementation

- The generator finds uniform successful ASCII traces that consume exactly one byte, observe only that byte and execute no rule calls. A repeat can batch those traces while preserving the exact per-iteration work/depth cost.
- Short prefixes decline. Depth/work shortages, Unicode, escapes, unknown traces and the terminating failure execute the original body. Complete iterations stop at the work budget; the original loop reproduces partial limit failures.
- `nitride-core/ascii-runs` enables scalar batching. `nitride-core/simd-ascii-runs` includes batching and adds runtime AVX2 detection on x86-64. Other CPUs/architectures use the same scalar path.
- AVX2 uses two nibble-table shuffles, a membership mask and a first-rejected-byte mask to classify 32 bytes at a time. Loads stay within the source slice. Disassembly contains `vpshufb`, `vpcmpeqb` and `vpmovmskb` in the new classifier. No native CPU flags are required.
- Regenerated grammar, binding and project bootstrap seeds match the emitter. All four frontend fixed-point checks pass.

## Final pinned comparison

Milliseconds, median of five fresh-process parses per cell; one untimed warmup per cell. Windows logical CPU 0, inherited process affinity mask 1. Variant order rotates/reverses. Parse timing excludes input reading, export, fingerprinting, process startup and destruction. Same i7-13700/Windows/Rust 1.99.1 host as the compiler audit. No PGO in this comparison.

| Input | Frozen baseline | Default (disabled) | Scalar feature | SIMD feature |
|---|---:|---:|---:|---:|
| 262144-valid | 83.231 | 82.869 | 84.449 | 81.309 |
| 262144-one | 103.532 | 97.597 | 104.704 | 100.697 |
| 262144-junk | 102.221 | 102.207 | 102.731 | 103.140 |
| 262144-dense | 640.799 | 626.761 | 636.965 | 631.845 |
| 4194304-valid | 1282.383 | 1320.345 | 1283.697 | 1283.188 |
| 4194304-one | 1530.850 | 1567.297 | 1569.254 | 1598.428 |
| 4194304-junk | 1666.532 | 1588.659 | 1607.588 | 1625.691 |
| long-trivia | 2.007 | 2.018 | 1.674 | 1.676 |
| unicode | 1.377 | 1.385 | 1.406 | 1.452 |
| string-1m | 6.252 | 6.214 | 2.174 | 1.801 |
| string-unicode-escapes | 1.650 | 1.575 | 1.473 | 1.451 |

220 timed executions plus 44 warmups; **0 tree/diagnostic fingerprint mismatches**. Final binary hashes, PE section hashes, feature fingerprints, source hashes and every sample are in [the final report](ascii-scanner-runs.json). [Unpinned](ascii-scanner-runs-unpinned.json) and [pinned](ascii-scanner-runs-prototype-pinned.json) preliminary trials used the prototype before the final feature gate and are retained as exploratory evidence; their binary paths have since been reused. The final report is authoritative.

## Validation and limits

Classifier tests cover all 256 byte values, masks, unaligned inputs, vector lengths and rejected-byte positions. Runtime differential tests compare exact position, observations, work and depth at work/frame/repetition boundaries, EOF, Unicode and escapes. Generated-parser/lexer tests cover the frozen C# fixtures, modes, long strings, limits, edits inside runs and retained old readers. Core tests pass with features disabled and enabled. Strict workspace all-target/all-feature Clippy, formatting and bootstrap checks pass. CI now checks the optional feature on Linux, macOS and Windows.

The workspace run's remaining failure is the pre-existing `existing_exports_match_native_frontend` comparison against an ignored XML export. Its old `omitted_semantics` metadata lists `Xml.Paired`; the pre-change executable also emits an empty list. The ignored export is preserved. Remaining prepare/process/WASM tests pass with that comparison excluded. A temporary test-only Python PATH entry and workspace-local temp directory avoid this machine's Windows Store Python alias and sandbox temp-file permissions; no machine settings are changed.

This is one host and a bounded workload set. The tiny Unicode case measures mostly parser setup, not vector throughput. There is no updated peak-memory, Roslyn, PGO-combination, ARM SIMD or long-running edit/retention measurement. Use the opt-in features only after checking the consuming workload. Reproduction commands are in [the harness README](../scripts/compiler-audit/README.md).
