# SDD ledger — plan: docs/plan.md

Design approved by user: "lets go", after normal Rust/desktop deployment correction.
Execution: inline, no commits or publication.

Ruling: New independent repository uses codex/native-engine without an extra
worktree; no existing Nitrogen source checkout is used for implementation.
Ruling: Annotated declarations (`let x : Scalar = 1;`) permit the current
declarative .NET typing APIs to serve as an actual oracle without translating
C# semantic blocks. The fixture duration type is named Portable.Time to avoid
clashing with the Portable.Duration operation identity.
Pre-flight: Task 1 format is consumed by Task 2 generation; task 2 trees/catalog
are consumed by task 3; shared normalized observations feed task 4 comparison.
No conflicting interfaces identified; exact Rust storage types are implementation choices.

Task 1: complete — real frontend exporter, shared catalog, generated reference
syntax, actual Nitrogen binding/type/HIR oracle, frozen observations. Five
exporter tests passed after observed red runs. The IDE metadata is generated
from catalog.json; the existing Nitrogen workspace check on sample.portable
completed successfully. Live IDE interactions were not tested.

Task 2: complete for the declared profile — generated static Rust tables,
table-driven parsing, compact indexed trees, rollback and work/depth guards.
Parser tests were observed failing before implementation and passing afterward.

Task 3: complete for the declared profile — sequential declarations,
references, nominal types, indexed HIR, whole-program signature preflight and
native execution. Nine pipeline tests passed after implementation and repairs.
23 frozen reference cases match, including HIR/source origins and invocation
traces. 64 generated valid/malformed cases match the live reference.

Task 4: initial native deployment evidence recorded in docs/footprint.json.
Release execution and dependency inspection verified on macOS arm64; executable
links only libSystem and runs with a system-only PATH and no DOTNET variables.
This is not a clean-VM test with .NET physically absent. Linux/Windows execution,
allocator high-water instrumentation and stack profiling remain pending.

Ruling: .NET HirConstant stores float, while Rust uses f64 per the design.
Exactly representable shared values are admitted; broader numerical parity
requires a separately specified tolerance and corpus. Do not claim bitwise parity.
Ruling: Generic .NET HirEvaluator accepts Scalar/Angle only. The oracle keeps
nominal Time in actual Nitrogen HIR and uses an explicit fixture execution host,
rather than weakening the type or claiming generic evaluator support.
Ruling: The initial supported grammar profile has one symbol namespace. Reject
multiple/incompatible kinds instead of silently erasing namespace semantics.

## Independent review and repairs

A fresh read-only reviewer inspected implementation, design, tests, and reference
adapter. All reported correctness findings were reproduced and repaired:

- Keyword literal boundaries: `letx:Scalar=1; x;` is rejected, matching Nitrogen.
- Symbol kinds: exporter/generator reject unsupported multiple/incompatible kinds.
- Public invalid Pratt rule IDs return grammar diagnostics instead of panicking.
- Fixture handler names and arity are validated before generation.
- Unterminated block comments follow Nitrogen's documented trivia-to-EOF policy.

Additional regression repairs include nested zero-width repetitions, language
snapshot identity and malformed syntax edges. Current native suite: 23 tests,
including one parity test containing 23 observations. Full lint gate passed.
No commits, pushes, or publication performed.


## Stage two: composition and isolated memory — complete

See stage2-plan.md and stage2-evidence.md. Multi-module export, typed Power
extensions, unchanged Base/Uses and XML syntax fixtures, generic lookahead and
negated classes, and isolated System allocation profiling are implemented.
40 Rust tests and 11 exporter tests pass; 51 frozen cases match live Nitrogen,
as do 128 generated cases across the two differential scripts. Formatter, clippy
and release builds pass. A fresh source-only copy passes all Rust tests offline.

Independent review found a broad bin ignore pattern hiding profiler source and
precedence incorrectly inherited by enclosed owner references. Both were repaired.
Three new regressions first failed, then passed: enclosed prefix operands, enclosed
infix operands and equal-precedence prefix/infix binding. Only a direct final
owner reference receives operand binding power, matching Nitrogen generation.

IDE workspace checks pass for the new fixtures; live IDE use remains unverified.
macOS arm64 is verified locally. Linux/Windows CI is configured and unexecuted.
All work remains local and uncommitted; no existing Nitrogen product is replaced.


## Stage three: parser scratch reuse — complete

Plan: stage3-plan.md. Repeated rule/infix child buffers and Pratt winner
snapshots use per-parse free lists; active recursive frames retain exclusive
ownership. All scratch is released before creating the tree's owned source
copy. The core remains dependency-free safe Rust with the System allocator.

Two allocation regressions were observed failing at 29,036 and 8,039 allocations,
then passing below 1,000. New parallel tests exposed a profiler-test filename
collision (same PID/mode); unique sequence IDs repaired it. All 42 Rust tests,
11 exporter tests, formatting, strict clippy and locked release builds pass.
51 frozen observations and 128 generated cases match live Nitrogen using the
optimized release binary. Fresh independent review found no actionable defects.

Ruling: Keep tiny per-parse pool metadata — it reduces parse allocation count by
more than 99 percent on 1,000-item fixtures; portable peak adds 856 bytes, while
XML peak and all returned tree capacities are unchanged. No lower retained-memory
or uninstrumented speed claim. Raw baseline retained in stage2-phase-memory.json.
See stage3-evidence.md for measurements and remaining limits. No commit or push.


## Stage four: Rust language preparation — complete

Plan: stage4-plan.md. nitride-prepare parses admitted .ngr sources, resolves
composition, preserves normalized metadata/hashes, validates the profile and
loads relocatable manifests. CLI prepare supports stdout and validated output
replacement. build.rs consumes .ngr/catalog/project inputs directly and tracks
all dependencies; no exported language.json or .NET bootstrap is needed.

Ruling: Syntax-only semantic blocks are explicitly opaque balanced regions,
not C# or Nitrogen semantic-statement validation. Their omission remains visible
in metadata. Parsed supported declarative clauses still undergo structural
validation before omission. Raw/verbatim strings and unpaired UTF-16 surrogates
are outside admission. Shared codegen errors currently report the first source
at byte zero; native parse/reference errors carry exact authored spans.

55 Rust tests, 11 exporter tests, formatter, strict clippy and locked release
builds pass. 68 positive native preparations match live .NET export; both reject
14 invalid grammars. All 51 frozen and 128 generated runtime comparisons pass.
The final source-only copy builds offline without tools/ or any language.json
inputs, with dotnet unavailable on PATH. Native preparation and execution run
in that same environment. This is not a machine with .NET physically uninstalled.

Review findings were reproduced with failing tests and fixed: duplicate sequence
labels, omitted clause fields and temporary cleanup ownership. Related structural
checks cover unreachable choices, keyword boundaries, nullable prefixes/tokens
and duplicate extension names. All scratch/frontend changes are safe Rust.

Ruling: Integrated prepare adds 198,816 bytes to the combined CLI (651,328 bytes
now, previously 452,512). The core still has zero third-party dependencies;
profiling executable remains 452,496 bytes. This is tooling footprint, with no
claim that preparation consumes constant memory or runtime RSS remains identical.
Linux/Windows workflows include preparation but remain unrun. No commits/pushes.


## Stage five: multiple symbol namespaces — complete

Plan: stage5-plan.md. New preparation outputs contain an ordered, unique symbol
kind inventory. New codegen actions preserve declaration kind and reference-kind
order; the binder keys its environment by (kind, spelling). Binding evidence keeps
the selected kind. HIR/execution continue to use unique declaration identities.
Legacy metadata without inventory keeps the earlier single-kind gate.

Ruling: Follow actual Nitrogen Project.Lookup: authored kind priority comes before
cross-kind recency in this sequential document profile. The first matching kind
wins, even if its type subsequently fails; do not silently retry by desired type.
Current namespace identity is Nitrogen's kind string, not module-qualified text.

24 actual Nitrogen cases were frozen before native implementation. All 59 Rust
tests pass, including namespaced binding/type/HIR/source/effect parity and once-only
initializers. 64 generated namespace programs also match live Nitrogen. Existing
51 frozen cases and 128 generated programs still match. Exporter tests: 12;
preparation comparisons: 69 valid and 14 invalid. Formatting/clippy/release pass.
Independent read-only review reported no actionable findings.

The configured new language loads via explicit focused workspace inputs; the
valid sample passes and a temporary wrong-namespace sample reports NB0001. A
file-only check was found insufficient because it selected no workspace language;
its prior exit-zero result is not treated as editor-loading evidence. Live
VS Code/Rider use remains unverified. No Nitrogen product code was changed.

Native-only source build omitting tools/ and all exported language JSON succeeds
offline, with dotnet unavailable on PATH. Its new sample prepares with inventory
[value,rate] and executes values [10,20,10,20], preserving selected binding kinds.

Existing measured portable/Power/XML phase allocation counts, retained bytes and
peaks are unchanged. Combined CLI: 668,016 bytes (+16,688); profiler: 452,672 bytes
(+176). Namespace benchmark records 2,000 declarations + 2,000 references for
1,000 pairs; details in stage5-evidence.md and namespace-memory.json. No full
DateCalc, inferred declaration, overload, scope or built-in/function admission.
No commits or publication; Linux/Windows CI configured but unrun.


## Stage six: inferred declarations and exact overloads — complete

Plan: stage6-plan.md; validation and limits: stage6-evidence.md. Catalog-opted
inference obtains nominal types from prior initializer HIR. Binder stores compact
declaration identities rather than duplicate type strings. Exact nominal input
matching selects concrete operations before unchanged execution preflight.

Ruling: Nitrogen requires a generated C# reference adapter for this fixture's
inference and overload properties. It is not part of native preparation/build.
Review exposed mixed-error ordering in the adapter; four frozen regressions and
an explicit combined source-order policy repair it using actual type/HIR facts.
Do not attribute that adapter policy to raw Nitrogen diagnostics.

68 Rust tests, 12 exporter tests, 108 frozen/256 generated runtime comparisons,
and 70 valid/14 invalid preparation comparisons pass. Formatting, strict clippy,
release and source-only offline native builds pass. Actual workspace positive and
negative controls pass; live editor interaction remains unverified. CLI684,816 B;
core has zero third-party dependencies. Inference memory report is recorded,
without RSS or throughput claims. No commits/pushes; Linux/Windows remain unrun.


## Published repository and cross-platform CI — complete

The user authorized GitHub creation and push. The project is committed and pushed
to private TovarishN/Nitride, default branch main. Historical no-publication and
uncommitted notes above describe the earlier milestones.

Initial CI exposed exact-byte source/catalog hash changes from checkout line
endings. A clean clone reproduced the failure. Enforced LF attributes and a live
regenerated composition export repair it without weakening hash checks; provenance
preserves original upstream hashes. A fresh autocrlf=true clone passes 68 tests.

Commit 2b8dc1a passes the full hosted macOS/Linux/Windows matrix, including native
preparation, release build, strict clippy, tests and sample execution. Live
preparation and composition runtime parity remain valid. See ci-evidence.md.
Platform execution is now verified; cross-platform RSS and live editor use are not.


## Stage seven: built-in callable symbols — complete

Plan: stage7-plan.md. Added unscoped builtin inventory, strict catalog-backed call
rules and exact nominal/arity selection into ordinary concrete-operation HIR.
Resolution follows real Nitrogen: all locals before builtins, with kind priority
within each tier; a selected local noncallable does not retry a hidden builtin.

Ruling: Source-ordered binding diagnostics handle unresolved outer call names
before later argument references. Binding still completes before type checking.
The separate builtin-binding vector preserves existing per-reference storage;
ordinary namespace/inference phase allocation/byte metrics remain unchanged.

75 Rust tests, 14 exporter tests, 144 frozen/320 generated runtime comparisons,
and 71 valid/14 invalid preparation comparisons pass. A clean source-only offline
native build prepares and runs Calls with no .NET on PATH. Actual workspace valid
and noncallable-shadow controls pass. Final independent review found no actionable
issues; its extra 15 live probes match. CLI734,672 B; core dependencies remain zero.
The new fixture is included in all three CI jobs; see branch checks for current
hosted status. Full DateCalc, authored function bodies and live editors are deferred.


## Stage eight: typed user-defined templates — complete

Plan: stage8-plan.md; limits and evidence: stage8-evidence.md. Added pure declarative
function definitions, typed parameter scopes, nested/forward expansion and complete
origin chains, using fixed catalog operations in a document-local profile.

Ruling: Preserve Nitrogen template substitution effects: repeated parameters repeat
computation; unused arguments are checked/lowered but emit no executed HIR. Match
its selected-root API, including the outer-use diagnostic for unknown-result cycles;
do not claim its registered-root API admits identity-result expansions.

Ruling: Keep full origins in Program side tables. Existing node/binding storage
and namespace/inference/call phase allocation metrics remain unchanged; expanded
functions carry explicit origin/expansion storage and work costs.

Final independent review found one cyclic type-cache issue. Five new frozen cases
reproduced it RED; bounded fixed-point type propagation/constraint checks make them
GREEN. Full 87-test suite, 15 exporter tests, 189 frozen/384 generated observations,
and 76 valid/16 invalid preparations pass. Source-only offline native build and
actual workspace controls pass. CLI801,104 B; core dependencies remain zero.
The reviewer ran 23 probes; no other actionable findings or deferred minors.
Stacked draft PR/three-platform CI track hosted validation. No draft PR merge,
DateCalc admission, mixed global captures or live editor interaction is claimed.


## Stage 9: Self-hosted admitted grammar frontend

Replaced the handwritten .ngr lexer/parser with generated core parsing of
languages/grammar/Grammar.ngr, followed by a Rust model decoder. A checked-in Rust
seed breaks the Cargo cycle; regeneration and a rebuilt fixed-point check match
byte for byte. Actual Nitrogen exports the same normalized grammar. New strict
comment parsing is opt-in so existing application-language behavior is preserved.
99 Rust tests,15 exporter tests,77 valid/16 invalid preparation profiles,
189 frozen and 384 generated runtime comparisons pass locally. Clean offline
source-only build/fixedpoint/preparation/execution and configured workspace
positive/negative controls pass. CLI818400B(+17296), profiler519600B; phase memory
allocation/retained/peak metrics unchanged. Full Nitrogen grammar coverage and
live editor interaction remain deferred. Final review/publication tracked in the
stage9 plan and evidence.


## Stage 10: Built-in values, text literals and source checks

Plan: stage10-plan.md; evidence: stage10-evidence.md. Added the DateCalc-shaped
Dates profile. Host-supplied `builtin value` names are resolved after all
locals and read at most once per execution, after preflight. `lowers Op(text F)`
passes exact source spellings as Core.Text HIR to text host handlers. Catalog
`iso-date` checks report coded diagnostics before execution. Grammar.ngr gained
text arguments; the seed was regenerated, and the fixed point and live export
parity hold.

111 Rust tests, 15 exporter tests, 220 frozen and 448 generated live
observations, and 82 valid / 16 invalid preparation profiles pass. A clean
offline source-only build runs Dates without .NET on PATH. Nitrogen's workspace
checker passes the sample and reports DATE0001 on an invalid date.
CLI 835,216 B (+16,816). Older phase allocation counts, retained bytes and peaks
are unchanged. Self-review only; not yet published. DateCalc rewrites, calls,
weekday/Math functions and live editors remain deferred.


## Stage 11: Quote rewrites and syntax-function helpers

Plan: stage11-plan.md; evidence: stage11-evidence.md. Added `syntax function`
quote helpers with ordered match arms, and inline and helper `rewrites`
clauses, over a Rewrites profile composed with Dates. Templates are parsed by
the generated language in template mode, with `$` holes, and validated on
every compile. Holes take the caller's HIR; template names see only built-ins.
Arms match original syntax. Reachable-only re-emission reproduces Nitrogen's
repeated computation and keeps discarded syntax from running. Origins record
the use and template-relative spans.

120 Rust tests, 249 frozen and 512 generated live observations, and 88 valid /
20 invalid preparation profiles pass. A clean offline source-only build runs
Rewrites without .NET on PATH. CLI 918,416 B (+83,200). Older allocation
counts and retained bytes are unchanged; HIR peaks fell. Self-review only; not
yet published. List splices, grammar functions, weekday/Math calls and live
editors remain deferred.


## Stage 12: CSharp4 admission (Milestone A of C# recovery)

Design: superpowers/specs/2026-10-24-csharp-recovery-incremental-design.md; plan:
superpowers/plans/2026-10-24-csharp4-admission.md; evidence: stage12-evidence.md.
Vendored CSNitrogen's CSharp4 grammar with provenance. Admitted grammar
functions (a Rust port of Nitrogen's checker, evaluator and factoring),
transparent alternatives, reference and shared operators, prefix
associativity, and surrogate-pair classes. The parser now has leaf tokens, a
separate frame budget and a size-relative work budget.

140 Rust tests; 232 frozen C# cases and 64 generated programs match Nitrogen's
trees; 107 valid / 29 invalid preparation profiles match the live exporter.
Linear scaling to 16 MiB, about 6x slower than Nitrogen. CLI 1,501,968 B.
A clean offline build prepares and parses C# without .NET. Recovery
(Milestone B) and incremental parsing (Milestone C) follow.
