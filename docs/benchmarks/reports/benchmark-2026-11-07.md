# Engine milestones: November 7–8, 2026

The five approved directions are implemented on `codex/engine-next-milestones`.
Runtime languages now own editable syntax snapshots, the LSP applies incremental
UTF-16 notifications, JSON selects strict trivia, and Calendar has a complete
typed planning workflow. The dependency-index experiment is retained for its
measured storage reduction. This is local, unreleased development work.

## Measurement scope

The [initial checkpoint](../2026-11-07/baseline.json) contains 44 native
cases. Brief compilation overlapped that run, so its timing is exploratory.
It is not the basis for a speedup claim. Historical Roslyn and System.Text.Json
comparisons were not rerun; their dated reports remain historical.

The [final paired matrix](../2026-11-07/storage-paired.json) compares
matched portable release binaries, with inherited Rust flags cleared, LTO,
one codegen unit and no PGO or native-CPU targeting. Both builds use the same
source; only `incremental/dependencies.rs` is replaced with its `2e0211b`
layout for the baseline. Production source is `f8bab61`; raw data records
toolchain, host, input and executable hashes, samples and medians. A dirty
source digest also includes then-present documentation and raw artifacts;
executable hashes identify the measured outputs.

Each case has one untimed fresh process per variant and five measured processes,
alternating variant order. No builds or tests ran alongside this matrix.
Parser time excludes tree fingerprint export; process wall time and RSS include
launch, export and destruction. Instrumented heap figures count requested System
allocator bytes, separately from OS RSS. Retained storage excludes language
analysis shared by the owner/cache; it is not a process heap estimate.

## Storage decision

The selected representation reduces leaf dependency records from 24 to 20 bytes.
Two naturally aligned `u32` values preserve the complete subtree identity;
checked `u16` positions/widths fall back to the original wide representation on
overflow. There are no packed unaligned accesses or truncating conversions.
The memory-budget regression failed at 3,251,896 bytes per 100,000 facts before
the change and passes the new 3,200,000-byte bound.

| JSON case | Index bytes before → after | Index reduction | Retained snapshot reduction |
| --- | ---: | ---: | ---: |
| records-1m | 968,336 → 844,708 | 12.77% | 2.40% |
| records-16m | 15,424,232 → 13,445,596 | 12.83% | 2.41% |
| numbers-1m | 2,377,848 → 2,110,056 | 11.26% | 4.74% |
| numbers-16m | 38,446,512 → 34,141,728 | 11.20% | 4.73% |

The complete 32-case matrix keeps C# parse medians between −0.82% and +1.52%,
JSON warm parse medians between −2.57% and +0.70%, and median edits between
−3.46% and +3.37% relative to the control. These differences are below the
5% gate and are not claimed as speed improvements. Structured JSON retained
heap reductions agree with the storage counters; instrumented heap peaks are
unchanged in the measured cases.

The target is the dependency index, not all syntax storage. A transient FactLog
conversion was not implemented because it would add a second publication buffer;
no performance claim is made for that alternative. Heap peak is effectively
unchanged. An earlier apparent C# dense-recovery RSS regression did not reproduce:
an independent [five-pair replication](../2026-11-07/rss-replication.json)
gave medians 730,808,320 and 731,709,440 bytes (about +0.12%). RSS includes flat
export and is noisy; this change does not demonstrate a process peak reduction.

## Editor analysis

The LSP benchmark compares current fresh analysis with current cached-snapshot
updates, not with the published 0.3.0 LSP. It measures the in-process API through
ready diagnostics, hints and navigation. Registry loading is separate; stdio,
the server's debounce and VS Code rendering are excluded. One warm edit precedes
five samples, always measuring the update before fresh analysis. This fixed order
is a limitation when interpreting small differences. Every resulting tree shape/edge list, value and problem is compared
with fresh analysis outside the timed region.

[Raw editor results](../2026-11-07/lsp-analysis.json), median milliseconds:

| C# input | Fresh analysis | Cached update | Parse update | Materialize | Editor indexes |
| --- | ---: | ---: | ---: | ---: | ---: |
| csharp-262144-valid | 101.17 | 7.98 | 0.39 | 7.37 | 0.14 |
| csharp-262144-one | 394.11 | 11.21 | 2.93 | 7.99 | 0.16 |
| csharp-4194304-valid | 1656.55 | 173.62 | 4.31 | 162.23 | 4.66 |
| csharp-4194304-one | 6208.29 | 179.21 | 9.45 | 162.72 | 4.91 |

CSharp4 is syntax-only, so its semantic phase is negligible in these runs.
Phase medians do not necessarily sum to the total median. Materialization
dominates the remaining large-document update cost; avoiding repeated flat
tree export is the clearest next editor optimization. Registry load and initial
analysis are recorded separately in the raw artifact. Inputs come from
`csharp4_scaling.generated(size)` at 256 KiB/4 MiB targets; `one` removes one
semicolon using `recovery_scaling.damaged(source, "one")`. Save inputs with a
`.csharp4` extension for workspace language selection.

Syntax recognition reuses snapshots. Materialization, binding, structural checks
and editor indexes still process the whole document. Debounced notifications
become one minimal UTF-8-safe difference after ordered transactional UTF-16 edits;
distant changes may therefore reuse less syntax. Frontend authoring files retain
full analysis. Background scheduling/cancellation and incremental semantic
dependencies remain useful follow-ups, guided by these phase costs.

## Correctness and review

- Workspace tests: 701 passed test executions, three ignored. Formatting and
  strict all-target/all-feature clippy passed.
- Optional scanner/SIMD and native-only CLI, process and LSP checks passed;
  all four self-hosted bootstrap fixed points passed.
- Python benchmark/release tooling: 22 tests passed under Python 3.14.
  Extension compilation and 18 extension tests passed.
- Pinned JSONTestSuite commit `1ef36fa01286573e846ac449e8683f8833c5b26a`:
  all 95 `y_` cases accepted and 188 `n_` cases rejected by generated parsing;
  35 implementation-defined cases are not pass/fail gates. Vendored bytes were
  checked against upstream blobs and the MIT license is included.
- Final [standard](../2026-11-07/standard-traces.json) 4,334 and
  [adversarial](../2026-11-07/adversarial-traces.json) 1,286 edit traces
  have zero differential failures. Timed JSON edit fingerprint sequences match
  fresh-parse oracles for every edited state. Paired C# runs separately validate
  the first two edits, covering both alternating replacement states; retained
  history is additionally covered by the trace suites.
- Calendar's deterministic leap-day/two-week workflow agrees through Rust,
  process bindings and the existing component. Preflight/type failures invoke
  no handlers; nested rewrites preserve source origins. LSP tests cover edited
  hints, diagnostics, navigation, reload ownership and stale/invalid changes.

Independent design, ownership and final branch reviews were completed. Ownership
review findings restored static analysis caching and retained analysis through
recovery fallbacks. Final review found two issues, both reproduced and fixed:
strict-trivia recovery classified comment-shaped grammar tokens as trivia, and
the JSON edit benchmark needed fresh-parse equivalence evidence. Regressions now
cover both. The trivia issue affected coverage metadata, not JSON acceptance.

Execution choices: scoped materialization plus the established recovering compile
path preserve structural diagnostics while semantics remain whole-document;
static cache entries own complete analysis tables to preserve allocation
accounting. Installed skill helpers had CRLF shebangs, so normalized temporary
copies/manual progress records were used without editing installed skills.

Manual VS Code interaction and new hosted Windows/Linux CI were not performed.
Reviewers did not independently rerun the full test/benchmark suite; the counts
above are implementer-run evidence. No release, push, merge or upstream project
change is included. Nitrogen compatibility and reproducing DateCalc are not
project goals; reference comparisons are optional engineering evidence. Modern
C# semantics and no_std remain outside these milestones.

## Reproduction

Use the [approved plan](../../superpowers/plans/2026-11-07-engine-next-milestones.md)
for focused tests and the [Calendar guide](../../../languages/calendar/README.md)
for embedding, native/process execution and clock grants. Benchmark harnesses
must be built before timing; never compile alongside measurement. Python 3.11+
is required for release tooling. On macOS, RSS collection needs permission for
`/usr/bin/time -l` to query kernel timing information.

```sh
python3 -m unittest scripts/test_benchmark_checkpoint.py
python3 scripts/benchmark_checkpoint.py --suite all --output checkpoint.json
python3 scripts/benchmark_storage.py \
  --before target/storage-before/release --after target/storage-after/release \
  --before-heap target/storage-before-heap/release \
  --after-heap target/storage-after-heap/release --output storage-paired.json
python3 scripts/shared_subtrees.py --differential --native-syntax \
  --profiler target/storage-after/release/nitride-profile \
  --manifest fixtures/recovery/manifest.json --limit 4334 --report standard.json
python3 scripts/shared_subtrees.py --differential --native-syntax \
  --profiler target/storage-after/release/nitride-profile \
  --manifest fixtures/recovery/adversarial.json --limit 1286 --report adversarial.json
cargo run --release -p nitride-lsp --example benchmark -- WORKSPACE SOURCE.csharp4
```

For the storage control, build once with the current dependency-index file and
once with `git show 2e0211b:crates/nitride-core/src/incremental/dependencies.rs`,
using separate source copies/target directories and identical flags. Build the
`scripts/compiler-audit` and `scripts/json-bench` manifests plus the workspace
`nitride-profile` binary; the JSON heap variant enables its harness `heap` feature.
The raw artifact executable hashes identify the measured outputs.
