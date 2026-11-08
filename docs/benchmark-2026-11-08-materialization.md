# Snapshot materialization: November 8, 2026

This follow-up targets Nitride's measured editor update cost. It removes two
per-node temporary child vectors from occurrence indexing, retaining the current
flat-tree API. Children append to the existing work stack and its appended suffix
is reversed to preserve source order; completed children stream directly into
output edges. Node IDs remain postorder and source spans, labels, damaged-node
metadata and logical work/capacity limits retain their established behavior.
No dependencies or language-specific branches are added.

## Correctness

The new allocation regression exports 4,096 repeated items and compares all node
shapes, roots, child IDs and edge labels against fresh flat parsing. It failed
before the change with **8,261 allocation calls** and passes a **256-call ceiling**
afterwards. Counters measure only materialization and use the existing serialized
allocation-test harness. This is an allocation-call bound, not a retained memory
or process RSS claim. Large child-sequence iterators still have internal storage.

Fresh verification: **702 workspace test executions passed**, three ignored.
Strict all-target/all-feature clippy passed. The workspace suite includes shared
syntax/recovery, retained-history edits, runtime ownership, strict JSON, Calendar
and LSP incremental analysis tests. Formatting and patch whitespace checks passed before committing.

Independent read-only review found no actionable issues with traversal order,
arena child counts, work limits, metadata or allocation-test synchronization.
The reviewer did not run tests or judge measured performance.

## Matched editor measurement

[Raw results](benchmarks/2026-11-08/materialization.json) identify inputs, source
hashes, executable hashes, host and toolchain. The baseline is `2d31168` and the
candidate changes only `crates/nitride-core/src/syntax/shared.rs`. Source origin
and build flags are recorded from the controlled build procedure; binary hashes
identify the outputs but do not independently prove those settings. Both executables
were built from the same isolated source directory, using identical portable
release flags, no PGO and the LSP's `--no-default-features` configuration. Cargo
source timestamps were refreshed after replacing the file to ensure rebuilding;
the executable hashes differ. No builds or tests ran during measurements.

Each process runs one warm edit followed by five measured edits, comparing each
update's problems, values, complete tree shapes and edges with fresh analysis
outside the timed region. Fresh and incremental analysis within one executable
share its export implementation; the separate regression and workspace tests
supply comparisons against the established flat parser.

The valid 4 MiB case uses five fresh processes per executable, alternating which
executable runs first. Other cases use one process each and are exploratory.
Tables show medians of process medians. The in-process example excludes registry
loading, server debounce, stdio and editor rendering. Each update is measured
before fresh analysis, so small differences may reflect ordering/cache effects.
CSharp4 is syntax-only; these runs do not establish typed semantic performance.

| C# input target | Process pairs | Materialize before → after (ms) | Cached analysis before → after (ms) | Analysis reduction |
| --- | ---: | ---: | ---: | ---: |
| 256 KiB valid | 1 | 7.61 → 5.85 | 8.29 → 6.61 | 20.3% |
| 256 KiB missing semicolon | 1 | 7.75 → 6.26 | 11.00 → 9.49 | 13.7% |
| 4 MiB valid | 5 | 169.61 → 143.54 | 180.89 → 154.01 | 14.9% |
| 4 MiB missing semicolon | 1 | 166.50 → 146.42 | 182.19 → 161.85 | 11.2% |

The repeated 4 MiB valid case reduces materialization by **15.4%** and total
cached analysis by **14.9%**. All five process pairs favor the candidate for
cached analysis. The other cases also improve in this run but have not received
the same fresh-process replication. Fresh-analysis phase medians and individual
samples remain available in the raw report; no general parsing speedup or
cross-platform improvement is claimed. The change is retained for the bounded
allocation count and repeated large-document update improvement.

The result improves the existing flat export rather than eliminating it. A
snapshot-native navigation representation could reduce more of the remaining
whole-document cost, but would require a separate design spanning editor indexes,
semantic consumers and retained snapshot lifetimes. Nitrogen compatibility is
not an acceptance criterion for either direction.

## Reproduction

Build `crates/nitride-lsp/examples/benchmark.rs` at the baseline and candidate in
isolated source copies, with identical flags. Keep both executables before the
next build replaces an output. Refresh modified source timestamps or use fresh
build directories when switching source versions.

```sh
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo build --locked --release \
  -p nitride-lsp --no-default-features --example benchmark
python3 scripts/benchmark_lsp.py \
  --before PATH_TO_BASELINE --after PATH_TO_CANDIDATE \
  --output materialization.json
```

The runner generates 256 KiB/4 MiB C# inputs, including a removed-semicolon case,
and uses `.csharp4` paths for workspace language selection. Run from this checkout
with its `nitride.nproj` workspace; build before measuring and keep competing
compilation/test work stopped. Manual VS Code and hosted cross-platform testing
were not performed for this change.
