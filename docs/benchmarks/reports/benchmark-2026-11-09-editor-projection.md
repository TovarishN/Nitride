# Direct editor projection — November 9, 2026

Eligible syntax-only workspace analysis now projects immutable snapshot nodes
straight into the editor's existing owned tree. This removes the intermediate
full `SyntaxTree` and the subsequent node/edge copies, source export, edge labels
and recovery coverage that the editor path does not consume. The final editor
arrays remain dense, and their postorder IDs, spans, children and root are exact.
This is a first step toward snapshot-native editor access, not its completion.

Typed documents, authoring frontends and syntax-only languages with declarations,
references or rule checks retain the established materialized-analysis path.
Projection failures discard partial arrays and take that path too. Syntax-only
problems and repair fixes use the snapshot's existing sidecars; the shared repair
conversion keeps insert/delete behavior identical. No language-specific parser
branches, dependencies, unsafe code or public tree-content changes were added.

The core postorder visitor enforces work and storage-capacity bounds, preserves
multiple placements of shared subtrees and scopes each child-ID slice to its
callback. Runtime access clones the immutable shared snapshot while holding its
store lock, then releases the lock before invoking consumers. Reentrant reads
and edits retain the original view and language safely.

## Correctness evidence

The core regression compares visitor node tuples and child IDs to full export
for a shared subtree at two source positions and rejects zero work before any
output. Runtime tests read and edit a handle reentrantly within a scoped view.
LSP tests compare direct analysis with the materialized oracle through valid,
Unicode, missing-delimiter, skipped-junk and subsequent edits. Comparisons cover
nodes/edges/root, problems, repairs, declarations, values, symbol indexes, both
token palettes, folding, selections, symbols, formatting and completion.
A partial projection work-limit failure falls back without leaking partial data.
Declaration/reference cases retain metadata; typed Portable and XML structural
checks retain their established diagnostics, including XML0001.

Fresh local checks passed: 750 workspace test executions (three ignored), 226
native-only LSP tests, strict workspace all-target/all-feature clippy, formatting,
35 Python release/benchmark/harness tests and patch checks. Four native-only
runtime snapshot tests and the real-binary LSP integration check also passed. Existing
LSP lifecycle, cancellation, typed parity and grammar checks remain suite gates.
No new packaged-extension or manual visual acceptance claim is made here; the
[preceding baseline](benchmark-2026-11-09-editor-responsiveness.md) records the
previous packaged build separately.

## Matched measurements

[Raw results](../2026-11-09/editor-projection.json) include source/input
and executable hashes, CPU/host/toolchain, individual samples and process order.
The baseline executable was retained from `1f2e8c0`; its production Rust and
benchmark source bytes are identical to the merged `c27886a` base. Both builds
use native-only portable release flags, workspace LTO and one codegen unit,
without PGO or inherited Rust flags. This host is an Apple M4 Pro. Build origin
and flags are declarations; hashes identify actual supplied executables.

Each process performs one warm edit and five measured updates. Each update is
compared with fresh analysis outside timing. The candidate additionally compares
its final editor tree with full snapshot export **after all timed iterations**,
so this oracle cannot warm a later sample. The baseline predates that additional
oracle and projection timing field; it uses the same measured loop. No tests or
builds ran alongside measurements. Update always precedes fresh analysis;
cache/order effects and single-host limits remain. These are C# syntax-only
inputs, not evidence about typed semantic throughput or GUI latency.

| C# target | Process pairs | Cached analysis before → after | Reduction |
| --- | ---: | ---: | ---: |
| 256 KiB valid | 1 | 2.277 → 1.952 ms | 14.3% |
| 256 KiB missing semicolon | 1 | 4.891 → 4.299 ms | 12.1% |
| 4 MiB valid | 5 | 38.507 → 31.356 ms | 18.6% |
| 4 MiB missing semicolon | 1 | 46.454 → 32.939 ms | 29.1% |

Tables show medians of process medians. All five large-valid process pairs favor
the candidate (before 37.8–39.3 ms; after 31.1–31.7 ms). Other rows are exploratory
single pairs. Retain this change for the repeated 18.6% improvement and passing
correctness checks, without chaining historical optimization percentages.

For the repeated valid 4 MiB case, the baseline spends 28.44 ms in full
materialization and 4.49 ms in editor processing/copying. The candidate records
23.77 ms as `syntax_projection_ns` and 1.65 ms as editor work. Its
`materialize_ns` is zero because it avoids full export, **not** because syntax
projection is free. These fields represent different stages; total cached
analysis is the primary comparison. Fresh whole-document parsing still takes
about 1.5 seconds in both executables, so this does not solve initial parse cost
or interrupt an obsolete in-flight parse. No allocation-count or retained-memory
reduction is quantified by these timings.

## Reproduction and remaining work

```sh
cargo build --locked --release -p nitride-lsp --example benchmark --no-default-features
python3 scripts/benchmark_lsp.py --before PATH_TO_BASELINE --after PATH_TO_CANDIDATE \
  --baseline-revision c27886a --build-description 'record matched flags and provenance' \
  --output /tmp/editor-projection.json
```

Keep both executables before rebuilding. The runner records the owning core,
runtime wrapper, analysis and example source hashes; the candidate's new module
also contributes to the full dirty-source identity. Source bytes can be checked
against the base with `git diff 1f2e8c0 c27886a -- crates`.

Direct projection still traverses every occurrence and allocates dense editor
arrays. Later work can investigate snapshot-native feature queries or retained
projections. Typed indexing, cold preparation responsiveness and cooperative
parser interruption remain separate projects. Review and hosted results are
reported on the PR rather than inferred from local timings.

Independent review found one Important traversal-budget gap: a wide node could
queue all children before the next visit charged work. A 4,096-child regression
failed before the fix. Child scheduling now consumes budget before expansion,
including the pending-list reversal cost at a constant work factor; sufficient
budgets preserve every occurrence, and partial-output fallback remains checked.
The corrected full suites and matched measurements were rerun. No Critical
findings remained.

Deferred minor: the new `analysis/projection.rs` module is included in the full
dirty-source fingerprint but is not listed with an individual baseline/candidate
hash. The reviewer independently recalculated the earlier run's reported numbers
and checked source/ownership logic; the refreshed post-fix run is the local
implementer's evidence. The reviewer did not run tests/builds/benchmarks or judge
GUI behavior or independently establish historical build provenance.
