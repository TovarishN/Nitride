# Inferred declarations and exact overload selection

User Go approves the next two stated capabilities, in order. Execute inline with
the Nitrogen, planning, TDD and verification workflows. Existing design.md's native
deployment, ownership, nominal types, full validation/preflight and once-only
initializer requirements apply. No commits/pushes or Nitrogen product edits.

## Contracts

- A sequential declaration without `type` gains inference only when its catalog
  statement explicitly sets `infer:true`. Its declared value field supplies the
  inferred nominal type. Explicit annotation and inference policies cannot mix
  on the same rule. Untyped declarations without policy remain unsupported.
- Bind the complete document to declaration identities first. Lower sequentially
  afterward, taking reference types from the already-validated declaration HIR.
  Shadowing initializers use prior bindings. No inference executes host handlers.
- Catalog `overloads` maps a grammar lowering target to ordered concrete operation
  IDs. Match exact nominal input tuples; never convert or use expected result
  types to disambiguate. Reject empty/unknown/duplicate input-signature groups
  during preparation. Keep chosen concrete signatures in HIR and exact preflight.
- Existing fixed-operation and annotated declaration behavior remains compatible.
  Untouched profiles and reference observations must keep passing.

## Reference boundary

Nitrogen's declarative TypeOfSymbol does NOT infer initializer types. DateCalc
uses C# symbol properties for this. Keep native input Inference.ngr declarative;
generate a separate development-only Inference.Reference.ngr with explicit C#
type properties/overload selection, and compare against actual Nitrogen
binding/semantics/HIR. The adapter declares the extra semantic contract; do not
claim existing declarative inference or translate arbitrary C# in Rust. Reference
HIR uses typed symbol references (no initializer expansion), so stored results
are reused. Record generated adapter provenance and syntax parity through corpus.

## Tasks

1. Build the actual reference fixture with inferred/annotated value+rate
   declarations, nominal Time, reference-kind priority and exact Scalar/Time
   operation overloads. Freeze successful/error syntax, bindings, HIR, spans,
   values/traces before implementing the equivalent native pipeline.
2. Add failing prep/codegen/runtime tests for explicit inference policy, shadowing,
   type chains, invalid initializers/annotations, exact overload matching and
   rejection before effects. Extend native/.NET export of untyped sequential
   clauses while codegen enforces the explicit catalog policy.
3. Implement optional annotation actions, declaration-identity binding and
   iterative inference, then overload group validation and selection. Preserve
   fixed-operation diagnostics, limits, nominal identity and full preflight.
4. Register Cargo/CLI/editor fixture, add frozen and generated live comparisons,
   verify actual workspace loading with positive and negative controls. Native
   authoring/build/runtime remain independent of reference C# files and .NET.
5. Run full Rust/exporter/preparation/runtime gates, native-only source build,
   footprint measurements and one fresh review; repair material findings and
   deliver evidence. No general C# frontend, function calls, scopes, DateCalc
   compatibility, return-directed overloads or implicit conversions are admitted.

Pre-flight: parser/exporter preserve null declaration targets; catalog policy is
consumed only by codegen. Binder records declaration IDs; lowering uses already
available initializer HIR types. Overload selection consumes those nominal types
and emits existing concrete-operation HIR, leaving execution unchanged.

## Completion

All five tasks are complete. See stage6-evidence.md for the final validation,
review repair, explicit reference policy and deployment/footprint limits.
