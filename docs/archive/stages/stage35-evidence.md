# Stage35 cold recovery publication

A cold parse of a file with one syntax error cost about twice a valid parse: 1.66 s against 0.84 s on 4 MiB. Roslyn takes 0.63 s for either.

A profile of the one-error 16 MiB file split the time three ways:

- **Strict attempt:** the failed attempt up to the error took 26%.
- **Post-error recognition:** recognizing the rest of the file took 28%.
- **Publication:** publishing the recovered snapshot took 34%.

The two parse phases together cost about one valid parse, so most of the extra cost was publication.

- **Evacuation tables.** The recovered graph shares chunks with the failed strict attempt's seed, so compaction evacuates it into a new arena. Evacuation tracked visited nodes and their new IDs in std `HashSet` and `HashMap` tables hashed with SipHash. It also looked up each node's chunk through the sequence, and relocated facts went through the same map. It now uses dense per-chunk tables, indexed through the same chunk index as in-place compaction. Nodes are visited in the same order, so the new IDs are unchanged.
- **Seed facts.** A cold recovering parse starts from the seed's fact index, then inserted every newly recognized fact one at a time. Each insert rebuilt and reallocated a path through the persistent tree. The facts are now merged in one pass: existing and new entries are sorted together, a new entry replaces an existing one at the same position and key, as `insert` did, and the tree is packed once. Because its leaves fill differently, the index is 0.5 MB larger on 4 MiB, about 0.5% of retained bytes.
- **Coverage scan.** Coverage of the recovered source checked comments and whitespace one character at a time, with a fresh reader for each query. One reader now serves the whole scan.
- **Recovery keys.** The set of recovered structures is looked up once per published node. It uses the parser's seeded fast hasher instead of SipHash.

## Timings

Stage34 (`8a79eaa`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Before s | After s | Speedup | Peak RSS before → after MiB |
|---|---:|---:|---:|---|
| 256 KiB valid | 0.054 | 0.054 | 1.00x | 18 → 18 |
| 4 MiB valid | 0.848 | 0.835 | 1.02x | 190 → 191 |
| 256 KiB one error | 0.097 | 0.077 | 1.26x | 33 → 33 |
| 4 MiB one error | 1.664 | 1.205 | 1.38x | 380 → 386 |
| 16 MiB one error | 7.19 | 4.92 | 1.46x | 1569 → 1375 |
| 4 MiB truncated | 1.078 | 0.862 | 1.25x | 342 → 296 |
| 256 KiB junk | 0.545 | 0.528 | 1.03x | 40 → 38 |
| 4 MiB junk | 8.57 | 8.12 | 1.06x | 521 → 537 |
| 256 KiB dense errors | 1.414 | 1.383 | 1.02x | 44 → 42 |
| 4 MiB dense errors | 23.2 | 22.5 | 1.03x | 436 → 413 |

Logical work and diagnostics are identical on every input except 4 MiB dense, whose work count varies between runs of the same binary, as stage34 recorded.

Median single-character edits are equal or slightly faster:

| Script | Before ms | After ms |
|---|---:|---:|
| 4 MiB valid, middle | 2.05 | 1.97 |
| 4 MiB one error, early | 10.6 | 10.4 |
| 4 MiB one error, middle | 10.6 | 10.3 |
| 4 MiB one error, late | 10.4 | 10.2 |
| 16 MiB valid, middle | 11.9 | 11.9 |
| 16 MiB one error, middle | 40.1 | 39.4 |

Against the [stage30 Roslyn rows](stage30-comparison.md), cold one-error parsing is now within 1.0x of Roslyn at 256 KiB (0.077 s against 0.078 s). It is 1.9x Roslyn at 4 MiB (1.20 s against 0.63 s) and 3.8x at 16 MiB (4.92 s against 1.29 s). The remainder is mostly real recognition: the strict attempt up to the error and post-error recognition of the rest, together about one valid parse, plus the evacuation copy that later edits benefit from.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `503e08cebce3f6902b4534539d7264a0a5963efafa80ea628a8ec40ac2be0408`.

[Paired timings](stage35-paired.json), [standard traces](stage35-differential.json), [adversarial traces](stage35-adversarial.json).
