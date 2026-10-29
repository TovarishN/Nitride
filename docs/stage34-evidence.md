# Stage34 incremental comparison and recovery reopening

This round added the incremental comparison with Roslyn that the earlier comparisons lacked. It also tested whether the parser's memo tables are worth removing on valid code. They are not.

## Incremental parsing against Roslyn

A new harness, [`stage30-harness/rinc`](stage30-harness/rinc/Program.cs), applies the same edit scripts as `nitride-profile --edits`. Each edit applies one text change, then calls `WithChangedText` and `GetRoot`. Roslyn 5.0.0 targets C# 4, runs on .NET 10, and does one untimed pass over the script to warm the JIT before three timed passes. Nitride runs two processes. The scripts are 60 single-character edits at the given position.

On valid files, Nitride's incremental reparse was already 2.5–6× faster than Roslyn's. On a file that already contained one syntax error, Nitride was 1.5–1.9× slower, and its edit cost grew with file size.

| Script | Roslyn median ms | Nitride before ms | Nitride after ms | After / Roslyn |
|---|---:|---:|---:|---:|
| 256 KiB valid, middle | 0.56 | 0.21 | 0.22 | 0.39 |
| 4 MiB valid, early | 11.7 | 1.91 | 2.02 | 0.17 |
| 4 MiB valid, middle | 11.2 | 2.02 | 1.96 | 0.17 |
| 4 MiB valid, late | 11.8 | 1.96 | 2.10 | 0.18 |
| 4 MiB one error, early | 11.8 | 17.2 | 10.6 | 0.90 |
| 4 MiB one error, middle | 11.5 | 17.1 | 10.6 | 0.93 |
| 4 MiB one error, late | 12.1 | 17.0 | 10.5 | 0.87 |
| 16 MiB valid, middle | 54.6 | 11.9 | 11.9 | 0.22 |
| 16 MiB one error, middle | 55.9 | 103.4 | 40.3 | 0.72 |

Roslyn's 90th-percentile edit is about twice its median, because of GC. Nitride's is within 5–40% of its median.

## Recovery changes

With an error present, each edit reruns the recovery search from the start. That search replays the reused top-level items in batches, and that part is inherent. The test file has 11,331 top-level namespaces, and Roslyn walks them as well. The cost came from three other sources:

- **Prefix reopening.** Every terminal failure walked the whole chain of prefix checkpoints, looking for ones whose lookahead covered the failure. On 4 MiB, 256 failures per edit walked 1.46M checkpoints. Each checkpoint now records its chain's reach: the furthest observation of the checkpoint or of any checkpoint before it. The walk stops once the failure lies past that reach. It skips only checkpoints that could never match, so the queued alternatives are identical. On 4 MiB the walk dropped to 275 steps per edit.
- **Recovery maps.** The task-interning map and the seen-state map used SipHash. They now use the parser's seeded fast hasher. Their keys are fixed-width values and grammar label names, never source text.
- **Recognized-end scan.** At each failure, the scan tried every grammar literal, and the list included duplicates. Literals are now deduplicated and grouped by first byte. Only literals that start with the current byte are tried, and the result, a maximum, is unchanged.

Recovery's trivia skipping also uses one reader per run instead of one per character. This change was neutral in measurement and is kept for simplicity.

## Cold timings

Current main (`d552b1d`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Before s | After s | Speedup |
|---|---:|---:|---:|
| 256 KiB valid | 0.055 | 0.054 | 1.03x |
| 4 MiB valid | 0.840 | 0.843 | 1.00x |
| 256 KiB one error | 0.109 | 0.096 | 1.13x |
| 4 MiB one error | 1.661 | 1.669 | 1.00x |
| 256 KiB dense errors | 4.17 | 1.42 | 2.93x |
| 4 MiB dense errors | 88.3 | 23.3 | 3.80x |
| 256 KiB junk | 0.568 | 0.544 | 1.04x |
| 4 MiB junk | 8.60 | 8.56 | 1.00x |
| 4 MiB truncated | 1.113 | 1.089 | 1.02x |

Dense inputs gain the most because they fail often while long chains of checkpoints are live. Peak RSS is unchanged within 2 MiB, except on 256 KiB junk, which rose from 40 to 49 MiB.

Logical work and diagnostic counts are identical on every input except 4 MiB dense. There, work differs between processes by up to 0.03% on main as well (874,953,354 and 875,181,187 in two runs of the same binary), so the difference is pre-existing run-to-run variation, not a change in behavior.

## Memo tables on valid code

Instrumenting valid 4 MiB counted which memo entries are ever read:

| Memo | Inserts | Hits | Never read |
|---|---:|---:|---:|
| Lexical prediction, matched | 567K | 725K | 0% |
| Lexical prediction, failed | 363K | 34K | 91% |
| Lexical call, matched | 57K | 11K | 80% |
| Lexical call, failed | 125K | 34K | 82% |
| Pratt, matched | 635K | 113K | 91% |
| Pratt, failed | 23K | 0 | 100% |

On this input, `Expression`, `EmbeddedStatement`, `ClassMember` and `TypeDeclaration` are never read. But on valid 16 MiB, all memo maintenance together takes about 3–4% of samples: hash inserts, rehashing, generation rotation, and the reference counts that cached entries hold. Dropping memos on the basis of one corpus would also give up the linear-time guarantee on inputs that backtrack, such as cast ambiguities, so they are kept.

The valid-code profile is flat. The largest functions are lead failure checks at 6.8%, `match_contents` at 6.5% and `token_end` at 6.1%. Arena bookkeeping (`adjust`, `find_new`, `add_prepared`, `restore`, `recycle`) totals about 15%.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `cf29ee7167ba31f53ae0060b5811cbbf83a0b0c11a1852953ba4cdceb45a66c7`.

[Paired timings](stage34-paired.json), [incremental comparison](stage34-incremental.json), [standard traces](stage34-differential.json), [adversarial traces](stage34-adversarial.json).
