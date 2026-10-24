# Inferred types and exact nominal overloads

Stage six adds explicitly opted-in initializer type inference and exact nominal
operation overloads. The inference sample returns `[5,6,3]`, with concrete
AddScalar, Duration, AddTime and SubTime operations visible in HIR/host traces.

## Contract and implementation

Catalog statement `infer:true` admits an unannotated sequential declaration.
Explicit annotation and inference policies cannot mix on one rule. Binding records
declaration identities; iterative lowering obtains inferred reference types from
already-lowered initializer HIR. Shadowing initializers see previous bindings.
Binding finishes before type checking; no checking operation invokes host code.

Overload groups match exact input type tuples. Result type does not disambiguate,
and no implicit conversion or retry in another symbol namespace occurs. Codegen
rejects empty/unknown groups, bad arities and duplicate input tuples, including
identical inputs with different results. Chosen concrete operation signatures use
the existing whole-program preflight and execution path. Failed checks publish no
HIR. Initializers execute once; later references read stored results.

## Reference boundary and review repair

The native grammar is pure `languages/inference/Inference.ngr`. Nitrogen's
DeclarativeTypes.TypeOfSymbol does not infer initializer types. The generated
`Inference.Reference.ngr` and `InferenceSemantics.cs` explicitly provide fixture
symbol type properties and exact overload selection. They are development/editor
adapters, not native build inputs or arbitrary C# translation. Their provenance
file hashes source grammar, catalog and generated outputs. Reference observations
use actual Nitrogen parsing, binding, symbol properties and typed HIR, with an
explicit fixture host for numeric execution.

Independent review found a mixed-error ordering mismatch: the old oracle returned
Nitrogen file diagnostics before its adapter-added initializer mismatch check.
For example, `let x:Scalar=duration(1); value(x)+duration(2);` selected the later
overload error in the reference but the earlier initializer error in Rust.
Four new frozen regressions reproduced the gap. The inference-only oracle now
combines actual file diagnostics with initializer consistency facts obtained from
actual symbol properties and nested HIR, then selects the earliest source span.
This defines the adapter's primary-error policy; raw Nitrogen is not claimed to
implement that combined policy. Other profile oracle behavior is unchanged.

## Validation

- 68 Rust tests pass, including 33 frozen inference cases, initializer reuse,
  exact selected-operation preflight before effects, and a 1,000-declaration chain.
- 108 frozen runtime observations and 256 generated programs match the live
  references in total (inference contributes 33 and 64 respectively).
- 12 exporter tests pass. Native preparation matches 70 valid profiles; both
  preparers reject 14 invalid grammar cases. Formatting, strict clippy, locked
  release builds and the .NET reference build pass.
- Focused actual Nitrogen workspace inputs include root metadata, generated
  reference grammar/helper and sample. The valid sample passes; temporary
  `let x=duration(1); value(x)+2;` fails with INF0001. The file was removed.
  Live VS Code/Rider interaction remains unverified.
- A clean source copy excluding tools/, every language.json, all language C#
  helpers and every .Reference.ngr builds offline with .NET absent from PATH.
  Native preparation and sample execution succeed in that environment. The host
  still has .NET installed elsewhere; this proves build/runtime independence.

## Measured footprint

macOS arm64 release CLI: **684,816 B (668.8 KiB)**, +16,800 B from stage five.
Profiler: **469,456 B**, +16,784 B. Core third-party dependencies remain zero.

The new inference workload has 1,000 pairs: 2,000 declarations, 2,000 references,
4,000 statements, 81,342 source bytes, 26,001 syntax nodes and 10,000 HIR nodes.
Three runs agree on requested-byte metrics and allocation counts:

| Phase | Retained | Incremental peak | Allocations |
|---|---:|---:|---:|
| Parse | 1,523,134 B | 1,540,944 B | 49 |
| Bind/check/HIR | 802,816 B | 1,313,944 B | 8,093 |
| Execution | 81,920 B | 266,416 B | 4,045 |

Raw data: [inference-memory.json](inference-memory.json). These are requested
System allocation bytes with independent phase baselines, not RSS. Caller input,
static tables and allocator overhead are excluded; owned parser source is counted.
JSON serialization occurs after snapshots. Peaks cannot be summed as a process
peak. No throughput or process-memory improvement is claimed from this new workload.

For the unchanged namespace workload, HIR peak falls from 839,960 B to 692,504 B
with compact declaration identities; retained bytes and allocation counts remain
unchanged. Compare [stage5-namespace-memory.json](stage5-namespace-memory.json)
with [namespace-memory.json](namespace-memory.json). Standard workload results
are in [phase-memory.json](phase-memory.json).

Full DateCalc, general C# helper translation, callable functions, nested scopes,
recovery, incremental editing and native editor integration remain deferred.
Only macOS was executed; Linux/Windows CI is configured but unrun. Work is local,
uncommitted and unpublished; Nitrogen product code is unchanged.
