# Stage21 memo key hashing checkpoint

The private memo key hashes all eleven semantic bytes in one call instead of four tuple-field calls. It retains std randomized HashMap, full equality, the same key/entry size, both memo generations and the existing ownership/eviction policy. This applies to shared parsing, including interpreted edits and probes; recognition work is unchanged. There are no new dependencies or default backend changes.

| Valid C# target | Stage19 paired median | Memo key paired median | Speedup |
|---|---:|---:|---:|
| 256 KiB | 0.135375 s | 0.130629 s | 1.036x |
| 4096 KiB | 2.240930 s | 2.146724 s | 1.044x |
| 16384 KiB | 9.180052 s | 8.886307 s | 1.033x |

Two independent three-round fresh-process alternating comparisons per size, same host, same valid generated C# input, --native-syntax --snapshot, uninstrumented allocator. Pooled medians have six samples per backend per size. No concurrent builds or tests during capture. Roslyn is a preceding same-host capture, not interleaved.

Both three-round batches improved all three sizes. These are modest measured gains, not Roslyn parity. The historical Roslyn reference is in the raw paired report; host drift prevents treating that ratio as a new head-to-head measurement.

Validation: 307 workspace test executions, strict all-target/all-feature clippy, fmt, grammar bootstrap fixed point, nine Python driver tests, all 4,334 standard and 1,286 adversarial traces. New RED/GREEN key tests preserve every field under forced hash collisions and the original cache-entry size. Existing whole-parser tests cover lexical contexts, precedence, diagnostics and limits. Two 100,000-edit reader windows remain bounded.

The first edit of a recovered 16 MiB file validated against fresh parsing in 113.413 ms, copied 1537 bytes, with 0 fallbacks and 0 maintenance passes. Damaged cold parsing is a separate single sample: 79.468 s.

The source-overlay audit reconstructs the exact Stage19 source digest by replacing only shared.rs and removing the new memo.rs. Prior implementation files are unchanged. Local changes remain uncommitted. Independent final review found no Critical, Important or Minor issues. Its read-only audit checked supplied execution evidence without rerunning heavy tests. Review and measurement scope remains this host/corpus and the two changed source files; cross-platform/universal gains, fresh Roslyn parity and a new audit of unchanged Stage15–19 code are not established.

[Paired full parses](stage21-paired.json), [admission](stage21-admission.json), [standard traces](stage21-differential.json), [adversarial traces](stage21-adversarial.json), [recovered edit](stage21-first-edit.json), [reader lifetime](stage21-lifetime.json).
