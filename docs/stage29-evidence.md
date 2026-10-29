# Stage29 token-led prediction and precomputed Pratt dispatch

Before this change, about 45% of the logical work on a valid 4 MiB C# file went to alternatives that failed. That is 111M of 246M, measured by instrumenting failed Pratt prefixes and failed choice alternatives. The existing first-character filter could not reject them. Keyword-led alternatives (modifiers, `class`/`struct`/…, predefined types) all begin with a letter. Identifier-led ones (`Alias::`, generic names, one-parameter lambdas) only fail at their second token.

## Token leads

`Dispatch` now computes a lead for every non-lexical rule, infix tail and choice alternative. A lead is the set of ways a non-empty match can begin. Each entry is a first token, either a literal with keyword boundaries or a complete lexical-rule match, plus what may follow it: a required second token, *open* or *any*.

- **Computation:** it is a least fixpoint over the grammar, solved with a worklist, and unknown absorbs. Rules whose start set is unknown stay unknown, so cycles and unproductive rules still reach the recognizer's ordinary depth and grammar checks.
- **Open and any:** an open token may be extended by what follows. It becomes *any* after an unknown part, an inline predicate, a rule with `except`, or a Pratt operand, because operators may follow. Repetitions pair one-token iterations with the next iteration.
- **Size limit:** sets above 48 entries keep only their first tokens, and larger ones become unknown.

Before an alternative runs, the parser checks the input against its lead after trivia:

- **Literals:** compared exactly as `literal` would compare them.
- **Lexical rules:** run exactly as the alternative's first step would run them, memoized and rolled back.
- **Excluded alternatives:** charged one step and given the failure position the real attempt would record. That is the first token, or the token after a matched first token.
- **Errors during prediction:** the alternative runs anyway and meets the error itself.

Leads are stored as 4-byte steps into the grammar's sorted terminal table.

## Precomputed Pratt dispatch

Every non-lexical Pratt point with at least four operators gets a precomputed plan:
- validated operator metadata: power, label and parts;
- first-character routes, sized to the operator count;
- eligibility masks by precedence floor.

The loop peeks once per operand or operator position, attempts routed operators in grammar order and charges the excluded remainder in bulk. Unoptimized recognition and malformed grammars keep the interpreted loop and its errors.

The lead analysis does most of the work. The plan adds about 3% on a valid 4 MiB file.

## Timings

Current main (`6ae9fa9`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host. Valid inputs use `--snapshot` and damaged inputs `--snapshot-recover`.

| Input | Before s | After s | Speedup | Work before → after |
|---|---:|---:|---:|---|
| 256 KiB valid | 0.096 | 0.079 | 1.21x | 15.8M → 12.2M |
| 4 MiB valid | 1.532 | 1.259 | 1.22x | 245.8M → 190.2M |
| 16 MiB valid | 6.287 | 5.182 | 1.21x | 1030M → 788M |
| 256 KiB one error | 0.154 | 0.132 | 1.17x | unchanged |
| 4 MiB one error | 2.398 | 2.048 | 1.17x | unchanged |
| 16 MiB one error | 10.21 | 8.73 | 1.17x | unchanged |
| 4 MiB truncated | 1.568 | 1.378 | 1.14x | unchanged |
| 4 MiB junk | 9.34 | 8.99 | 1.04x | unchanged |
| 4 MiB dense | 89.6 | 89.9 | 1.00x | 805M → 876M |

Recovery work counts are its own accounting; recovery probes keep their recognition work. Peak RSS is within ±3%.

Edits are unchanged. Median single-character edit at the middle of the file:

| Input | Before ms | After ms |
|---|---:|---:|
| 4 MiB valid | 2.41 | 2.37 |
| 4 MiB one error | 18.9 | 18.8 |
| 16 MiB valid | 13.3 | 13.7 |
| 16 MiB one error | 108.9 | 109.0 |

Those costs sit in reuse, publication and recovery, not in recognition.

## Costs

- **Retained memory:** `Dispatch` is retained per snapshot and counted as index storage. It adds about 52 KiB for C# 4 (102.5 KiB → 155 KiB on a 13 KiB dense-error file).
- **Cold-parse setup:** the analysis runs once per cold parse; edits share it. A tiny C# file's first parse goes from 0.32 ms to about 0.87 ms.
- **Rejected alternative:** a process-wide cache removed that cost but broke the guarantee that dropping every snapshot releases all dynamic storage, so it was dropped.

## Validation

- **Workspace:** 440 workspace test executions pass, including three new lead-soundness tests, with strict all-target/all-feature clippy and fmt clean.
- **Earlier failures:** an earlier unsound version passed both trace corpora but failed the incremental strict-diagnostic tests on error positions. The tests above now cover each case that broke.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, including the cold-snapshot check, with zero mismatches.
- **Profiler SHA-256:** `c62eb7ecd012fb4d17e57dd5af89fa62137123bdb177e5cfde56a78d15e5254d`.

[Paired timings](stage29-paired.json), [standard traces](stage29-differential.json), [adversarial traces](stage29-adversarial.json).
