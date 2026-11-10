# Parser scratch allocation reduction

Continue the approved small-memory native-engine work. Keep syntax, diagnostics,
limits, tree ownership, typing and execution unchanged. No new grammar features,
allocator changes, commits or publication.

The current parser allocates child vectors for each rule/infix attempt and
copies each Pratt winner into fresh vectors. Reuse cleared scratch vectors
within a single parse; active recursive frames retain exclusive ownership.
Drop all scratch storage before returning the owned syntax tree. Keep the core
dependency-free and safe Rust. This is an implementation choice within the
existing architecture, not a new compatibility profile.

1. Add an allocation regression using the existing isolated profiler: 1,000
   independent statements and XML elements must each parse with fewer than
   1,000 allocations. Existing measured baselines are 29,036 and 11,039.
   Confirm failure first. Avoid timing assertions.
2. Reuse child and Pratt snapshot storage through parser-local free lists.
   Preserve rollback/ambiguity/precedence behavior and exact retained storage.
3. Run all Rust, exporter, frozen and generated parity checks; rebuild release
   and repeat isolated memory measurements. Preserve the previous raw report.
4. Obtain one fresh review, address material findings and record evidence.

Acceptance is a substantial allocation reduction with unchanged parity and no
unexplained memory-peak regression. Global pools, unsafe parser code, compacting
trees, memoization and grammar rewrites are outside this pass.
