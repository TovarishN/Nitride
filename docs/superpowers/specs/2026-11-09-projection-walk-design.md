# Projection traversal record reuse

The merged direct projection still resolves arena records repeatedly per
occurrence. Carry immutable StoredNode data in pending frames so entry/exit
processing does not resolve it again. Complete leaves without queuing an exit
frame or opening an empty child iterator. Preserve exact postorder nodes, child
IDs, root, placement offsets, callback order and existing work-budget semantics:
every enter, exit and scheduled child still consumes its original unit even if
an internal frame is elided. Keep the pre-expansion wide-root check and storage
capacity checks. No new dependencies, unsafe code or public result changes.

A test-only, thread-local record-read counter will pin the reduction on a wide
leaf-heavy tree without wall-clock assertions or production instrumentation.
Compare outputs, emitted prefixes and errors across small budgets against a
legacy reference traversal for mixed nodes. Existing ownership, recovery and
editor feature oracles remain gates. Retain only with matched cached-analysis
measurements; do not infer typed, initial-parser or GUI improvements.

Record each owning source file explicitly in benchmark artifacts, including the
projection module. A file absent in a historical baseline is represented as null;
a bad baseline revision must still fail. This closes PR48's deferred minor for
new measurements while preserving historical reports.
