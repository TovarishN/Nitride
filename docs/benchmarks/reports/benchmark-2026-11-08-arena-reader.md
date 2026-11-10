# Local arena reads during export: November 8, 2026

Following the [temporary-vector optimization](benchmark-2026-11-08-materialization.md),
this change addresses repeated chunk-directory searches during occurrence indexing.
A private reader remembers the last borrowed chunk for one export pass. Node data
is copied once per traversal visit and reused for rule, span and child-count
queries. Children still visit and export in source order with postorder node IDs.

The reader has constant storage, adds no owning references, and changes neither
published arenas nor retained snapshots. It has no global state, synchronization
or unsafe code. A cache hit checks the chunk identity; slot bounds still apply.
Child iterators borrow the immutable arena independently of the reader's mutable
cache, allowing lookups during iteration. This preserves recovery metadata,
logical work limits and capacity checks. Public syntax APIs are unchanged.

## Verification

A new regression alternates chunks whose nodes occupy the same slot, probes
missing chunks and slots, checks labeled child sequences, and reads old and
extended arenas independently. The correct reader passes. Deliberately removing
the chunk-identity guard produces an assertion failure; the guard was restored
before all verification and builds.

Fresh verification: **703 workspace test executions passed**, three ignored.
Strict all-target/all-feature clippy passed. Formatting and patch whitespace
checks passed before committing.

Independent static review found no actionable issues in the cache, traversal or
benchmark changes. The reviewer did not run tests, builds or performance checks.
Existing materialization allocation bounds and fresh-flat-tree comparisons remain
part of the workspace suite. No new allocation or retained-memory reduction is
claimed for this constant-size cache.

## Matched measurements

[Raw results](../2026-11-08/arena-reader.json) record executable and input
hashes, baseline/candidate source hashes, host, toolchain and individual samples.
The baseline is `5312ba5`, including the earlier vector removal. The candidate
changes only `syntax/arena.rs` and `syntax/shared.rs` in production sources.
The baseline executable is the retained candidate from the preceding report
with the same SHA-256. Both executables use the same isolated build directory and portable release
flags, with inherited Rust flags cleared, no PGO, and the LSP example's
`--no-default-features` configuration. Build origin and flags are declarations
from the controlled procedure; hashes identify outputs without independently
proving their origin. Source bytes were checked against the isolated build copy.
No compilation or tests ran alongside measurements.

Each process performs one warm edit and five measured edits. The valid 4 MiB
case uses five process pairs with alternating executable order; other cases use
one pair and are exploratory. Tables summarize medians of process medians.
The example compares every incremental output's tree shape, edges, problems and
values against fresh analysis within the same executable, outside timing.
Workspace tests separately compare exports against the established flat parser.

These are in-process analysis timings: registry loading, debounce, stdio and
VS Code rendering are excluded. Within each process, updates always precede
fresh analysis. CSharp4 is syntax-only, so semantic performance is not established
by these inputs. Results do not establish other-machine or hosted-platform gains.

| C# input target | Process pairs | Materialize before → after (ms) | Cached analysis before → after (ms) | Analysis reduction |
| --- | ---: | ---: | ---: | ---: |
| 256 KiB valid | 1 | 5.74 → 1.60 | 6.36 → 2.18 | 65.7% |
| 256 KiB missing semicolon | 1 | 6.18 → 1.72 | 9.22 → 4.75 | 48.5% |
| 4 MiB valid | 5 | 140.93 → 28.43 | 151.06 → 38.74 | 74.4% |
| 4 MiB missing semicolon | 1 | 149.46 → 29.37 | 166.04 → 44.64 | 73.1% |

The repeated valid 4 MiB case reduces cached analysis by **74.4%** and
materialization by **79.8%**. All five process pairs favor the candidate for
cached analysis. Other cases improve in this run but are exploratory single-pair
measurements. The change is retained for this repeated improvement and passing
correctness checks. Percentages compare matched binaries from this run; they
are not chained with percentages from the earlier report.

Flat materialization still accounts for most of the remaining large-document
analysis time. Snapshot-native editor access and background scheduling/cancellation
remain possible follow-ups; neither is introduced by this change.

## Reproduction

Build the LSP `benchmark` example at `5312ba5` and at the candidate, keeping both
executables. Use identical release flags and refresh source timestamps or use
fresh target directories when replacing source files. Then run:

```sh
python3 scripts/benchmark_lsp.py \
  --before PATH_TO_BASELINE --after PATH_TO_CANDIDATE \
  --baseline-revision 5312ba5 \
  --build-description 'Matched portable release; no-default-features LSP example; Rust flags cleared; no PGO; only arena.rs/shared.rs differ' \
  --output arena-reader.json
```

The runner generates matching 256 KiB/4 MiB C# inputs, including a missing
semicolon. It records baseline/build information as explicit declarations rather
than inferring it from executable files. macOS CPU metadata collection may need
permission outside a restricted sandbox. Manual VS Code and hosted cross-platform
checks were not performed.
