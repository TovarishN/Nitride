# Cooperative cancellation — November 9, 2026

This unreleased milestone lets an obsolete LSP analysis release the single worker
at parser/recovery checkpoints. It preserves generation checks as the publication
barrier and leaves request-waiter cancellation independent of diagnostic work.

## API and behavior

`nitride_core::with_cancellation(&token, || operation())` installs an opt-in scope.
`CancellationToken::new()` supports explicit cancellation; `from_flag` adapts an
`Arc<AtomicBool>`, and `from_predicate` adapts a cheap, nonblocking, thread-safe
predicate. A true predicate result is latched. Nested scopes combine tokens and
restore their predecessor on return or unwind. Unrelated threads do not inherit
scopes; parser and recovery spill threads explicitly do. Snapshots do not retain
the scope, so cancelling an edit does not poison future edits or old readers.

Existing parser signatures and `Limits` remain unchanged. Pollers capture the
scope once and consult it in batches of 1,024 logical work units; speculative
work rollback preserves the remaining polling credit. Default unscoped polling
requires no token atomic read at each step. Operation boundaries also check the
signal, including snapshot publication, projection/export and fallback entry.

Snapshot APIs return `Err` with diagnostic category `cancelled`, with no partial
snapshot or full-parse retry. Legacy flat recovery keeps its diagnostic-carrying
sentinel return shape and proposes no repair for cancelled input. The LSP worker
discards cancelled outcomes rather than publishing the control-flow diagnostic.
Existing limits still apply and can win when exhausted at the same checkpoint.

This bounds logical work between checks, not wall-clock execution. Individual
primitive scans, grammar-table preparation, bulk index/arena maintenance, mutex
waits, allocation/destruction and semantic/host/component calls can delay return.
No thread termination, unsafe code, core dependency or universal deadline is added.
Release builds retain `panic=abort`; unwind tests do not establish release panic
isolation.

## Measurement protocol

The retained baseline is merge `e7eb003` (PR #49). Both executables use locked,
release, native-only builds on the same host. The paired benchmark alternates
executable order across five fresh process pairs on valid 4 MiB C# input. Each
process warms one edit and measures five updates; checked fresh-analysis oracles
run outside timing. Report medians are medians of process medians. These are
local in-process measurements, excluding registry load, debounce, stdio and GUI.

Ordinary unscoped parsing and an unsignalled `from_flag` scope are measured
separately. The latter uses `NITRIDE_BENCH_CANCELLATION=1`; the pre-feature baseline
ignores this variable. Both cached updates and fresh parses are reported.

The cancellation harness calibrates checkpoint counts on valid and missing-
semicolon 4 MiB inputs. It gates at 10%, 50% and 90% of checkpoint ordinals, three
trials each. After the main thread signals, the gated predicate deliberately
returns false, forcing another polling batch before cancellation is observed.
Signal-to-return includes that batch, local cleanup and thread join. These gates
are neither elapsed-time percentages nor parser-phase labels. Preparation is
excluded. The counting calibration is not an overhead benchmark. A partial
snapshot, unreached gate, unobserved signal or incomplete sample set fails the
harness. Raw artifacts retain hashes, source identity and every sample.

## Verification

Deterministic regressions cover nested scopes/unwind restoration, thread
isolation, work batching and rewinds, strict/shared/generated parsing, recovery,
all spill paths, transactional snapshot edits, retained language/readers and
latest-job execution after obsolete-job cancellation. A real parser is gated in
the worker test; no large-file timing guess decides correctness. Existing
protocol tests retain waiter-ID distinctions, diagnostic continuity, freshness,
shutdown and generation barriers. A teardown regression preserves default API
checkpoints after cancellation TLS has been destroyed.

## Reproduction

```sh
cargo test --workspace --locked
cargo test --locked -p nitride-lsp --no-default-features
cargo test --locked -p nitride-cli --no-default-features --test runtime_snapshots --test lsp_binary
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
python3 -m unittest scripts/test_release.py scripts/test_editor_acceptance.py scripts/test_benchmark_editor.py scripts/test_benchmark_checkpoint.py scripts/test_benchmark_lsp.py scripts/test_benchmark_cancellation.py
cargo build --locked --release -p nitride-lsp --no-default-features --example benchmark --example cancellation
python3 scripts/benchmark_cancellation.py --binary target/release/examples/cancellation --output docs/benchmarks/2026-11-09/cancellation-latency.json
python3 scripts/benchmark_lsp.py --before /tmp/nitride-cancellation-before --after target/release/examples/benchmark --baseline-revision e7eb003 --large-valid-only --output docs/benchmarks/2026-11-09/cancellation-default-overhead.json
NITRIDE_BENCH_CANCELLATION=1 python3 scripts/benchmark_lsp.py --before /tmp/nitride-cancellation-before --after target/release/examples/benchmark --baseline-revision e7eb003 --large-valid-only --output docs/benchmarks/2026-11-09/cancellation-scoped-overhead.json
```

Build and retain the baseline executable at `e7eb003` before switching sources;
its temporary path above is only the local retained copy. Do not run tests or
builds concurrently with measurement. Binary and per-file hashes identify the
actual builds, including dirty candidate sources.

Independent whole-change review identified three gaps: flat recovery result
construction, one generated trivia-speculation rollback, and wide-child loops
in full export. Each received a failing directed regression before its fix.
Recovery construction now polls through history/events, strict-subtree copying,
repair/damage metadata and coverage; the missing rollback preserves polling
credit; export polls child scheduling/edge emission and checks its final boundary.
Existing logical work-limit accounting is preserved separately from polling.
The reviewer ran no builds/tests or measurements. No Important finding was deferred.

The CI-selected Python tooling suite is the required gate here. A broader local
`unittest discover` attempt also selected historical .NET reference/composition
checks; 14 failures and one error resulted from missing reference helper binaries.
Those optional reference checks were not used as cancellation acceptance evidence.

The retained baseline's SHA-256 is
`70d285fed443d6fa249df943c20dc47c370cdaccc4579ac33c0c9da95419fc46`,
matching the candidate executable recorded in both PR #49 projection-walk runs.

Final-source local verification passed: 767 workspace test executions (three
ignored), 228 native-only LSP tests, six native CLI/runtime tests, 39 CI-selected
Python tooling tests, strict workspace all-target/all-feature Clippy, formatting,
version consistency, acceptance-script syntax and patch checks. Native release
benchmark builds passed after the review fixes. Hosted cross-platform CI remains
a separate gate; packaged GUI/paint latency was not remeasured for this milestone.

## Results

All 18 gated trials returned cancellation; no partial result was accepted.

| Input | Gate ordinal | Median signal-to-return | Range |
|---|---:|---:|---:|
| valid | 10% | 0.088 ms | 0.083–0.089 ms |
| valid | 50% | 0.167 ms | 0.139–0.238 ms |
| valid | 90% | 0.220 ms | 0.214–0.282 ms |
| missing-semicolon | 10% | 0.126 ms | 0.125–0.161 ms |
| missing-semicolon | 50% | 0.591 ms | 0.557–0.595 ms |
| missing-semicolon | 90% | 0.599 ms | 0.596–0.655 ms |

| Candidate mode | Cached before → after | Change | Fresh before → after | Change |
|---|---:|---:|---:|---:|
| Unscoped | 29.96 → 33.13 ms | +10.6% | 1.554 → 1.661 s | +6.9% |
| Unsignalled flag scope | 29.04 → 32.48 ms | +11.8% | 1.534 → 1.626 s | +6.0% |

These runs show a repeatable cost: all five pairs in each run
have slower cached and fresh analysis. The projection phase grows by roughly
16% in both runs; parse-update medians are nearly unchanged. Results support
fast checkpoint cancellation on these inputs, with about 3–3.5 ms additional
cached analysis and 0.09–0.11 s additional fresh analysis. Reducing polling
overhead is a follow-up, especially in snapshot traversal. No statistical or
cross-host significance claim is made, and mode-to-mode differences across
separate rounds do not isolate the token adapter cost.

Raw complete artifacts: [latency](../2026-11-09/cancellation-latency.json),
[ordinary APIs](../2026-11-09/cancellation-default-overhead.json),
[unsignalled scope](../2026-11-09/cancellation-scoped-overhead.json).

The subsequent [polling optimization](benchmark-2026-11-09-polling.md) updates
PR #50: unscoped cached analysis improves 10.9% against this initial build;
active-scope total time is effectively unchanged. Direct pre-cancellation
comparison still shows 8.6% cached / 6.9% fresh active-scope overhead. The
original measurements above remain historical evidence for `ec674f6`.

The later [active-polling layout follow-up](benchmark-2026-11-09-active-polling.md)
updates PR #50 again: active cached analysis improves 2.5%, active fresh analysis
5.9%, with a 5.2% unscoped cached regression against the scope-specialized build.
Its direct pre-cancellation comparison shows 5.1% cached / 0.3% fresh overhead.
The measurements above remain the historical record for the earlier build.
