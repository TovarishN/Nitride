# Active polling layout — November 9, 2026

Continuation of the [scope-specialized traversal work](benchmark-2026-11-09-polling.md)
in PR #50. The retained baseline is `4c13a29`; the pre-cancellation reference
remains merge `e7eb003`. Earlier measurements and rejected experiments are kept.

## Change and rationale

The only production change marks `cancellation::Context::check` with `#[cold]`
and `#[inline(never)]`. The intent is to keep token/predicate traversal and
control-flow diagnostic construction out of inlined parser/traversal hot paths.
No polling sites, work thresholds, fixed-unit boundaries, limit calculations,
logical conditions, scope ownership or cancellation latching change. Parser
jump/rewind accounting and the specialized unscoped walk remain unchanged.

This is a compiler-layout experiment. The local measurements support retaining
the hints; they do not prove that a particular register-allocation or instruction-
cache effect caused the result. No compiler-independent speed guarantee follows.

## Verification and review

The initial candidate passed all 101 core library tests before timing. Independent
review found no actionable correctness or maintainability issues in the two
attributes. No builds, tests or measurements were delegated to the reviewer.
Existing deterministic cancellation, spill, scope, rewind, transactional-edit,
worker and traversal boundary regressions remain preservation gates.

## Measurement protocol

Use locked release native-only prebuilt examples on one host, with no concurrent
builds or tests. Compare valid 4 MiB C# input in five alternating fresh process
pairs. Each process warms one edit and measures five updates, checking fresh
analysis tree/edge/problem/value oracles outside timing. Medians below are
medians of process medians. The scope-enabled `4c13a29` comparison puts both
executables under the same unsignalled flag adapter. The direct `e7eb003`
comparison enables the scope in the candidate; the older executable ignores
that environment variable. Unscoped behavior is measured separately.

The unchanged 18-trial latency harness gates valid/damaged 4 MiB parses at
10%, 50% and 90% of calibrated checkpoint ordinals. The released gate returns
false after the main thread signals, so timing includes the next polling batch,
cleanup and thread join. Ordinals are neither elapsed-time percentages nor
parser-phase labels. Preparation, debounce, stdio and GUI are excluded. Individual
primitive scans, bulk maintenance, mutex waits, allocation/destruction and
semantic/host calls retain their documented interruption limits.

## Reproduction

Retain the `benchmark` example built at `4c13a29` as `/tmp/nitride-active-before`,
and one built at `e7eb003` as `/tmp/nitride-cancellation-before`. Build and test the
candidate, then run each measurement sequentially:

```sh
cargo test --workspace --locked
cargo test --locked -p nitride-lsp --no-default-features
cargo test --locked -p nitride-cli --no-default-features --test runtime_snapshots --test lsp_binary
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo build --locked --release -p nitride-lsp --no-default-features --example benchmark --example cancellation
NITRIDE_BENCH_CANCELLATION=1 python3 scripts/benchmark_lsp.py --before /tmp/nitride-active-before --after target/release/examples/benchmark --baseline-revision 4c13a29 --large-valid-only --output docs/benchmarks/2026-11-09/active-cold-scoped.json
python3 scripts/benchmark_lsp.py --before /tmp/nitride-active-before --after target/release/examples/benchmark --baseline-revision 4c13a29 --large-valid-only --output docs/benchmarks/2026-11-09/active-cold-default.json
NITRIDE_BENCH_CANCELLATION=1 python3 scripts/benchmark_lsp.py --before /tmp/nitride-cancellation-before --after target/release/examples/benchmark --baseline-revision e7eb003 --large-valid-only --output docs/benchmarks/2026-11-09/active-cold-original-baseline.json
python3 scripts/benchmark_cancellation.py --binary target/release/examples/cancellation --output docs/benchmarks/2026-11-09/active-cold-latency.json
```

Temporary executable paths are conveniences; retained executable, source and
input hashes identify the actual measured artifacts, including dirty candidate
sources. Direct comparisons establish residual cost; ratios from separate rounds
are not multiplied to infer recovery of original overhead.

## Results

| Comparison | Cached before → after | Change | Fresh before → after | Change |
|---|---:|---:|---:|---:|
| 4c13a29 → candidate, active scope | 32.30 → 31.50 ms | -2.5% | 1.660 → 1.563 s | -5.9% |
| 4c13a29 → candidate, unscoped | 28.52 → 30.01 ms | +5.2% | 1.612 → 1.544 s | -4.2% |
| Pre-cancellation → candidate, active scope | 29.01 → 30.49 ms | +5.1% | 1.514 → 1.519 s | +0.3% |

Active-scope cached analysis improves 2.5% and fresh analysis improves 5.9%
against `4c13a29`; all five pairs improve on both metrics. Unscoped fresh
analysis improves 4.2%, but unscoped cached analysis grows 5.2% (about 1.5 ms);
four of five unscoped cached pairs worsen. This is a measured tradeoff.
The two hints are retained for the background-worker and fresh-analysis gains,
with the unscoped cached cost kept explicit for review.

The direct pre-cancellation comparison shows 5.1% cached overhead and 0.3%
fresh overhead with active cancellation. Fresh analysis is near that baseline
in this round; cached overhead remains. This does not establish zero overhead
or significance across hosts. Earlier and current rounds remain separate
evidence; their ratios are not multiplied.

Final-source local verification passed: 767 workspace tests (three ignored),
228 native-only LSP tests, six native CLI/runtime tests, strict workspace
all-target/all-feature Clippy, formatting, patch checks and native release
example builds. The same source/executable hashes match the initial active
comparison after verification. Unchanged Python tooling retains its prior
required 39-test green gate.

Raw reports: [active scope](benchmarks/2026-11-09/active-cold-scoped.json),
[unscoped](benchmarks/2026-11-09/active-cold-default.json),
[original baseline](benchmarks/2026-11-09/active-cold-original-baseline.json).

## Cancellation latency

All 18 final trials returned cancellation without a partial snapshot.

| Input | Gate ordinal | Median | Range |
|---|---:|---:|---:|
| valid | 10% | 0.084 ms | 0.071–0.089 ms |
| valid | 50% | 0.176 ms | 0.142–0.180 ms |
| valid | 90% | 0.208 ms | 0.203–0.259 ms |
| missing-semicolon | 10% | 0.131 ms | 0.125–0.135 ms |
| missing-semicolon | 50% | 0.579 ms | 0.562–0.638 ms |
| missing-semicolon | 90% | 0.608 ms | 0.604–0.625 ms |

The full local range is 0.07–0.64 ms. These parser trials retain the earlier
sub-millisecond behavior on the measured cases; no universal deadline or
whole-editor cancellation claim is made. Projection remains covered by the
unchanged deterministic work/boundary regressions.

Raw [latency evidence](benchmarks/2026-11-09/active-cold-latency.json).
