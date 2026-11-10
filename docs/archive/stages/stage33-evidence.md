# Stage33 publication and prediction checkpoint

Inclusive profiling of the valid-code path put publication at 12.6% of samples. Token prediction was at 22–24%, about a third of it lexing and leaf building done only to learn where a token ends.

- **In-place publication:** a cold strict parse now publishes its graph without compaction when at most 1/8 of node slots are dead. On valid 4 MiB, 98.7% of slots are live at publication, since releases free failed attempts during the parse and every remaining node is reachable from the root. Compaction copied the whole graph to drop 1.3% of it; skipping it keeps about 1.3 MB of dead slots (1.9% of retained bytes on 4 MiB) and removes the copy's transient memory. The maintenance-debt trigger now applies only to edits; a cold snapshot published in place starts debt-free, like a compacted one.
- **Recovering snapshots still compact.** They publish over the failed strict attempt's shared seed chunks, where compaction rebuilds the reachable graph. Skipping it there was measured: cold one-error 4 MiB went from 1.69 s to 1.38 s and peak from 370 to 250 MiB. But later edits next to the error became 3–5% slower (17.5 → 18.3 ms at 4 MiB, 105 → 110 ms at 16 MiB), because the rebuilt graph's tree-ordered layout serves those edits better. That trade-off is left out.
- **Fact index:** the cold fact index packs sorted facts over borrowed slices, copying each fact once into its leaf instead of once per tree level, and sorts unstably, since its keys are unique.
- **Lexical predictions** no longer build leaves. A prediction behaves exactly like the memoized lexical call it stands for: the same memo hit, or the same scan, charges, statistics and memo entry. But it records the match as a scanned verdict without a node. The first real call that finds that verdict builds the leaf, charging the memo hit it would have charged, and upgrades the entry. Memo cost is charged as for the finished leaf, so generation rotation is unchanged. Logical work and memo hits are identical to stage32 on every measured input.

A larger lead limit (160 entries) was tried for modifier-led declarations. It rejected nothing new and was slower, so it was dropped. Work-based waste accounting overstates lexing: the costliest remaining "failed" alternative is mostly identifier-lexer logical work, not time.

## Timings

Current main (`26b49de`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Before s | After s | Speedup | Peak RSS before → after MiB |
|---|---:|---:|---:|---|
| 256 KiB valid | 0.056 | 0.054 | 1.05x | 23 → 18 |
| 4 MiB valid | 0.894 | 0.838 | 1.07x | 252 → 191 |
| 16 MiB valid | 3.654 | 3.413 | 1.07x | 965 → 738 |
| 256 KiB one error | 0.110 | 0.109 | 1.02x | 34 → 33 |
| 4 MiB one error | 1.698 | 1.660 | 1.02x | 374 → 375 |
| 16 MiB one error | 7.41 | 7.17 | 1.03x | 1567 → 1548 |
| 4 MiB truncated, junk, dense | | | 1.01x | about equal |

Median single-character edits are equal or slightly faster:

| Input | Before ms | After ms |
|---|---:|---:|
| 4 MiB valid | 1.98 | 1.99 |
| 4 MiB one error | 17.6 | 17.4 |
| 16 MiB valid | 12.1 | 12.0 |
| 16 MiB one error | 105.7 | 104.3 |

## Remaining structure

Of 2.28M nodes created on valid 4 MiB, 0.52M (23%) are freed as failed speculation. The arena's per-node bookkeeping is now the largest area: reference counting of temporaries, rollback trails and commits, about 20% inclusive. Removing it needs a different arena: append-only with rollback by truncation, and memo results promoted out of rolled-back regions. That is a redesign, not a tuning step.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy and fmt clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, including the cold-snapshot check, with zero mismatches.
- **Profiler SHA-256:** `76a96f5efdd71b3266c75e653b53d8123725932115d8465588d005ffd7c19b34`.

[Paired timings](stage33-paired.json), [standard traces](stage33-differential.json), [adversarial traces](stage33-adversarial.json).
