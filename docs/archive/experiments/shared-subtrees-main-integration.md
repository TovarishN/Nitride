# Shared-subtree parser integration into main

Parser stack `d3215aeb5faf1f64dfb272a10a67d6fe3f0fbd31` integrated with current main `2b49f275fa054bb03e04d3cbdc4947a11ae51866`. Accepted Stage15–24 code and Stage25 rejected-experiment evidence are preserved; rejected trial code remains absent.

The merge keeps main's borrowed grammar/LSP APIs and the shared snapshot backend's static grammar boundary. Semantic syntax access and the common recovery engine carry grammar lifetimes. Materialized snapshots keep static grammar ownership independently of the source snapshot. Main's owned diagnostic codes are retained and exported correctly, and both retained/exported allocations are counted; the new focused regression failed before the accounting fix and passed afterward. The grammar seed was regenerated with the combined generator and main's grammar.

Validation: 431 Rust test executions passed (3 ignored), strict all-target/all-feature clippy, formatting, grammar bootstrap fixed point, nine Python driver tests, Release workspace build, four native CLI smoke cases and all12 native language preparations. Independent read-only merge review found no Critical, Important or Minor issues. These are local-host results, not hosted cross-platform CI.

Historical benchmark reports retain their original source/executable identities. No new throughput comparison is claimed for the merged revision. Local merge only; no remote push requested. The unrelated primary LSP checkout and local ignored checkpoints remain outside this integration's mutations.

[Machine-readable checks](shared-subtrees-main-integration.json).
