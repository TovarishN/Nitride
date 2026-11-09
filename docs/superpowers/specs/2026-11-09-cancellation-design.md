# Cooperative cancellation of obsolete parsing

Add an opt-in core cancellation scope. A cloneable token wraps a thread-safe
predicate (including an Arc<AtomicBool> adapter). Nested scopes combine tokens;
RAII restores prior context after return or unwind. Existing signatures and Copy
Limits remain unchanged. Parser/recovery instances capture the scope once, then
poll at batched work checkpoints; no atomic or TLS access per character/step.
Capture and restore the scope on every parser/recovery spill thread.

Cancellation is a distinct `cancelled` diagnostic, never a syntax-repair trigger
or reason to restart full parsing. Check strict/shared/generated parsing,
recovery search/resynchronization and shared recovery metadata. Snapshot parse
and edit must return cancellation without publishing a partial snapshot; old
readers remain valid. Editor projection and full export use checkpoints too.
The LSP worker enters the scope with its existing obsolete-job flag. Boundary
checks prevent cancelled analysis from entering expensive fallback or publishing
results. A request waiter cancellation alone does not cancel diagnostic work.

Checkpoints bound logical parser/recovery work between polls, not wall-clock
execution of an individual primitive, grammar-table preparation, semantic/host
calls, bulk index/arena maintenance, mutex waits, OS allocation or destruction. These remain explicit limits. No forced
thread termination, unsafe code or new core dependencies.

Use deterministic predicates/gates to prove in-flight strict/recovery/edit and
spill cancellation, scope restoration, reader safety and the latest job following
an obsolete job. Measure signal-to-return latency on real large-file workloads,
and matched normal/false-token overhead. Preserve all raw results; do not claim
a speedup or universal cancellation deadline from single-host samples.
