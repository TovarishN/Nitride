# Stage22 arena lookup checkpoint

The arena now checks the current chunk identity after its existing dense lookup and before the chunk-directory HashMap fallback. Recycled slots can defeat the dense lookup, while the full current chunk ID still provides a direct route. The check rejects freed slots, stale IDs, published IDs and IDs from other builders. Other live chunks retain directory lookup. No new persistent cache, ownership change, allocation, ID renumbering, budget change or dependency was added.

| Valid C# target | Stage21 paired median | Arena lookup paired median | Speedup |
|---|---:|---:|---:|
| 256 KiB | 0.131812 s | 0.123528 s | 1.067x |
| 4096 KiB | 2.112935 s | 1.949153 s | 1.084x |
| 16384 KiB | 8.679107 s | 8.005329 s | 1.084x |

Two independent three-round fresh-process alternating comparisons per size; same host and generated valid C#; --native-syntax --snapshot; ordinary uninstrumented allocator. Six samples per backend per size, with no concurrent builds/tests during capture. Roslyn reference is historical, not a new interleaved comparison.

Both independent batches improve every size. Full-parse speed improved on this host/corpus; Roslyn parity is not met. Historical Roslyn ratios in the raw report are references, not a fresh head-to-head.

Validation: 310 workspace test executions; strict all-target/all-feature clippy, fmt, grammar bootstrap fixed point, nine Python driver tests; all 4,334 standard and 1,286 adversarial traces; two bounded 100,000-edit reader windows. Three new identity regression tests were RED for the missing private helper, then GREEN. Existing ID-exhaustion/ownership tests remain green.

The first edit of a recovered 16 MiB file validated against fresh parsing in 143.513 ms, copied 1537 bytes, with zero fallback/maintenance. Its single latency capture was slower than the prior Stage21 capture (113.413 ms), despite equal logical work, so a same-input controlled timing investigation was added. The separate damaged cold single sample took 75.090 s.

The edit timing control runs one process per frozen backend, twenty alternating nearby edits with eight retained readers, and reports the first edit separately. Fresh-parse validation is disabled only for this timing control; the admission edit and complete corpora were validated separately. It is not a new first-edit p95 result.

- stage21: first 116.883 ms, median of twenty 111.804 ms, median after first 111.778 ms, maximum 116.883 ms.
- arena_lookup: first 115.915 ms, median of twenty 113.084 ms, median after first 113.079 ms, maximum 115.915 ms.

The large first-edit slowdown was not reproduced in the timing control: first edits were 116.883 ms before and 115.915 ms after. The twenty-edit median was about 1.14% slower (111.804 ms versus 113.084 ms). This small control does not establish universal incremental-latency parity or an edit-speed gain.

The source-overlay audit reconstructs exact Stage21 by replacing only arena.rs. All earlier source checkpoints remain unchanged. Local work remains uncommitted. Independent final review found no Critical or Important issues. One Minor test-coverage gap is deferred: the other-live-chunk test exercises dense lookup rather than deterministically forcing the unchanged non-current recycled HashMap fallback. No correctness blocker was found. The review audited supplied evidence without heavy reruns. Scope remains one host/corpus and this sole source delta; general incremental p95/parity, fresh Roslyn parity and a new independent audit of prior Stage15–21 code are not established.

[Paired full parses](stage22-paired.json), [admission](stage22-admission.json), [standard traces](stage22-differential.json), [adversarial traces](stage22-adversarial.json), [recovered edit](stage22-first-edit.json), [edit timing control](stage22-edit-control.json), [lifetime](stage22-lifetime.json).
