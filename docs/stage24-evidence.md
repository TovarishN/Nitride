# Stage24 fused ASCII lexical decisions

Generated lexical rules now use immutable ASCII decisions when their normalized expression outcome is fully determined by the first byte and consumes zero or one byte. Each decision preserves exact work, maximum relative depth and observed read reach. Existing root entry guards/exclusions remain in place. Executed rule references, unknown suffixes, multi-byte matches, non-ASCII, EOF and insufficient work/depth headroom take the unchanged generated body. No per-parse cache, dynamic allocation, ownership or private-budget change was added. Default/edit/probe recognition remains unchanged.

| Valid C# target | Stage22 median | Fused ASCII median | Speedup |
|---|---:|---:|---:|
| 256 KiB | 0.122067 s | 0.112226 s | 1.088x |
| 4096 KiB | 1.937146 s | 1.785744 s | 1.085x |
| 16384 KiB | 7.990055 s | 7.316087 s | 1.092x |

Two independent three-round fresh-process alternating comparisons per size, same generated valid C#, --native-syntax --snapshot, uninstrumented Release allocator. Six samples per backend per size. No concurrent builds/tests. Nitrogen values are preceding same-host captures from nitrogen-full-parse-comparison.json, not newly interleaved with this candidate.

Both batches improve all three sizes. At 16 MiB current fused parsing is about 1.44x the preceding Nitrogen initial-incremental time; the 5.069 s target is not yet met. This is a reference comparison, not a new paired Nitrogen measurement or Roslyn parity claim.

CSharp4 has 18 tables (18,432 logical static bytes before linker folding). Generated C#-profile Rust source grew from 1,320,522 to1,464,559 bytes. The profiler grew from3,155,088 to3,171,600 bytes. Retained syntax/index representation is unchanged; static table bytes are not per-snapshot allocations.

The fresh Stage22 CPU sample identified class testing and the large ContextualFormat scanner among remaining active costs. It is symbolized diagnostic evidence, not admission timing or a CPU-percentage claim. Initial restrictions deliberately leave runtime reference-chain fusion for later to preserve existing function-entry stack guards.

Validation: 321 workspace test executions; strict all-target/all-feature clippy, fmt, grammar bootstrap fixed point and nine Python driver tests; all 4,334 standard and 1,286 adversarial traces; two bounded 100,000-edit reader windows. New pure compiler tests cover short-circuit choices, empty/optional/repeat cases, observed literal width, refs/suffix decline and resource limits. Runtime tests compare all 128 ASCII bytes over work/frame boundaries, with exact fallback state, EOF/Unicode, overflow/clipping and invalid metadata checks. CLI regressions cover formatting characters, escapes, all parser modes, limits and comment-boundary edits with old readers.

Logical work, memo/cache events and native call/work counters are identical to Stage22 on all measured valid inputs. At 16 MiB that includes 1,030,258,386 total logical work and 506,889,738 native lexical work. The optimization executes fewer physical decisions while retaining semantic accounting. Reclaimed-byte telemetry varies with existing randomized map release ordering and is not asserted equal.

A recovered 16 MiB file's first edit matched fresh parsing in 116.080ms, copying 1,537 bytes, with 0 fallbacks and 0 maintenance passes. Damaged cold parsing is a separate single sample: 74.479 s. Compiled callbacks are not retained by edited snapshots.

The grammar seed was regenerated through the bootstrap example and its fixed point rechecked. The owned-source overlay reconstructs exact Stage22; all other source files remain unchanged. Local changes remain uncommitted. Independent final review found no Critical, Important or Minor issues. It audited supplied execution evidence without rerunning heavy tests. Scope remains this delta and workload/host; prior milestones, universal throughput, fresh Nitrogen superiority, Roslyn/compiler conformance and new recovery capabilities are not re-established.

[Paired samples](stage24-paired.json), [admission](stage24-admission.json), [standard traces](stage24-differential.json), [adversarial traces](stage24-adversarial.json), [recovered edit](stage24-first-edit.json), [reader lifetime](stage24-lifetime.json), [preceding Nitrogen comparison](nitrogen-full-parse-comparison.md).
