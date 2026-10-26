# Predictive recognition prototype evidence

Status: prototype implemented and verified locally. Roslyn parity is not met. Main is unchanged; this extends the managed shared-subtrees worktree without staging, commits, push or merge.

## Implemented

The opt-in shared recognizer compiles conservative rule-start and infix-tail sets from prepared grammar expressions. Nullable/unknown and non-ASCII ambiguity remain eligible; all candidate ranking still uses the existing longest-match/precedence/tie logic. Rules reaching a cycle before mandatory consumption stay unfiltered, preserving limit diagnostics. Analysis has a one-million-expression-visit budget and bounded nesting, with permissive fallback on exhaustion. Metadata is retained in a per-snapshot Arc, reused across edits and included in physical index allocation accounting.

Outer lexical recognitions use the existing two bounded memo generations under a distinct context key. Both successes and failures retain source observations, local furthest position and relative depth. Hits retain/clone syntax handles without descendant copying. The token-children mode and lexical Pratt bodies continue through the existing path. Recovery probes retain the original recognizer because its work participates in the frozen search budget; the accelerated cold attempt still runs before recovery begins. There is no global cache or new dependency. The flat parser remains the independent compatibility oracle.

This is a prepared-dispatch and lexical-reuse prototype, not an ahead-of-time generated lexer/parser backend. Parser work statistics exclude metadata compilation; full parse elapsed time includes compilation, source storage and publication.

## Verification

Eight new test functions were observed RED then GREEN (including a two-case fatal-leading-grammar test): impossible-prefix work, compiled-start metadata, repeated lexical lookahead, productive leading-recursion diagnostics, bounded deep-label metadata, fatal leading grammar checks, infix precedence overflow and fresh/incremental recovery of the frozen call-opener deletion. All 47 core tests and the workspace suite pass, as do strict all-target/all-feature clippy and formatting. All 4,334 standard and 1,286 adversarial traces pass with zero mismatches. Release 100,000-edit zero/eight-reader windows, release-all storage checks, nine Python driver tests, formatting, strict all-feature clippy and bootstrap pass. The workspace suite has 278 successful test executions.

One fresh whole-delta reviewer found the leading-recursion issue; it was fixed and verified by the author. The author additionally fixed the unbounded label helper. No second independent post-fix review is claimed. The reviewer did not run performance, corpus or lifetime checks while timing was active.

## Measurement boundaries

Same generated CSharp4 inputs and hashes as the previous frozen capture and Roslyn report. Three fresh processes per strict/one-error input at 256 KiB, 4 MiB and 16 MiB; allocator instrumentation is compiled out and allocation-phase fields are null. Roslyn and previous Nitride figures are earlier same-host captures, not interleaved reruns. Roslyn timings are ParseText/GetRoot with I/O, process startup and semantics excluded. Different tree representations and recovery policies remain explicit.

A two-second CPU sample of the starts-only intermediate identified expression recognition and guards as substantial remaining costs. That sampled diagnostic run is not final timing evidence. Pre-review medians and executable are preserved in .superpowers/sdd/2026-10-26-predictive-recognition/ and are superseded by corrected-code capture.

The first corpus capture found one mismatch among 4,334 traces: faster probes changed recovery-budget exhaustion. That version was rejected. The complete failed report is preserved as `.superpowers/sdd/2026-10-26-predictive-recognition/pre-compat-differential.json`; the corrected version keeps probe work accounting unchanged and passes the complete recapture and both corpora.

## Final full-parse medians

Seconds, three fresh processes each, on byte-identical inputs. Before and Roslyn are preceding same-host frozen captures; runs are not interleaved.

| Target | Mode | Previous shared | Prototype | Speedup | Roslyn | Prototype / Roslyn |
|---|---|---:|---:|---:|---:|---:|
| 0.25 MiB | strict | 0.720 | 0.226 | 3.19x | 0.0646 | 3.50x |
| 0.25 MiB | one | 1.110 | 0.849 | 1.31x | 0.0711 | 11.94x |
| 4 MiB | strict | 11.052 | 3.573 | 3.09x | 0.6181 | 5.78x |
| 4 MiB | one | 17.260 | 13.384 | 1.29x | 0.6165 | 21.71x |
| 16 MiB | strict | 44.038 | 15.500 | 2.84x | 1.2889 | 12.03x |
| 16 MiB | one | 84.360 | 71.052 | 1.19x | 1.4429 | 49.24x |

The 16 MiB strict parse executes 1,114,192,138 recognizer work steps versus 3,117,072,749 before this prototype. This does not measure Roslyn instruction counts. Recovery-probe accounting is deliberately unchanged; damaged-input speedups come from the faster initial strict attempt.

The first 16 MiB recovered edit matches full legacy recovery at 102.902 ms, copies 1,537 source/syntax bytes, recomputes63 coverage bytes, and uses no fallback or maintenance. This is one validated first-edit sample, not a refreshed p95 matrix. The retained bound is8,549,536 bytes; latest storage is129,096 bytes and the eight-reader union is137,704 bytes, with155 maintenance passes per100,000 edits.

[Full parse raw samples](stage16-full-parse.json), [standard traces](stage16-differential.json), [adversarial traces](stage16-adversarial.json), [first edit](stage16-first-edit.json), [lifetime](stage16-lifetime.json), [admission audit](stage16-admission.json).

Remaining performance work is whole-rule execution and finer lexical/token dispatch. The flat compatibility parser is unchanged, and this prototype remains opt-in. Hosted CI and fresh cross-platform performance measurements are not claimed.
