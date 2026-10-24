# Built-in callable symbol admission

Nitride now prepares unscoped `.ngr` builtin declarations and compiles
catalog-backed calls through binding, exact nominal signature selection and
ordinary concrete-operation HIR. The sample returns `[6,-5,0,-7]`.

## Binding and typing contract

`catalog.builtins[kind][name]` lists concrete operation IDs.
`catalog.calls[rule].args` lists the call's argument fields; that rule must have
one References clause. Optional normalized-v1 `builtins` records carry declared
(kind,name) identities. Missing inventory means no built-ins. Invalid/duplicate
inventories, missing/undeclared mappings, duplicate input tuples, empty signatures,
unknown operations/kinds/rules and malformed call fields fail preparation, even
when unused. Scoped built-ins remain explicitly rejected.

Resolution follows actual Nitrogen Project.Lookup: search visible sequential
locals across the authored kinds, then built-ins across those kinds. A local in
a later kind can hide a builtin in an earlier kind. A selected local noncallable
fails typing without a retry. Initializers see previous locals or a built-in
before introducing their own declaration. Dotted aliases are explicit name strings.
Binding errors are primary-selected by source position before reporting type
errors; this matters for nested calls with an unresolved outer name.

Selection uses exact input tuple and arity, including zero arguments; result
context and implicit conversions never select a candidate. The fixture exercises
0/1/2 arguments and multiple arities under one name. It does not introduce varargs
or an engine arity cap. Arguments evaluate once left-to-right; initializer results
are reused. The chosen concrete operation uses unchanged whole-program exact-host
preflight, so missing later bindings prevent earlier effects.

Existing Binding records retain their layout. Built-in bindings have separate
storage and expose explicit names rather than fabricated source identities.

## Independent reference and validation

Pure `Calls.ngr` is the native input. `scripts/calls_adapter.py` generates a separate
`Calls.Reference.ngr` and C# helper with explicit symbol type properties and
operation selection. The helper checks that the actual bound symbol is builtin;
selection by text alone would incorrectly call hidden built-ins. Provenance hashes
cover pure grammar, catalog and generated adapter/helper. Native builds do not
translate or consume the C# adapter.

- **75 Rust tests** pass, including **36 frozen call observations** from actual
  Nitrogen parsing, binding, semantic diagnostics and typed HIR, plus an explicit
  numeric fixture host. These include aliases, nested calls, nominal overloads,
  arities, local-first resolution, initializer shadowing, undefined names and
  UTF-8 spans. Existing inference-adapter diagnostic policy remains unchanged.
- **64 generated call programs** match live Nitrogen and native release. All old
  runtime gates still pass: total **144 frozen observations** and **320 generated
  programs** across profiles.
- **14 development exporter tests** pass. **71 valid preparation profiles** match
  live .NET export; both preparers reject the shared **14 invalid grammars**.
- Call-specific effect tests prove argument ordering/reuse and zero effects on
  missing selected host bindings. Handcrafted invalid public call tables return
  structured diagnostics. Formatting, strict clippy, locked release and reference
  builds pass.
- One fresh independent review found no actionable correctness issues and ran
  **15 additional differential probes**, all matching. This is separate from the
  permanent generated corpus count.

The actual Nitrogen workspace loader uses root nitrogen.json, generated reference
grammar/helper and sample as explicit inputs. The positive sample passes; a
`function neg=2; neg(3);` control reports CALL0001. The temporary document was
removed. Live VS Code/Rider interaction remains unverified.

A clean source-only copy omitting tools/, all language.json exports, language C#
helpers and all .Reference.ngr files builds offline. With .NET environment entries
removed and PATH restricted to system tools, native prepare lists the six built-in
names and sample execution returns `[6,-5,0,-7]`. The host still has .NET elsewhere.

The three-platform workflow now includes the call sample and native preparation.
See the feature branch's GitHub checks for its hosted validation status; earlier
cross-platform evidence is in [ci-evidence.md](ci-evidence.md).

## Footprint

macOS arm64 release CLI: **734,672 B (717.5 KiB)**, +49,856 B from stage six.
Profiler: **469,728 B**, +272 B. Core third-party dependencies remain zero.
The CLI includes native preparation, all fixtures and observation JSON support.

The new call workload uses 1,000 pairs: 2,000 statements, 1,000 declarations,
1,000 value references and 5,000 built-in references. Its 48,669 source bytes
produce 21,001 syntax nodes and 8,000 HIR nodes. Three runs agree on byte/count
metrics:

| Phase | Retained | Incremental peak | Allocations |
|---|---:|---:|---:|
| Parse | 1,490,461 B | 1,492,320 B | 54 |
| Bind/check/HIR | 745,472 B | 1,269,856 B | 6,110 |
| Execution | 139,264 B | 255,536 B | 5,043 |

Raw data: [calls-memory.json](calls-memory.json). These are requested System bytes
with independent phase baselines, excluding caller input, static data, allocator
overhead and RSS; owned source is counted. JSON is formatted after snapshots.
Peaks cannot be added. No process-memory or throughput improvement is inferred.

Existing namespace and inference workloads have identical retained bytes, phase
peaks and allocation counts. Current remeasurements:
[namespace](stage7-namespace-memory.json), [inference](stage7-inference-memory.json).
Earlier baselines remain in namespace-memory.json and inference-memory.json.

This admits built-in callable symbols only. Built-in values, scoped built-ins,
user-defined bodies, lexical scopes, general C# helpers, recovery, incremental
editing and full DateCalc remain outside the profile.
