# Stage40 post-error recognition and metadata walk

A cold 16 MiB parse with one error took 4.98 s, against Roslyn's 1.29 s. Its profile split into three parts:

| Part | Share |
|---|---:|
| Strict attempt up to the error | 30% |
| Generated pre-recognition of the rest | 40% |
| Finishing the recovered snapshot | 27% |

Pre-recognition does the same logical work per byte as the strict parse, but it ran about a third slower per byte. The finishing step walked every published node to collect recovery metadata.

- **Fact lookups past the seed.** Every rule call checks the reusable fact index. In a cold recovery, that index holds the seed's facts, which all lie before the error, while pre-recognition works after it. Each lookup descended the whole tree before finding nothing. `Index::get` now first checks the root's absolute bounds and returns at once outside them. That is the same answer the descent gives. Pre-recognition now costs about the same per byte as the strict parse.
- **Metadata walk.** Collecting recovered structures, lexical token spans, damaged nodes and reused spans visited each node through `NodeView`. That meant about four chunk lookups per node, each a search of the arena's chunk sequence. A new `PublishedArena::walk` resolves chunks through one dense index for the whole walk. It visits nodes in the same order: a stack, last child first. The collected lists are identical, and so are metadata work, sidecar bytes and retained bytes.

Logical work and diagnostics are identical on every input except 4 MiB dense, whose work count varies between runs of the same binary, as recorded in stage34.

## Timings

Stage39 (`39ab922`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Before s | After s | Speedup |
|---|---:|---:|---:|
| 256 KiB valid | 0.055 | 0.055 | 1.01x |
| 4 MiB valid | 0.851 | 0.856 | 0.99x |
| 256 KiB one error | 0.075 | 0.069 | 1.10x |
| 4 MiB one error | 1.203 | 1.043 | 1.15x |
| 16 MiB one error | 4.98 | 4.23 | 1.18x |
| 256 KiB junk | 0.077 | 0.069 | 1.11x |
| 4 MiB junk | 1.207 | 1.049 | 1.15x |
| 256 KiB dense errors | 0.445 | 0.431 | 1.03x |
| 4 MiB dense errors | 10.04 | 9.66 | 1.04x |
| 256 KiB truncated | 0.389 | 0.386 | 1.01x |
| 4 MiB truncated | 0.875 | 0.805 | 1.09x |

Peak RSS is within noise. Median single-character edits in files with an error are 3–5% faster:

| Script | Before ms | After ms |
|---|---:|---:|
| 4 MiB valid, middle | 2.04 | 2.06 |
| 4 MiB one error, early | 9.57 | 9.35 |
| 4 MiB one error, middle | 9.69 | 9.30 |
| 4 MiB one error, late | 9.55 | 9.15 |
| 16 MiB valid, middle | 11.9 | 12.1 |
| 16 MiB one error, middle | 38.0 | 35.9 |

Against the [stage30 Roslyn rows](stage30-comparison.md):

| Input | Roslyn s | Nitride s | Ratio |
|---|---:|---:|---:|
| 256 KiB one error | 0.078 | 0.069 | faster |
| 256 KiB junk | 0.081 | 0.069 | faster |
| 4 MiB one error | 0.63 | 1.04 | 1.7x |
| 4 MiB junk | 0.69 | 1.05 | 1.5x |
| 16 MiB one error | 1.29 | 4.23 | 3.3x |

Measured for this change but not applied: a reuse of recorded facts at every interpreted probe call gained nothing, because few facts exist at nested positions. Caching predicate probes would cover only the 24% of calls that repeat. A first-byte mask for whole leads would reject only 5% of lead checks on valid code.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `d1108467513bb9dd50ad41472eb6b03f262ec1fd447c8ac9fc40552b6b45748b`.

[Paired timings](stage40-paired.json), [standard traces](stage40-differential.json), [adversarial traces](stage40-adversarial.json).
