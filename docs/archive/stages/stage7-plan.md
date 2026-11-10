# Built-in callable symbols

User Continue authorizes the proposed built-in call stage. Implement inline with
Nitrogen, planning, test-first and final-review workflows. Ordinary Rust targets,
no .NET native build/runtime requirement, zero third-party core dependencies.
Work on codex/builtin-calls in the existing independent checkout.

## Contract

Admit unscoped .ngr builtin declarations and catalog-backed call rules. Inventory
entries identify (kind,name); the catalog maps each to concrete signatures.
Resolve local sequential declarations across all authored kinds first, then try
builtins in authored kind order, following actual Nitrogen Project.Lookup. A
selected local noncallable fails typing; never retry a hidden builtin. Existing
value binding and initializer reuse stay intact. Builtin observations report
names explicitly, never fabricated declaration source positions.

Calls select exact input tuples (including zero arguments) and emit ordinary
concrete-operation HIR. All arguments evaluate once left-to-right. Complete
binding, type checks and exact host preflight precede effects. No callable values,
scoped builtins, user function bodies, C# translation, conversions, return-directed
selection, or full DateCalc admission. Handcrafted invalid action tables diagnose.

Catalog: builtins[kind][name] = [operation IDs]; calls[rule].args = [field names].
A call rule has exactly one References clause. Validate every declaration, mapping,
operation/signature tuple, call label and rule, including unused metadata. Reject
scoped builtins explicitly. Optional v1 builtin inventory absent means none.

Keep ordinary Binding storage unchanged; add a separate builtin-binding vector
and per-call selected-target map, avoiding larger per-reference records in older
profiles. Pure Calls.ngr builds natively; a generated C# reference/editor adapter
supplies actual Nitrogen symbol types and optional operation selection.

## Tasks

- [x] Freeze actual Nitrogen parsing/binding/type/HIR/execution cases for calls,
  aliases/dotted names, arities, namespaces/shadowing, nominal errors and no effects.
- [x] Watch frontend/codegen/call pipeline tests fail; implement inventories,
  validated static call tables, symbol resolution and exact operation lowering.
- [x] Register native build/CLI and Nitrogen editor config; verify workspace
  positive and negative controls. Add generated live comparisons and effect tests.
- [x] Run full Rust/reference/preparation gates, clean native-only build, footprint
  checks and one fresh review. Record fixes/evidence; commit and push for CI.

Ruling: New semantic mapping lives in the companion catalog, matching earlier
inference/overload policy. The reference adapter explicitly supplies that contract;
no raw Nitrogen declarative call-inference claim is made. Existing optional v1
metadata and prior profiles remain compatible. Fixed-arity fixture syntax does
not impose an engine arity limit.

## Completion and review

All four implementation tasks are complete. Native gates, workspace controls,
source-only build, memory evidence and final independent review pass; the branch
is prepared for publication and hosted CI. No actionable review findings;
15 extra reviewer differential probes match. See stage7-evidence.md.

Ruling: Binding errors are collected and primary-selected by source position,
preserving Nitrogen's complete-binding-before-typing stage order while handling
nested calls whose outer name precedes argument references. Prior observations
remain unchanged. Cost if wrong: different primary diagnostic on multiple errors.

Ruling: Separate builtin-binding storage preserves old per-reference layout and
allocation workloads; the cost is an additional empty vector in Program and a
separate per-call target map. The binding observation adapter merges by source
position. Measured namespace/inference phase bytes and allocation counts agree
with the prior baseline.
