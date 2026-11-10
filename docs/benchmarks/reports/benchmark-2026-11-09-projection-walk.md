# Projection walk record reuse — November 9, 2026

The direct editor projection introduced by [PR #48](https://github.com/TovarishN/Nitride/pull/48)
still repeatedly resolved arena records and queued exit work for leaves. The
visitor now carries immutable node records in its pending frames and completes
leaves inline. Records are reused only within one walk; no state is retained
across edits. The editor still receives the same dense tree.

A leaf still consumes its enter and exit work units. Every scheduled child still
consumes a unit, and the wide-node preflight remains before child expansion and
frame allocation. Postorder IDs, child slices, root, source placement, capacity
checks, emitted prefixes and limit diagnostics remain unchanged. Typed, metadata
and rule-check routing and runtime ownership are untouched. No dependencies,
unsafe code or production instrumentation were added.

## Correctness and cost checks

A thread-local counter compiled only for core tests records arena-record reads,
including reads needed to obtain child iterators. On the 4,096-child fixture,
the old walk failed a bound of at most two reads per occurrence; the new walk
passes. This is a work-cost regression test, not a cache-miss or timing measure.
The counter does not appear in normal benchmark/release dependencies.

An independent recursive oracle checks exact emitted node/child prefixes and
root/error results for budgets 0–31 on shared branches at different positions
with source gaps. It passed before and after the optimization. Existing wide
budget, materialized-tree, recovery, editor-feature and runtime reentrancy tests
remain gates.

Local verification passed: 751 workspace test executions (three ignored), 226
native-only LSP tests, five native CLI/runtime tests, strict all-target/all-feature
workspace clippy, formatting and patch checks. All 37 Python tooling tests passed.
The new provenance tests cover a newly added module, a changed existing source,
and invalid historical revisions.

## Matched measurements and the repeat

[Full run](../2026-11-09/projection-walk.json) and
[primary-case repeat](../2026-11-09/projection-walk-repeat.json) retain
all raw observations. Both runs use the same two prebuilt executables on this
Apple M4 Pro host: native-only portable release, LTO, one codegen unit, no PGO
or inherited Rust flags. The baseline is PR #48's corrected candidate, whose
hash matches its earlier raw report and whose source bytes match merged
`1dcd65e`. The benchmark example, fresh-analysis comparisons and final full-tree
oracle are identical in both executables. Build origin/flags are declarations;
recorded hashes identify the actual supplied files.

Each process makes one warm edit and five measured updates, always updating
before fresh analysis. Each update compares problems, values and tree/edges
against fresh analysis outside timing; each final tree also matches full export
after all timed iterations. The runner alternates executable order between process
pairs. Medians below are medians of process medians. No builds or tests ran
alongside either measurement round.

| Case / round | Process pairs | Cached analysis before → after | Reduction |
| --- | ---: | ---: | ---: |
| 256 KiB valid / full run | 1 | 2.007 → 1.991 ms | 0.8% |
| 256 KiB missing semicolon / full run | 1 | 4.485 → 4.339 ms | 3.3% |
| 4 MiB valid / full run | 5 | 31.861 → 30.207 ms | 5.2% |
| 4 MiB missing semicolon / full run | 1 | 35.622 → 32.540 ms | 8.6% |
| 4 MiB valid / independent repeat | 5 | 30.868 → 27.964 ms | 9.4% |

Four of five primary pairs improved in the first round; the fifth regressed from
31.291 to 34.231 ms. Its fresh, parse-update and editor phases also slowed, but
these observations do not establish a cause. That inconsistency prompted one
repeat of the preselected primary case using the same binaries. All five repeat
pairs improved. Both rounds and every sample are retained, with no outlier
exclusion. Nine of ten primary pairs favor the candidate. The smaller and recovery
rows are exploratory single pairs; the 0.8% small-valid change is not evidence of
a meaningful general improvement there.

For the primary case, projection medians fall from 24.238 to 22.096 ms in the
first round and 23.476 to 20.695 ms in the repeat. Keep the bounded internal
change for the lower record-read cost, passing equivalence checks and repeated
large-document median improvements. Report the two rounds separately: do not
choose only the stronger result or chain percentages with PR #48. This does not
establish other-machine, typed semantic, initial-parser or GUI improvements.
No allocation or retained-memory reduction is quantified.

## Provenance and reproduction

Fresh reports now record the projection module's individual before/after hashes,
closing PR #48's deferred provenance minor for subsequent measurements. A file
absent in an older baseline is explicitly `null`; an invalid revision still
fails. Historical reports are unchanged.

```sh
cargo build --locked --release -p nitride-lsp --example benchmark --no-default-features
python3 scripts/benchmark_lsp.py --before PATH_TO_BASELINE --after PATH_TO_CANDIDATE \
  --baseline-revision 1dcd65e --build-description 'record matched flags and provenance' \
  --output /tmp/projection-walk.json
# Repeat just the predefined primary case, retaining the original round:
python3 scripts/benchmark_lsp.py --before PATH_TO_BASELINE --after PATH_TO_CANDIDATE \
  --baseline-revision 1dcd65e --large-valid-only \
  --build-description 'same binaries, independent primary-case repeat' \
  --output /tmp/projection-walk-repeat.json
```

Keep both executables before rebuilding. Individual source fingerprints, combined
source identity, input and executable hashes, host and toolchain are in the raw
reports. Cold preparation, obsolete in-flight parsing and dense editor-tree
rebuilding remain distinct costs. Review and hosted checks are reported on the PR.

Independent whole-branch review found no Critical/Important issues or actionable
minors. It checked cached record identity/placement, immutable ownership, leaf
exit charging, wide preflight, capacity checks, callback/error equivalence and
test-only instrumentation. The reviewer recalculated every process/round median
and checked source/executable hashes, including the baseline's prior artifact.
The reviewer did not run tests/builds or independently establish build flags,
measurement isolation, GUI behavior, general performance or statistical
significance. Local execution evidence and declared build procedure remain
separate from that static review.
