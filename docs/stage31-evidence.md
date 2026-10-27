# Stage31 lexing once per position and generated choices

The aim was valid-code speed: read each token once, and generate choices and Pratt dispatch. Profiling current main showed the stage29 token checks had become the largest single cost: `token_end` and `lead_failure` took about 18% of samples. Every check re-derived literal properties, entered lexical rules through the generic rule path, and rescanned trivia already skipped at the same position.

## Changes

- **Token cache:** a direct-mapped cache of 512 slots, allocated on first use, keyed by position. Each slot holds the identifier word extent, lexical rule ends and the trivia end, each with the observation it made. Source text is immutable during a parse, so entries never go stale. Replays record the same observation, and trivia replays charge the same logical work, so results and logical work do not depend on cache hits.
- **Terminal metadata:** terminals carry a precomputed keyword flag and first byte. Most lead steps are rejected by one byte compare, and steps sharing a first token are read once. Lexical predictions enter below `rule`, which has no reusable or pending facts for lexical rules.
- **Keyword exclusion:** generated scanners exclude keywords with a sorted spelling table and a length mask, instead of scanning every spelling for every identifier. Answers and observations are unchanged.
- **Memo rotation:** memo generation rotation keeps the retired map's buckets, since the new generation refills to the same bounded size. Eviction under the cache budget still frees them.
- **Generated choices:** every choice is generated, including choice-rooted rule bodies. Each alternative is an arm of `SyntaxContext::choice`. The parser selects candidates by routes and leads, keyed by the static expression, exactly as for interpreted choices, and both share one loop. This measured neutral (1.07 s before and after on valid 4 MiB). Choice interpretation was not a material cost. It is kept because it is correct and removes the remaining interpreted choice roots; the release profiler grows by about 9%.
- **Pratt dispatch:** not generated. The planned loop already runs precomputed tables, and the profile shows no single cost there that generation would remove. The remaining costs are arena reference counting (about 15% of samples), memo maps and the rule-call protocol.

The checked-in grammar seed was regenerated, and the bootstrap fixed point verified.

## Timings

Current main (`3915433`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Before s | After s | Speedup |
|---|---:|---:|---:|
| 256 KiB valid | 0.079 | 0.065 | 1.21x |
| 4 MiB valid | 1.266 | 1.039 | 1.22x |
| 16 MiB valid | 5.156 | 4.297 | 1.20x |
| 256 KiB one error | 0.132 | 0.117 | 1.13x |
| 4 MiB one error | 2.040 | 1.818 | 1.12x |
| 16 MiB one error | 8.78 | 7.87 | 1.12x |
| 4 MiB truncated | 1.377 | 1.187 | 1.16x |
| 4 MiB junk | 9.00 | 8.82 | 1.02x |
| 4 MiB dense | 90.1 | 89.4 | 1.01x |

Peak RSS falls 4–15% on valid and one-error inputs. Median single-character edits are unchanged within noise: 4 MiB valid measured 2.28–2.50 ms on both builds over repeated runs, and 16 MiB one error 109 ms.

Against the [stage30 Roslyn rows](stage30-comparison.md) (same host, inputs and harness), Nitride cold valid is:

| Input | Nitride cold s | Roslyn cold s | Ratio |
|---|---:|---:|---|
| 256 KiB | 0.064 | 0.071 | 0.90x, faster |
| 4 MiB | 1.04 | 0.63 | 1.65x |
| 16 MiB | 4.28 | 1.23 | 3.5x |

Warm, Roslyn stays 1.8x, 7.2x and 6.8x faster.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy and fmt clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, including the cold-snapshot check, with zero mismatches.
- **Profiler SHA-256:** `9c793c8931b1e1eb8c854068598dca49f7ed88e51ad5680c72d556abdbe592d7`.

[Paired timings](stage31-paired.json), [standard traces](stage31-differential.json), [adversarial traces](stage31-adversarial.json).
