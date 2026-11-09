# Cancellation polling overhead — November 9, 2026

Follow-up to [the cancellation milestone](benchmark-2026-11-09-cancellation.md)
in PR #50. The initial cancellation build, `ec674f6`, remains the matched
baseline for this optimization; pre-cancellation merge `e7eb003` is a separate
reference. The original report and every original artifact are retained.

## Investigation

The first candidate encoded disabled polling as a maximum threshold and removed
the context-presence branch per step. Five alternating process pairs showed
+0.9% cached / +0.1% fresh unscoped time and -1.6% cached / -0.3% fresh scoped
time. That candidate was discarded because it did not address the measured cost.
All samples, source hashes and its 18 passing cancellation trials remain in
`polling-threshold-trial-{default,scoped,latency}.json`. The now-discarded source
hashes intentionally differ from the final candidate.

The next candidate keeps the original parser/recovery poller. Postorder walks
select a const-specialized path once using captured scope presence. Unscoped
walks compile without hot-loop polling. Active walks check fixed batch boundaries
without reading/updating a threshold on every traversal step. They visit each
consecutive logical work value, including scheduled children and elided leaf
exits, so each next check is at most 1,024 logical units away. Entry and final
boundaries also check. Work-limit accounting and tree construction are unchanged.
The fixed-boundary helper must not be used for work jumps or rewinds; those paths
continue to use the original poller.

## Correctness and review

Independent review caught an entry-span change from the full root to an empty
span. A directed regression reproduced it before the span was restored. A second
regression fails on the original traversal when cancellation is signalled from
the final callback; the final boundary catches it. Review found no further
issues. No builds/tests/measurements were delegated to reviewers.

Existing deterministic gates cover batching, work jumps and rewinds, nested and
inherited scopes, strict/generated/recovering parsing, old readers, transactional
edits, next-job execution and request-waiter independence. Existing projection
checks compare node/edge order, structure and logical limit positions.

## Measurement protocol

Use prebuilt locked release native-only executables, with no simultaneous tests
or builds. Each valid 4 MiB C# comparison alternates executable order across five
fresh process pairs; each process warms one edit and measures five updates.
Fresh-analysis correctness oracles run outside timing. Report medians are medians
of process medians. Both baseline and candidate use the false flag scope in the
scope-enabled comparison. These are local in-process measurements, excluding
registry loading, debounce, stdio and GUI. Gated signal-to-return measurement
retains the previous 18-trial protocol and includes the next polling batch,
cleanup and thread join. No universal deadline follows from these measurements.

The discarded trial is reconstructible from its
[hash-verified patch](benchmarks/2026-11-09/polling-threshold-trial.patch) against
`ec674f6`; it is separate from the final implementation. Its measurements remain
[unscoped](benchmarks/2026-11-09/polling-threshold-trial-default.json),
[scoped](benchmarks/2026-11-09/polling-threshold-trial-scoped.json), and
[latency](benchmarks/2026-11-09/polling-threshold-trial-latency.json).

For reproduction, build and retain the native `benchmark` example at `ec674f6`
as `/tmp/nitride-polling-before`, then build the candidate:

```sh
cargo test --workspace --locked
cargo test --locked -p nitride-lsp --no-default-features
cargo test --locked -p nitride-cli --no-default-features --test runtime_snapshots --test lsp_binary
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo build --locked --release -p nitride-lsp --no-default-features --example benchmark --example cancellation
python3 scripts/benchmark_lsp.py --before /tmp/nitride-polling-before --after target/release/examples/benchmark --baseline-revision ec674f6 --large-valid-only --output docs/benchmarks/2026-11-09/polling-default.json
NITRIDE_BENCH_CANCELLATION=1 python3 scripts/benchmark_lsp.py --before /tmp/nitride-polling-before --after target/release/examples/benchmark --baseline-revision ec674f6 --large-valid-only --output docs/benchmarks/2026-11-09/polling-scoped.json
python3 scripts/benchmark_cancellation.py --binary target/release/examples/cancellation --output docs/benchmarks/2026-11-09/polling-latency.json
```

The additional scope-enabled pre-cancellation comparison uses a retained native
binary from `e7eb003`, `--baseline-revision e7eb003`, and the same scope environment
variable. That older executable ignores the variable. Temporary executable paths
are local conveniences; retained hashes identify actual builds. The scripts
record source identity, inputs, source hashes and every process sample. No
cross-round ratio multiplication is used to infer recovery of original overhead.

## Final candidate results

| Comparison | Cached before → after | Change | Fresh before → after | Change |
|---|---:|---:|---:|---:|
| PR #50 → candidate, unscoped | 33.67 → 30.01 ms | -10.9% | 1.744 → 1.735 s | -0.6% |
| PR #50 → candidate, active scope | 33.50 → 33.22 ms | -0.8% | 1.730 → 1.737 s | +0.4% |
| Pre-cancellation → candidate, active scope | 30.25 → 32.84 ms | +8.6% | 1.626 → 1.738 s | +6.9% |

Unscoped cached analysis improves by 10.9%; its projection phase improves by
13.8%. Active-scope cached analysis changes by -0.8% and its projection phase
by -2.6%; this small total difference is treated as effectively unchanged.
Fresh analysis against PR #50 is effectively unchanged in both modes.
The direct original-baseline comparison still shows 8.6% cached and 6.9% fresh
overhead with active cancellation. The retained change improves ordinary API
cost; it does not resolve the active-worker cost or claim general parser gains.

The scope-specialized traversal is retained for the unscoped improvement.
Further optimization should focus on active polling and fresh parsing; retained
editor projections remain a separate project. No statistical significance,
cross-host or GUI performance claim is made. Every trial remains in the repo.

Final-source verification passed: 767 workspace tests (three ignored), 228
native-only LSP tests, six native CLI/runtime tests, strict workspace
all-target/all-feature Clippy, formatting and patch checks. The regression for
entry-span preservation and final-callback cancellation is included. Python
benchmark tooling is unchanged from its prior required 39-test green gate.

Raw paired reports: [unscoped](benchmarks/2026-11-09/polling-default.json),
[active scope](benchmarks/2026-11-09/polling-scoped.json),
[original baseline](benchmarks/2026-11-09/polling-original-baseline.json).

## Final cancellation latency

All 18 final trials returned cancellation with no partial snapshot.

| Input | Gate ordinal | Median | Range |
|---|---:|---:|---:|
| valid | 10% | 0.088 ms | 0.079–0.096 ms |
| valid | 50% | 0.137 ms | 0.137–0.146 ms |
| valid | 90% | 0.203 ms | 0.196–0.208 ms |
| missing-semicolon | 10% | 0.158 ms | 0.129–0.165 ms |
| missing-semicolon | 50% | 0.600 ms | 0.576–0.631 ms |
| missing-semicolon | 90% | 0.653 ms | 0.620–0.735 ms |

The 0.08–0.74 ms range retains the previous local sub-millisecond behavior on
these cases; it establishes no universal deadline. These trials time parsing,
not editor projection. Traversal polling is covered by deterministic logical
work and boundary tests; no timed whole-editor cancellation claim is added.

Raw [latency evidence](benchmarks/2026-11-09/polling-latency.json).

The later [active-polling layout follow-up](benchmark-2026-11-09-active-polling.md)
updates PR #50 again: active cached analysis improves 2.5%, active fresh analysis
5.9%, with a 5.2% unscoped cached regression against the scope-specialized build.
Its direct pre-cancellation comparison shows 5.1% cached / 0.3% fresh overhead.
The measurements above remain the historical record for the earlier build.
