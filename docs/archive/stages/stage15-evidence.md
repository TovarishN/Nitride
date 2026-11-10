# Shared subtree evidence

Status: both requested reuse paths are implemented locally. Functional, retention and all instrumented/allocator-free edit gates pass. Performance admission is blocked by cold-cost gates; the complete instrumented matrix records 15 cold-time and two peak-memory misses. All six allocator-free cold comparisons also miss the time limit. Both complete reports are frozen. This is opt-in, with the existing flat APIs and recognizer retained as the compatibility oracle. No staging, commits, push, merge or hosted CI run is included.

## Delivered behavior

`parse_snapshot` shares immutable syntax chunks, relative child placements and UTF-8 source pieces. Pratt memo hits, longest-match winners and recovery probes select structural handles without copying descendants. `ParseSnapshot::edit` invalidates dependency intervals, shares compatible recognition facts, and rebuilds affected ancestors. Unchanged wide child runs are persistent slices, including between separated dirty regions. Old readers retain their original source, placements and syntax.

Recovery uses the existing grammar-driven search policy with shared strict probes. Repairs, coverage, damage and diagnostics use persistent relative records. Borrowed repairs/diagnostic slices are lazy exports. Failures and observations retain dependency bounds; disposable failed probes are bounded and only selected recovery frontiers publish failures. A result containing a resource-limit diagnostic explicitly uses the old recovery backend to preserve its exact limit behavior. The fallback is counted, and its copy count is unknown rather than reported as zero.

`NodeView` resolves snapshot occurrence positions. `materialize()` explicitly copies a flat compatibility tree. `compile_snapshot` and `compile_snapshot_recovering` consume occurrence views but recompute semantics; semantic indexing/export costs are separate from parse/edit measurements. No editor integration or incremental semantic analysis is included.

## Final functional validation

- `cargo test --workspace --locked`: 269 top-level tests plus one nested small-stack subprocess test passed (270 successful executions), zero failures, three ignored scaling/lifetime tests.
- All-target, all-feature strict clippy, formatting and grammar bootstrap fixed point pass.
- Frozen final executable: 4,334 standard forward/undo traces and 1,286 adversarial multi-edit/undo traces, zero mismatches. See [standard](stage15-differential.json) and [adversarial](stage15-adversarial-shared.json) reports.
- Seeded strict edits across every fixture profile, all eight parser modes, grammar self-source, typed HIR/binding/origin comparisons and concurrent old readers pass in the workspace suite.
- Wide 32,768-item edits across direct, nested and per-item-label grammars: allocation requests under 4.1 KiB, total allocation under 55 KiB; 512 copied published child bytes. Distant edit plus existing damage: 64 recovery child scratch bytes and 1,024 copied published bytes; exact tree, repairs and coverage parity. These regressions previously allocated a 786,432-byte strict temporary and 524,304 recovery child payload bytes.
- Python measurement-driver tests: nine passed, including failure exit status, preserved mismatches, null uninstrumented allocator fields, exact percentile/warmup handling and physical allocation deduplication.
- Live reference/preparation checks and the unchanged legacy Release scaling/work tests passed earlier in this worktree. Their evidence remains separate in [reference](stage15-reference.json), [recovery quality](stage15-recovery-quality.json) and [adversarial recovery](stage15-recovery-adversarial.json); subsequent changes concern the opt-in backend.
- Refreshed source-only `cargo build --release --locked --offline`: passes with dotnet absent from PATH, fresh CSharp4 preparation/recovery and shared snapshot profiling, zero core third-party dependencies. [Native deployment](stage15-native-deployment.json).

## Retention and destruction

Final 100,000-edit tests passed for zero/eight-reader windows. Fresh storage is 2,224 bytes; latest storage is 111,416 bytes, under the 8,523,016-byte bound, with 155 explicit maintenance passes. The eight-reader union is 120,408 bytes. Deliberately retaining 1,001 snapshots uses 1,634,288 unique bytes versus 157,146,656 logical bytes; latest alone is 204,696 bytes. Dropping all readers returns tracked net dynamic storage to zero. [Lifetime data](stage15-lifetime.json).

Maintenance is explicit: a warm pass evacuates all reachable syntax when shared chunks prevent in-place reclamation, may change identities in the new snapshot, and leaves old readers valid. Its time and copied bytes remain in edit statistics and gate evaluation.

## Measurement protocol

Frozen old executable/source `88cbc93` and [baseline](stage15-baseline.json) are unchanged. The instrumented [matrix](stage15-scaling.json) uses three process-cold samples per input; 256 KiB, 4 MiB and 16 MiB generated CSharp4; strict, one missing semicolon, dense missing semicolons, local junk and truncation. Strict/one-error edits use early/middle/late/alternating locations and retained-reader windows zero/eight, ten warmups plus 100 timed edits each. Each report retains source/executable hashes, raw phases and every failed gate.

Requested allocator bytes exclude caller input, allocator overhead, stacks and OS mappings. Phase peaks are never added. Parse measurements exclude compatibility export, JSON serialization and semantics. Storage includes source, syntax, dependency index and sidecars. Timed edit runs do not repeat full equivalence checking; independent validated manifests provide that evidence, and validated sample counts remain explicit.

The separate [uninstrumented timing report](stage15-latency-uninstrumented.json) compiles out the counting allocator, leaves allocator metrics null and compares cold timing with the same executable's independent unchanged legacy parser. It covers strict and one-error inputs at every size; it does not substitute for memory admission against the frozen baseline. This is one macOS development host, not a dedicated benchmark machine. Earlier provisional runs overlapped validation and are retained only under /tmp; final matrix starts after validation processes finish.

Numeric admission requires cold median <=1.25x time, <=1.50x peak and <=1.35x retained; local edit p95 <=50/100/200 ms and <=20% cold; copied existing source/syntax <=256 KiB. No threshold is relaxed after measurement. All 15 instrumented cold-time gates fail; strict 256 KiB and 16 MiB also fail the peak gate. Every retained-memory gate passes. The frozen tables below retain every missed gate. The earlier 4 MiB recovered-edit p95 around 152 ms came from recomputing the gap between an edit and unchanged damage; it is preserved in the superseded raw report, with a coverage-byte regression and a fix before the current capture.

## Review and limitations

The independent final reviewer identified four Important issues: wide temporary child vectors, quadratic/unbudgeted recovery metadata work, ignored measurement equivalence failure status and a capacity error converted to panic during evacuation. Each has a reproducing RED-to-GREEN regression and a fix; the final workspace suite is green. The reviewer reached its usage limit before delivering a final formatted verdict. Final author verification is available; independent post-fix sign-off is unavailable.

Deferred minor: cross-piece semantic text lookup may allocate before consulting an existing lazy cache. This affects semantic/export allocation cost, outside recognition/edit timings.

The complete decisions, test progression and compatibility tradeoffs are recorded in [the execution ledger](stage15-progress.md). Hosted Linux/macOS/Windows CI is unverified because no push is authorized. The managed detached worktree and all local changes remain available for review.

## Superseded first-edit failure

The pre-recognition-only 16 MiB recovered run exposed a 325.462-second first edit, 220,875,024 copied syntax bytes and 3.18 GB requested peak. It occurred in a warmup and was incorrectly excluded from the copy/fallback/equivalence gate summary. The driver now excludes warmups only from latency percentiles and reports their maximum separately; all edits participate in other gates. The old raw capture is preserved under /tmp/nitride-shared-pre-recognition-only-scaling.json. Prior zero-copy-miss summaries are superseded. The recognition-only predicate experiment was rejected because it increased recovery work and failed the existing dense budget. The accepted fixes bound construction hints, use sorted directory fences, respect the approved maintenance allowance, consume sound prefix facts and preserve facts whose history-owned roots cannot relieve eviction pressure. The first 16 MiB edit now matches full legacy recovery at 108.958 ms, 1,536 copied syntax bytes, 63 recomputed coverage bytes and 22.45 MB requested edit peak, with no maintenance/fallback. This diagnostic overlaps validation; the completed clean instrumented edit capture below provides the p95 evidence.

The selected-fact regression replays the exact selected ID after 4,999 unused probes under a 1 KiB cache budget. Its cold index already contains the reusable facts; first-edit reconstruction no longer fills a missing ~6.3 MiB prefix. See [raw first edit](stage15-first-edit.json).

## Completed capture audit

Both full reports are complete: 45 instrumented cold samples; 36 allocator-free shared/legacy cold samples; 96 edit cases, 9,600 timed edits and 960 warmups. All edit gates pass, including all-sample copy/fallback checks. Timed equivalence is disabled; parity evidence comes from the independent validated manifests and the first-edit reproduction. Every retained-memory gate passes. [Machine-readable admission audit](stage15-admission.json).

Source/workload hashes match the frozen manifests, current implementation and baseline inputs. Memo/winner/probe selection copies zero descendants in all 10,608 samples with known counters. Fifteen instrumented cold samples explicitly use legacy fallback and report unknown copying: 256 KiB junk/truncated, 4 MiB junk, and 16 MiB dense/junk, each with three repetitions. No edit sample falls back. Allocator-free phase allocation fields are verified null, not zero.

## Frozen numerical results

Instrumented cold medians; ratios compare the immutable old baseline. Values above 1.25x time, 1.50x peak or 1.35x retained fail.

| Target size | Pattern | Seconds | Time ratio | Peak MiB | Peak ratio | Retained MiB | Retained ratio |
|---|---|---:|---:|---:|---:|---:|---:|
| 0.25 MiB | strict | 0.695 | 1.642x | 9.66 | 1.507x | 4.36 | 1.334x |
| 0.25 MiB | one | 1.044 | 1.576x | 13.09 | 0.792x | 5.96 | 0.951x |
| 0.25 MiB | dense | 4.758 | 1.445x | 20.71 | 1.054x | 6.35 | 0.986x |
| 0.25 MiB | junk | 1.484 | 2.225x | 17.23 | 1.030x | 5.31 | 0.847x |
| 0.25 MiB | truncated | 1.378 | 2.107x | 16.95 | 1.023x | 3.98 | 1.008x |
| 4 MiB | strict | 10.625 | 1.593x | 138.05 | 1.379x | 67.93 | 1.334x |
| 4 MiB | one | 16.928 | 1.650x | 170.35 | 0.762x | 92.78 | 0.938x |
| 4 MiB | dense | 100.219 | 1.983x | 247.34 | 0.870x | 101.98 | 1.003x |
| 4 MiB | junk | 23.762 | 2.322x | 243.28 | 1.080x | 82.58 | 0.835x |
| 4 MiB | truncated | 17.028 | 1.655x | 127.32 | 0.639x | 68.96 | 1.107x |
| 16 MiB | strict | 44.053 | 1.663x | 537.12 | 1.721x | 264.71 | 1.321x |
| 16 MiB | one | 84.979 | 2.071x | 670.18 | 0.833x | 362.39 | 0.924x |
| 16 MiB | dense | 384.976 | 1.924x | 1053.03 | 1.031x | 331.19 | 0.821x |
| 16 MiB | junk | 112.674 | 2.769x | 960.35 | 1.193x | 324.82 | 0.828x |
| 16 MiB | truncated | 73.226 | 1.794x | 501.99 | 0.684x | 270.96 | 1.099x |

Worst case across early/middle/late/alternating positions and zero/eight readers. Every row contains eight cases with 100 timed samples each.

| Target size | Mode | Worst p95 ms | Warmup max ms | Absolute limit ms | Maximum copied bytes (all edits) | Fallbacks (all edits) |
|---|---|---:|---:|---:|---:|---:|
| 0.25 MiB | strict | 1.339 | 1.256 | 50 | 1,201 | 0 |
| 0.25 MiB | one | 20.281 | 20.602 | 50 | 2,225 | 0 |
| 4 MiB | strict | 4.059 | 5.414 | 100 | 1,281 | 0 |
| 4 MiB | one | 20.127 | 20.128 | 100 | 3,489 | 0 |
| 16 MiB | strict | 14.325 | 16.209 | 200 | 1,281 | 0 |
| 16 MiB | one | 106.109 | 113.915 | 200 | 1,793 | 0 |

Uninstrumented same-build comparisons (allocator fields unavailable):

| Target size | Mode | Shared cold s | Legacy cold s | Ratio | Worst edit p95 ms |
|---|---|---:|---:|---:|---:|
| 0.25 MiB | strict | 0.720 | 0.409 | 1.760x | 1.171 |
| 0.25 MiB | one | 1.110 | 0.631 | 1.760x | 21.562 |
| 4 MiB | strict | 11.052 | 6.358 | 1.738x | 4.510 |
| 4 MiB | one | 17.260 | 9.716 | 1.776x | 19.953 |
| 16 MiB | strict | 44.038 | 25.075 | 1.756x | 14.353 |
| 16 MiB | one | 84.360 | 38.341 | 2.200x | 120.382 |

Complete failed-gate records remain in the reports; nonzero driver exits mean admission failed, not a lost report.

- Instrumented: cold elapsed_ns: 15, cold peak_bytes: 2.
- Uninstrumented: cold elapsed_ns: 6.

Executable hashes (SHA-256):

- Instrumented: `47018108a9810da468987d9d7082dd1ab7a3b13f71b3105d834a979f91b90c1d`.
- Uninstrumented: `65d37b83069784785621b8aa158ba6907092bc5cfb155d7b37ff22d09bd033e4`.
- Source: `f19077f2e0b8e6ec2004d71bb287da77ee90e477abb1c9c411d6f1cc7140c439`.

