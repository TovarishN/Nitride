# Stage42 fused node commit, and the append-only arena question

Stage41 put arena bookkeeping at about 17% of a valid 16 MiB parse, and proposed an append-only arena for cold strict parses. Before redesigning, this stage measured where that cost actually goes.

## Fused add and commit

A completed rule added its node, which retained each child, and then committed over its mark, which released each child's temporary. For every edge, the two reference updates cancelled, and each looked up the child's chunk.

`ArenaBuilder::add_committed` now does both. When the temporaries above the mark are exactly the node's unpublished children, in order, they become the node's references directly, and neither update runs. Published children are never temporaries, and their ids precede the builder's, so the check needs no lookups.

Any other arrangement takes the ordinary add and commit. So does any node with more than 32 children. An early version also checked wide nodes, and it slowed valid edits by about 3%: each edit rebuilds the top-level list, with thousands of reused children. Reference counts, frees and node ids are unchanged.

## Timings

Stage41 (`9dd914f`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host. Logical work and diagnostics are identical except 4 MiB dense, whose work varies between runs of one binary (stage34).

| Input | Before s | After s | Speedup |
|---|---:|---:|---:|
| 256 KiB valid | 0.054 | 0.052 | 1.02x |
| 4 MiB valid | 0.828 | 0.813 | 1.02x |
| 4 MiB one error | 1.021 | 1.015 | 1.01x |
| 16 MiB one error | 4.13 | 4.12 | 1.00x |
| 4 MiB junk | 1.030 | 1.021 | 1.01x |
| 4 MiB truncated | 0.789 | 0.779 | 1.01x |
| 256 KiB dense errors | 0.430 | 0.432 | 1.00x |
| 4 MiB dense errors | 9.50 | 9.63 | 0.99x |

Edit timings are unchanged within noise, from 4 MiB valid (2.05 → 2.08 ms) to 16 MiB one error (36.0 → 36.3 ms).

## Why not an append-only arena

After this change, a valid 16 MiB profile attributes less to reference counting than stage41 estimated:

| Cost | Share |
|---|---:|
| Reference adjustment | 4.8% |
| Rollback (`restore`) | 2.8% |
| Commit | ~1.7% |

Most of the rest of the "arena" share is node creation, which any arena must do: writing nodes and edges, and computing weights. An append-only arena that rolls back by truncation and counts references once at publication could remove at most about 8%.

It would have to replace three things:

- **Rollback.** Memo and fact pins inside rolled-back regions would need to be promoted, since packrat reuse after backtracking is common.
- **Freed-slot reuse.** On valid code, failed speculation frees about 23% of created nodes, so a parse without reuse would compact at publication, about 8% of the parse.
- **Cache accounting.** The cache budget and its byte accounting depend on pin counts.

The expected net gain is small next to the risk, so the redesign is not pursued. The remaining gap with Roslyn on valid code is spread across generated recognition, lead-token reads, child lists and trivia replay, none above about 15%.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `4c6d3f6ae2c1d506f53e976db5468cf7d1844a1b8ce0df1d90c55f82c6563bc7`.

[Paired timings](stage42-paired.json), [standard traces](stage42-differential.json), [adversarial traces](stage42-adversarial.json).
