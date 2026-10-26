# Stage25: Roslyn baseline and rejected recognition-only trials

Roslyn is the performance target. Stage24 remains the accepted implementation. Recognition-only syntax trials were implemented and measured, then archived and removed because they produced no repeatable gain. No new optimization is admitted by this milestone.

## Fresh Roslyn comparison

| Valid C# target | Nitride Stage24 cold median | Roslyn cold median | Nitride / Roslyn |
|---|---:|---:|---:|
| 256 KiB | 0.114358333 s | 0.073564 s | 1.55x |
| 4 MiB | 1.830206834 s | 0.6728227 s | 2.72x |
| 16 MiB | 7.648903375 s | 1.284679 s | 5.95x |

Three fresh-process runs per backend and size, alternating order, same generated valid C# input hashes. Nitride uses generated syntax/shared snapshots and the ordinary Release allocator. Roslyn5.0.0.0 on .NET10 executes ParseText+GetRoot. Input IO, startup, option construction, post-parse validation and serialization are outside timing. Nitride includes SourceText construction, dispatch preparation and dependency-index construction; tree representations differ. These are host/workload-specific parser measurements, not compiler-conformance evidence.

Separate Roslyn warm medians after three warmups and five measured parses:0.0361582/0.144948/0.6375265s. Nitride warm parsing was not measured, so these are not a warm-vs-warm comparison. [Raw Roslyn baseline](stage25-roslyn-baseline.json).

## Measured speculative construction

A separate temporary diagnostic build counted4MiB:2,458,835 constructed node records and679,720 releases. NullableTypeName failed215,289 times with237,951 inclusive node constructions. VariableDeclaration statement alternatives failed79,317 times with113,310 failed-scope constructions; AliasPrefix failed702,524 times. Inclusive nested scope counts overlap and must not be added. Node records are not allocator allocations, releases may include memo eviction, and diagnostic timing includes instrumentation overhead. Recognition counters matched frozen Stage24 at256KiB and4MiB. The diagnostic overlay was archived and exact Stage24 restored before the production experiment. [Profile summary](stage25-speculation-profile.json).

The experiment selected generic two-reference syntax rules ending in lexical punctuation, without C# rule IDs. A missing marker in a bounded raw-byte window chose recognition-only execution of the actual grammar. Failed trials omitted speculative syntax construction; successful trials replayed ordinary construction. Virtual handles were excluded from lexical payloads, successful Pratt cache entries and published rule facts. Activation was limited to generated cold parsing with exact default limits and default AST options.

## Rejection evidence

Two independent batches, each three alternating fresh-process runs per backend at every size, ordinary allocator, no simultaneous tests/builds. Pooled medians contain six samples per backend/size; percentage is candidate latency relative to Stage24 (positive is slower).

| Target | Stage24 | Recognition trials | Latency change |
|---|---:|---:|---:|
| 256 KiB | 0.116290 s | 0.116254 s | -0.03% |
| 4096 KiB | 1.875390 s | 1.893084 s | +0.94% |
| 16384 KiB | 7.738600 s | 7.791919 s | +0.69% |

Both16MiB batches regressed; small-input apparent gains reversed on repetition. Trial attempts at4MiB were215,289, all failed, with no successful replay on this generated corpus. Logical work and retained storage matched Stage24. This experiment does not establish that construction is universally cheap; it establishes that this implementation/selection did not improve this workload. [Both batches and pooled comparison](stage25-paired.json).

The next priority is reducing repeated recognition and physical lexical/dispatch work. Removing speculative node construction while executing the same grammar work did not reduce the Roslyn gap. Generated predictive decisions or lexical reference-chain specialization need their own measurement and semantic proof; neither is implemented here.

## Validation and preservation

During development, four focused core recognition tests, six CLI generated-syntax tests and the codegen suite passed; strict all-target/all-feature clippy also passed. The final heuristic short-circuit was made before the frozen performance build. These development checks do not constitute full candidate admission. Candidate differential/recovery/edit/reader-window admission was intentionally skipped after the performance criterion failed; any future revival must run those gates.

The complete candidate source overlay, new helper, generated seed, frozen executable and source/executable manifests are preserved in the local plan checkpoint. All eleven owned source snapshots were restored and the new helper removed; the full source digest exactly matches Stage24. Earlier uncommitted Stage15–24 work and sibling repositories are preserved. Fresh restored local checks and the final active Release fingerprint are recorded separately in [validation](stage25-admission.json).

Independent read-only final review supported rejection and found no Critical/Important candidate issue. It did not run tests or establish restoration itself; restoration is verified by the separate exact source/executable audit. Administrative review notes are addressed by the candidate manifest and explicit skipped-gate scope. The source rebuild succeeded but its executable hash differed from the frozen accepted artifact (3,171,584 versus3,171,600bytes); byte reproducibility is unresolved. The rebuilt executable is archived, and the verified frozen Stage24 executable was copied back for exact active artifact identity. This is an artifact restoration, not a claim that the rebuild reproduced the old bytes. No new timing was captured from the rebuilt executable.

No commit, push, backend cutover or Roslyn-parity claim.
