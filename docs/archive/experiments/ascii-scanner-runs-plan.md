# ASCII scanner run experiment

Continue the compiler audit on `codex/ascii-scanner-runs`, separate from PR #21.

Prototype exact batched replay for repeated lexical expressions whose successful
ASCII first-byte traces consume one byte, observe exactly that byte, and have
the same work/depth cost. Do not cross executed rule references or skip the
terminating failure. Unicode, escapes, insufficient budgets and unknown traces
execute the original generated loop. Require a short successful prefix before
batching, so short tokens do not pay SIMD dispatch cost.

Compare the frozen portable audit executable against scalar batching and an
optional runtime-dispatched AVX2 classifier with a portable scalar fallback.
Use the existing full-tree/diagnostic fingerprints, valid and malformed C#,
long strings, and Unicode. Add exhaustive byte/classifier tests and exact
work/frame boundary tests before admission. Preserve the default portable build.
Retain only an implementation with measured scanner gains and no material
regression in representative full parses; record rejected experiments too.

Outcome: reject a default cutover because mixed-code cases have small
regressions/variation. Retain the measured long-string improvement behind
`ascii-runs` / `simd-ascii-runs`, both disabled by default. The default executable
text section matches the frozen baseline byte for byte. Final evidence and
validation limits are in `docs/ascii-scanner-runs.md`.
