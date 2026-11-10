# Stage32 arena and step-cost checkpoint

After stage31 the valid-code profile was flat. The largest area was the tree arena, at about 22% of samples. Five changes; the logical work count is unchanged on every measured input.

- **Release fast path:** every release zero-filled a 512-byte local stack before checking whether the node survived. Releases now decrement and return when the node stays alive. Freeing walks the subtree with a stack reused across calls, in the same order as before, so slot reuse and node IDs are unchanged.
- **Published-node lookups:** the arena's chunk lookup consulted its directory, a std `HashMap` hashed with SipHash, even for published nodes. Chunk IDs only increase, so IDs below the builder's first chunk now return at once. The directory uses the parser's seeded fast hasher.
- **Compaction lookups:** final compaction, and the relocation of recorded facts, look up chunks through a dense table instead of a binary search per node and edge. About 98.7% of slots are still live at publication on valid 4 MiB, so compaction itself is kept.
- **Reference counts:** unpinned 32-bit counts adjust directly, skipping the cache-charge computation that only matters for pinned nodes.
- **Step check:** `tick` and `tick_steps` are inlined, with one combined limit branch and a cold error path. The stack limit still takes precedence over the depth and work limits.

A per-position cache of lead outcomes was also tried. It gained 0.5% and was dropped.

## Timings

Current main (`c2ec978`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Before s | After s | Speedup |
|---|---:|---:|---:|
| 256 KiB valid | 0.065 | 0.057 | 1.13x |
| 4 MiB valid | 1.028 | 0.890 | 1.16x |
| 16 MiB valid | 4.314 | 3.663 | 1.18x |
| 256 KiB one error | 0.116 | 0.110 | 1.06x |
| 4 MiB one error | 1.810 | 1.695 | 1.07x |
| 16 MiB one error | 7.86 | 7.38 | 1.07x |
| 4 MiB truncated | 1.193 | 1.098 | 1.09x |
| 4 MiB junk | 8.84 | 8.67 | 1.02x |
| 4 MiB dense | 90.6 | 89.4 | 1.01x |

Median single-character edits at the middle of the file:

| Input | Before ms | After ms |
|---|---:|---:|
| 4 MiB valid | 2.94 | 2.09 |
| 4 MiB one error | 18.9 | 17.6 |
| 16 MiB valid | 13.4 | 12.0 |
| 16 MiB one error | 110.5 | 105.5 |

The 4 MiB valid edit figure is within the run-to-run spread seen in stage31, so treat it with care.

Against the [stage30 Roslyn rows](stage30-comparison.md), with the same host, inputs and harness:

| Input | Nitride cold s | Roslyn cold s | Cold ratio | Nitride warm s | Roslyn warm s | Warm ratio |
|---|---:|---:|---|---:|---:|---|
| 256 KiB valid | 0.066 | 0.071 | 0.93x, faster | 0.057 | 0.035 | 1.6x |
| 4 MiB valid | 0.910 | 0.631 | 1.44x | 0.897 | 0.144 | 6.2x |
| 16 MiB valid | 3.69 | 1.23 | 3.0x | 3.63 | 0.629 | 5.8x |
| 4 MiB one error | 1.71 | 0.631 | 2.7x | 1.69 | 0.135 | 12.5x |

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy and fmt clean. The grammar bootstrap fixed point is verified.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, including the cold-snapshot check, with zero mismatches.
- **Profiler SHA-256:** `cdcb04b610487ae010dc15ec4365c24f440644f21a4bcb93c55f62fab31972a3`.

[Paired timings](stage32-paired.json), [standard traces](stage32-differential.json), [adversarial traces](stage32-adversarial.json).
