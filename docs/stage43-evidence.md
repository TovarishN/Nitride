# Stage43 generated rules for recovery predicates

Recovery predicates test grammar fragments, such as choice arms and repetition bodies, rather than whole rules. Stages 38 and 39 answered predicates over a single rule reference from the generated parser. Any other fragment was interpreted throughout, including every rule it referenced.

A predicate now evaluates its fragment with the generated parser in charge of referenced rules. Only the fragment's own structure, a few sequence and choice nodes, is interpreted. A predicate needs only a yes or no, so the answer is returned and charged one step, as reusing a recorded fact is. A run that reaches a limit returns nothing, and the interpreted predicate decides as before.

Logical work falls 17% on 256 KiB dense input and 27% on 4 MiB dense input. Diagnostics are unchanged. Both trace corpora validate every shared snapshot against the flat recovery, whose predicates are always interpreted, and pass with zero mismatches.

## Timings

Stage42 (`3cd5ffe`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host. The host was busier than in stage42 (valid 4 MiB 0.89–0.92 s instead of 0.81–0.83 s), so compare within the table only.

| Input | Before s | After s | Speedup | Work before → after |
|---|---:|---:|---:|---|
| 256 KiB valid | 0.054 | 0.053 | 1.02x | equal |
| 4 MiB valid | 0.921 | 0.887 | 1.04x | equal |
| 4 MiB one error | 1.111 | 1.104 | 1.01x | −0.04% |
| 16 MiB one error | 4.51 | 4.45 | 1.01x | −0.05% |
| 256 KiB dense errors | 0.475 | 0.444 | 1.07x | 12.4M → 10.4M |
| 4 MiB dense errors | 10.48 | 9.49 | 1.10x | 229M → 167M |
| 4 MiB junk | 1.108 | 1.110 | 1.00x | −0.08% |
| 4 MiB truncated | 0.846 | 0.877 | 0.96x | −0.02% |

Valid input is unaffected by this change, so its 4% and the truncated input's −4% are host variation. Edit timings are within noise.

## What remains for dense errors

After this change, most of a dense-error parse is generated recognition inside recovery probes, about 40% of samples. A probe of a large rule near an error parses until that error stops it, and the search issues such probes from many starting positions. Allowing those runs to reuse recorded facts at any rule call, not only at repetition checkpoints, was tried and gained nothing measurable.

Further gains here would come from the search itself issuing fewer probes per error, which changes which repairs it finds. They would not come from making probes cheaper.

## Validation

- **Workspace:** all 455 workspace test executions pass, with strict all-target/all-feature clippy, fmt and the bootstrap seed check clean.
- **Trace corpora:** all 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, with zero mismatches.
- **Profiler SHA-256:** `e590e1800bd8f41afaa65ffa7791c0265b040fe3b033f9ce4d7efe83fa1e75f5`.

[Paired timings](stage43-paired.json), [standard traces](stage43-differential.json), [adversarial traces](stage43-adversarial.json).
