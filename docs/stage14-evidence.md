# Grammar-derived error recovery

Recovery is a separate opt-in API over the merged compact-tree base `3784a91`
(PR #7). Strict parse/compile behavior stays frozen. This stage adds complete
source coverage, bounded local repair, continued typed analysis and a private
execution barrier. Incremental parsing and editor integration remain future work.

The approved local admission gates pass: complete source coverage, reviewed
locality, aggregate Roslyn preservation/cascade gates, compatibility, bounded
scaling and native deployment. This is measured corpus admission, not a
guarantee of per-case Roslyn superiority on arbitrary damage.

## Contracts

- `parse_recovering` returns a tree, diagnostics and deterministic work/repair/
  fallback counters. Valid inputs return the strict tree without a sidecar.
- `SyntaxTree::recovery()` exposes missing literals/categories, exact skipped
  spans and a UTF-8 coverage partition. Missing lexical categories never invent
  identifier/number spelling. Every source byte remains in the original source.
- Grammar continuations use bounded Dijkstra search over original byte offsets.
  Insert/delete cost one; replacement costs two. The default local domain is
  cost 4, 4096 searched states and 4096 bytes. Minimum cost applies only within
  that domain and a validated continuation. Exhaustion returns explicit limit
  diagnostics and measured fallback; it is not a global optimum claim.
- Equal-cost ranking prefers fewer avoidable skipped bytes, fewer missing
  elements, then authored order. A scalar recognized by no authored terminal or
  outer lexical rule is mandatory deletion; actual skipped spans/costs remain
  visible. Predicates and lexical interiors remain strict.
- The recovery builder preserves alias elision and indexed 8-byte edges from
  Stage 13. Selected history/probe suffixes are packed at stable boundaries;
  accepted prefixes are not recopied per error.
- `compile_recovering` continues supported binding/types/HIR with `<error>`
  values. Limits or unsupported lowering can return no partial Program; the
  complete syntax tree remains available. Damaged declarations with names stay visible; missing names introduce
  no symbol. Dependent errors do not manufacture normal values. Independent
  errors, healthy origins and healthy later HIR survive.
- `Program` stores diagnostics privately. Every preflight/execute entry point
  rejects them before any operation handler or value provider can run, even if
  the caller ignores `RecoveredAnalysis.diagnostics`.

Syntax-only CSharp4, XML, composition and grammar authoring receive syntax
recovery; this adds no C# binding/type checking or unsupported XML checks.

## Frozen quality evidence

[`stage14-recovery-quality.json`](stage14-recovery-quality.json) records 757
accepted baselines and 4,334 concrete single-edit variants across all 12 fixture
profiles. The source manifest is [`fixtures/recovery/manifest.json`](../fixtures/recovery/manifest.json).
135 deliberately invalid generated programs and 208 inapplicable operations are
recorded explicitly. The legacy portable unterminated-comment baseline remains
in the report: strict parsing permits it, while recovery reports its damage.

The pinned development oracle is Microsoft.CodeAnalysis.CSharp 5.0.0, with C# 4
parse options. UTF-16 offsets are translated to original UTF-8 bytes, including
supplementary scalars. It is absent from normal native builds/runtime.

Each parser is compared with its own undamaged tree. Paired C# items use common
original statement/member/type intervals; full self-relative denominators are
also reported. Fingerprints retain structure, edge labels, lexical spellings
and missing-element state. Limit diagnostics count in the C# cascade gate;
syntax-only counts are also reported separately. Typed metrics retain HIR kinds/types/arguments,
binding identities and mapped source/rewrite origins; intentional propagation
from damaged declarations counts conservatively as semantic loss.

| Damage | Nitride paired items | Roslyn paired items | Nitride diagnostics, including limits | Roslyn diagnostics |
|---|---:|---:|---:|---:|
| Delete | 999/1011 | 958/1011 | 256 | 528 |
| Insert | 1034/1041 | 982/1041 | 291 | 948 |
| Duplicate | 1036/1052 | 1012/1052 | 324 | 590 |
| Swap | 933/965 | 914/965 | 452 | 758 |
| Truncate | 370/370 | 370/370 | 327 | 757 |
| Missing closer | 1029/1031 | 1009/1031 | 258 | 339 |

Every operation family and the overall aggregate pass the approved preservation
and diagnostic gates. Coverage failures: zero. The 1,512 C# edits still include
31 individual preservation regressions and 95 cases with more diagnostics than
Roslyn; the case-level report publishes these rather than implying per-case
superiority. Across all profiles there are 272 explicit fallback episodes.

[`stage14-recovery-adversarial.json`](stage14-recovery-adversarial.json) adds
1,286 separately frozen cases: 1,086 every-token mutations, 192 seeded multi-error
sequences and eight reviewed isolated errors. All pass byte coverage and repeat
run determinism. The eight reviewed errors also preserve every untouched sibling
and report exactly one primary syntax diagnostic. Preservation on the wider
adversarial set is reported without claiming every mutation is one local error.

## Scaling

All 12 final cases pass the work-per-byte gate: at each larger size, work/byte
stays within twice the 256 KiB target case for the same damage family. The
separate 256 KiB/4 MiB fixed-density class regression also passes (435 seconds
for both parses on this host). Timing varies strongly with grammar workload;
these are bounds and measurements, not an interactive-latency claim.

| Target | Damage | Source bytes | Seconds | Peak requested MB | Retained tree MB | Fallbacks |
|---|---|---:|---:|---:|---:|---:|
| 0.25 MiB | one | 262211 | 0.67 | 17.34 | 6.57 | 0 |
| 0.25 MiB | dense | 261483 | 3.42 | 20.59 | 6.67 | 0 |
| 0.25 MiB | junk | 262340 | 0.68 | 17.54 | 6.57 | 1 |
| 0.25 MiB | truncated | 196659 | 0.67 | 17.38 | 4.15 | 1 |
| 4 MiB | one | 4194491 | 10.47 | 234.56 | 103.70 | 0 |
| 4 MiB | dense | 4183161 | 52.28 | 298.15 | 105.17 | 0 |
| 4 MiB | junk | 4194620 | 10.44 | 236.19 | 103.70 | 1 |
| 4 MiB | truncated | 3145869 | 11.59 | 208.87 | 65.33 | 0 |
| 16 MiB | one | 16777291 | 41.67 | 844.02 | 411.37 | 0 |
| 16 MiB | dense | 16732761 | 202.08 | 1070.70 | 417.16 | 0 |
| 16 MiB | junk | 16777420 | 42.18 | 844.02 | 411.37 | 1 |
| 16 MiB | truncated | 12582969 | 41.25 | 769.77 | 258.48 | 0 |

The profiler counts requested System allocation bytes. Thread stack reservations,
allocator overhead, OS mappings, caller input and JSON serialization are excluded.
Elapsed time covers the whole recovery API, including its initial failed strict
attempt. The work counter covers the selected recovery attempt’s VM transitions
and strict probes; the initial strict attempt and stack retry attempts are not
added to that counter. The default work cap still bounds each attempt.

The 16 MiB one-error tree has the same 6,902,313 syntax nodes as the strict
Stage 13 tree, preserving its compact node/edge layout. The explicit coverage
sidecar nevertheless raises retained bytes from about 210 MB to 411 MB; recovery
scratch raises peak memory to 844 MB. Coverage compression remains a possible
future optimization. The long-junk case
uses an explicit bounded-search fallback at all three sizes. No general
memory advantage over .NET or universal one-diagnostic behavior is claimed.

See [`stage14-recovery-scaling.json`](stage14-recovery-scaling.json) for complete
phase counters, actual source sizes, source/tree capacity and profiler hash.

## Verification and compatibility

- 186 workspace Rust tests, formatting, strict clippy and grammar bootstrap fixed
  point are checked locally. Focused tests include the exhaustive 156-input
  tiny repair oracle, random UTF-8, lookahead/lexical guards, deep damage on a
  small thread, compact-tree spans, partial semantics, shadowing and all four
  execution/preflight barriers. `Program` construction through external struct
  literals is now restricted by its private diagnostic field; callers obtain
  programs through the compile APIs and inspect `diagnostics()`. Exhaustive
  `HirKind` consumers must handle the new `Error` case.
- All existing live comparisons pass: frozen observations, 576 generated
  programs, 238 CSharp4 cases plus 64 generated C# programs, and 107 prepared
  profiles plus 29 invalid grammars. The existing exporter/composition/metric
  Python suite passes 22 tests.
- New Cargo tests are discovered by the existing Linux/macOS/Windows workspace
  CI job, including frozen reviewed errors. Hosted results for these uncommitted
  changes are pending; local macOS does not establish hosted platform success.
- A clean source-only copy builds with `--release --locked --offline`, with no
  .NET executable on PATH. That executable prepares CSharp4 and recovers C# and
  Inference input with healthy later HIR. Core third-party dependencies: zero.
  See [`stage14-native-deployment.json`](stage14-native-deployment.json).
- Release CLI: 1,667,776 bytes, against Stage 13's 1,552,000 (+7.5%). Existing
  namespace/inference/calls/functions/dates/rewrites allocation workloads have
  identical retained System bytes at every measured phase/count. The largest
  rewrite HIR peak grows by 112 bytes; source/tree layouts remain compact.
  The `stage14-*-memory.json` files retain the repeated measurements.
- One independent read-only review by gpt-6-astra found an unresolved-call
  derivative overload diagnostic. Its focused regression failed then passed;
  the primary binding error remains and healthy later calls survive. No other
  confirmed finding or declined behavior remained. Rulings and RED/GREEN
  history are in [`stage14-progress.md`](stage14-progress.md).

## Reproduce

```sh
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p nitride-prepare --example bootstrap -- --check
cargo build --release --locked
cargo test --release -p nitride-cli --test recovery recovery_work_stays_linear_on_large_files_with_dense_errors -- --ignored
dotnet build tools/Nitride.RecoveryReference -p:UseSharedCompilation=false
python3 scripts/test_recovery_metrics.py
python3 scripts/recovery_compare.py
python3 scripts/recovery_adversarial.py
python3 scripts/recovery_scaling.py
python3 scripts/recovery_compatibility.py
```

The manifests are already frozen. `--freeze` explicitly replaces inputs and is
not part of routine verification. The legacy live-reference scripts additionally
require the development exporter and its pinned sibling Nitrogen checkout.
