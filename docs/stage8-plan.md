# User-defined typed template bodies

User Go approves the next stated function-body milestone. Implement inline using
Nitrogen, planning, TDD, verification and one fresh final review. Work on
codex/user-functions, based on 33810f0; keep the independent Nitrogen product
unchanged. Publish a stacked draft PR, without merging existing PRs.

## Contract and scope

Admit the existing declarative .ngr template mechanism: exported document-local
function definitions with scopes, typed parameters, lowers template Body(Params),
and references function Name lowers expand Name(Args). Bodies use fixed catalog
operations, typed literals, parameter references, parentheses and nested calls.
Forward function references and aliases/non-template diagnostics follow actual
Nitrogen. No closures, general lexical scopes, cross-file exports or runtime
function values. Mixing this profile with sequential globals or builtin call
policies is explicitly rejected for now; existing independent profiles remain.

Ruling: Preserve Nitrogen template substitution semantics: repeated parameters
repeat argument computation; unused arguments contribute no executed HIR, but all
arguments still undergo binding/type checks and lowering. This is an explicit
semantic boundary, not eager once-only function invocation. If wrong, effectful
arguments would differ from the reference; freeze traces before native work.

Definitions/parameter bindings are document-static. Validate every body, even if
unused. Infer result types from bodies; cyclic type queries terminate. Used
expansion cycles fail before execution; unused cycles match reference.
Bound recursion and total expansion work before effects. No partially valid HIR
is published and existing whole-program exact host preflight remains mandatory.

Use a separate template compiler backend selected only for admitted template
metadata, retaining compact existing syntax/HIR nodes and earlier profile paths.
Retain full ordered definition/use/argument origin chains in compact Program side
tables only for expanded HIR, with the first origin as the existing primary span.
Separated lists lower to existing sequence/repeat grammar tables; no runtime
parser feature or C# adapter is needed for the new function semantics.

## Tasks

- [x] Freeze actual Nitrogen syntax, bindings, diagnostics, full HIR origins,
  numeric execution and effects for typed template definitions/calls.
- [x] Watch frontend/codegen/runtime tests fail. Add strict template clauses,
  separated literal-delimiter lists, validated actions and bounded compilation.
- [x] Register native CLI/build/editor fixture; compare generated programs,
  verify real workspace positive/negative controls and source-only native build.
- [x] Run full regression/reference gates, footprint checks, fresh review;
  repair findings, commit and push stacked PR with cross-platform CI.

Review focus: duplicate/unused/bad parameters; argument arity/type spans; direct
and mutual cycles including cycles in unused arguments; recursion/depth/work
bounds; argument substitution origins/effects; invalid unused bodies; public
malformed tables; old-profile allocation and runtime compatibility.

Ruling: Match the existing selected-root reference API used by earlier profiles.
Full registered-root lowering rejects identity-result expansions; selected-root
lowering admits them. For cycles whose inferred result is absent, selected-root
lowering reports the outer unsupported use; other expansion cycles report the
closing call. Freeze these real API diagnostics rather than changing Nitrogen.
Cost: cycle diagnostics are API-specific, not full-Lower parity.

## Completion and final review

All four implementation tasks are complete locally; branch publication and hosted
CI are tracked in the stacked draft PR. See stage8-evidence.md. Independent review
found one important cyclic type-cache defect: transient unknown results could hide
invalid unused bodies and alter cycle diagnostics. Five actual-reference cases
reproduced it RED; fixed-point type propagation/constraint validation made all five
GREEN, with the full 87-test suite passing. Work accounting includes every pass.
The reviewer ran 23 probes; no other actionable findings or deferred minors.

Ruling: Full origin chains use compact Program side tables without changing
existing HirNode/Binding layout. Two empty vectors add stack-level Program fields
in old profiles, with no measured phase allocation/retained-byte changes. The cost
is extra retained origin data and compilation work for expanded templates.
