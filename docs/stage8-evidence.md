# Typed user-defined template bodies

Nitride now admits the existing declarative template clauses for document-local
function definitions, typed parameters, nested and forward calls, and inferred
body result types. The sample returns `[-6,6]`. Definitions are compiled away into
concrete catalog-operation HIR; they are not host operations or runtime closures.

## Semantics and preparation

`declares function Name export scope lowers template Body(Params)` introduces a
function and its parameter scope. Parameters use nonsequential typed declarations;
`references function Name lowers expand Name(Args)` resolves and expands a call.
Zero, one and multiple parameters use separated literal-delimiter lists. The
frontend normalizes those lists into existing sequence/repeat parser tables; no
new runtime parser form is required. Other separator forms remain rejected.

Global function names permit forward references. Parameter names are scoped to
their owning definition. Duplicate functions and parameters are errors even when
unused. References outside parameter scopes remain unresolved; aliases bind but
fail the requirement that a called symbol be a template. Exact arity and nominal
input types are checked, with result types inferred from body facts.

Every body and argument is validated before executable HIR is published, including
unused definitions and unused arguments. Type facts in recursive components are
propagated to convergence; constraints are checked using the stable facts. Pure
unknown-result cycles remain unknown. Used cycles block lowering, while unused
consistent cycles follow the reference. Every inference/expansion/materialization
visit consumes a work budget; recursive paths have a depth bound capped at 128.
Exponential materialization returns a structured limit error rather than executing.
Malformed public syntax storage and shared/detached template tree children diagnose.

Template substitution preserves actual Nitrogen effects: a repeated parameter
repeats argument computation, and an unused argument contributes no executed HIR.
Arguments are still lowered before body substitution, so an invalid or cyclic
unused argument is not silently accepted. This differs from eager once-only
function invocation. Existing built-in call and initializer behavior is unchanged.
All final roots undergo existing exact whole-program host preflight before effects.

This first profile allows fixed catalog operations in bodies, not mixed builtin
call policies, overload policies or sequential captured globals. General lexical
scopes, closures, cross-file exports and runtime callable values remain deferred.
The export spelling is preserved for correct single-document binding; a multi-file
native project model is not admitted.

## Real reference and origin evidence

Both implementations consume the same pure `Functions.ngr`; no C# semantic-property
adapter supplies the user-defined-body behavior. The development/editor helper only
registers catalog types and signatures. The oracle uses actual Nitrogen parsing,
binding, declarative types and HIR, followed by an explicit numeric fixture host.

As with prior profiles, the oracle selects statement roots with LowerSelected.
Nitrogen's registered-root Lower API rejects identity-result expansions, whereas
the selected-root API admits them. When a cycle has no inferred result, selected
lowering diagnoses the unsupported outer statement; when a result is known, it can
reach the closing call's cycle diagnostic. These real API-specific observations
are frozen, not patched into Nitrogen or claimed as full-Lower parity.

Expanded nodes preserve complete ordered call, definition, parameter-use and
argument origin chains. The primary span remains the first origin. Program stores
flat span data plus per-node ranges only for this backend; ordinary HirNode and
Binding layouts remain unchanged. Materialization duplicates argument subgraphs
where template substitution requires repeated execution, keeping existing linear
postorder execution unchanged.

## Verification and review repair

- **87 Rust tests** pass, including **45 frozen function cases** comparing syntax,
  chosen declarations, diagnostics and spans, full HIR origins, values and traces.
- **64 generated function programs** match live Nitrogen/native release. Prior
  gates remain valid: **189 frozen observations** and **384 generated comparisons**
  in total across profiles.
- **15 exporter tests** pass. **76 valid preparation profiles** match live .NET
  export; both preparers reject **16 invalid grammars**, including unsupported
  separated-list delimiter forms. Formatting, strict clippy, locked release and
  reference builds pass.
- Effect tests pin duplicate/unused arguments and whole-program host preflight.
  Limit tests bound exponential expansion. Public-storage regressions return
  structured diagnostics instead of panicking or traversing a shared syntax DAG.

One independent review found a cyclic type-cache bug: an in-progress query's
transient unknown result was permanently cached, so unused
`def f(x: Scalar)=duration(call f(x));` was accepted instead of reporting that
Duration cannot consume the eventual Time result. It also changed closing-cycle
spans in mutually recursive Scalar-returning bodies. Five new actual-reference
regressions reproduced these failures before the repair. Fixed-point propagation
and final-pass constraints now match all five, with the complete suite green.
The reviewer ran 23 probes and reported no other actionable findings. No second
review was substituted for test-first verification of the repair.

Actual Nitrogen workspace loading uses explicit root metadata, pure grammar,
catalog helper and sample inputs. The sample passes; a temporary
`def f(x: Functions.Time)=x; call f(2);` reports NT0001 and is removed. Live editor
interaction remains unverified.

A clean source-only copy excludes tools/, every language.json, language C# helper
and .Reference.ngr. It builds offline and prepares/runs the function sample with
.NET environment entries removed and PATH restricted to system tools. Values are
`[-6,6]`; the trace includes two Neg invocations for the repeated argument. The
machine still has .NET installed elsewhere. The CI matrix includes this fixture;
see the stacked feature PR's current hosted checks.

## Measured footprint

macOS arm64 release CLI: **801,104 B (782.3 KiB)**, +66,432 B from stage seven.
Profiler: **519,600 B**, +49,872 B. The Rust core still has zero third-party
dependencies. This measures the combined native tooling/fixture executable.

The new 1,000 definition/call-pair workload contains 2,000 declarations (functions
and parameters), 3,000 references, 50,669 source bytes, 24,001 syntax nodes and
5,000 executable HIR nodes. Definitions are not executable statements; there are
1,000 output roots. Three runs agree on requested-byte/count metrics:

| Phase | Retained | Incremental peak | Allocations |
|---|---:|---:|---:|
| Parse | 1,492,461 B | 1,492,461 B | 50 |
| Bind/check/expand/HIR | 618,496 B | 2,236,277 B | 53,120 |
| Execution | 73,728 B | 113,760 B | 3,022 |

Raw results: [functions-memory.json](functions-memory.json). These count requested
System allocation bytes, exclude caller input/static data/allocator overhead/RSS,
and use independent phase baselines. Owned parser source and retained HIR origins
are included; JSON serialization occurs after snapshots. Phase peaks cannot be
summed. No throughput or total-process-memory improvement is claimed.

Measured namespace, inference and built-in-call workloads retain identical phase
bytes, peaks and allocation counts to their previous baselines. Remeasurements:
[namespace](stage8-namespace-memory.json), [inference](stage8-inference-memory.json),
[calls](stage8-calls-memory.json). Origin side tables add no heap allocation to
those profiles; their empty vectors add fields to the stack-level Program value.

Full DateCalc, arbitrary C# translation, native editor integration and cross-file
function exports remain outside admission. This milestone is stacked on the
built-in-call branch; neither draft PR is merged by this work.
