# Nitride: native Nitrogen engine

Status: approved by the user on 2026-10-24 after revising the target to ordinary
Rust desktop/server deployment. Initial implementation lives in the separate
`/Users/dmitrikamenetski/work/Nitride` repository; see its README and evidence.

## Intent and agreed direction

Nitride is the proposed working name for a Rust implementation of Nitrogen's
language engine. Name and package availability have not been checked. The agreed
requirements are Rust, ordinary desktop/server deployment targets, and a small
memory footprint without requiring the .NET runtime. Embedded Linux,
microcontrollers, bare-metal operation, and `no_std` are not requirements.
Removing .NET is a concrete deployment benefit; actual process memory savings
must still be measured rather than assumed.

Preserve Nitrogen's defining pipeline: grammar composition, parsing, binding,
typed checking, typed HIR, and execution through explicit host operations.
Compatibility is admitted feature by feature using evidence, not inferred from
the implementation language or a successful arithmetic example.

## Proposed defaults requiring design approval

- Start as an independent Rust project named Nitride. Keep the existing Nitrogen
  implementation as a reference and development tool during migration.
- Prepare the design here; create the implementation in a separate repository
  after approval and filesystem access is available. Do not restructure Nitrogen
  or modify sibling repositories as part of this design step.
- Target ordinary macOS, Linux, and Windows applications. Start development and
  measurement on the available macOS host; add native platform validation as
  runners become available. No specific CPU architecture or board is required
  for the first milestone.
- Use ordinary stable Rust with `std` and standard allocation. Choose compact
  data structures and avoid unnecessary dependencies; do not require custom
  allocators, fixed-capacity storage, or a `no_std` build.
- Compile a selected language ahead of deployment for the first milestone.
  Dynamic language loading and runtime grammar compilation are deferred for
  scope, not prohibited by the architecture.

These are recommendations, not previously agreed user requirements.

## Current implementation evidence

- `Nitrogen.Runtime/SyntaxTree.cs` already uses compact arrays and explicit
  disposal. Retain compact storage as a baseline; do not assume Rust alone
  reduces per-node memory.
- `Nitrogen.Generator` is a Roslyn source generator producing C# parsers and
  views. `Nitrogen.Workspace/GrammarWorkspace.cs` compiles workspace languages
  with Roslyn; `WorkspaceLoadContext.cs` loads assemblies dynamically.
- `examples/DateCalc/DateCalc.ngr` embeds C# semantic properties and selection
  helpers. Full DateCalc is therefore not a portable declarative input today.
- `Nitrogen.Runtime/Semantic/HostOperationRegistry.cs` validates catalog identity
  and signatures. Preserve that contract rather than binding by a name alone.

## Architecture

### Build machine

A host compiler reads `.ngr` and produces Rust parser code and static language
metadata. It resolves composition and validates supported constructs before
emitting anything. Generated output records its format version, source hash,
and language identity. Reproducible output must not depend on absolute paths or
timestamps.

Initially, reuse Nitrogen's existing grammar frontend through a small .NET
exporter that produces a versioned, normalized intermediate format. A Rust host
tool validates that format and generates Rust. This bootstrap dependency is
allowed on the build machine only. Reimplementing the complete `.ngr` frontend
is a later migration stage, not a prerequisite for proving native deployment.

The supported grammar subset is explicit. Unknown constructs, C# semantic
blocks, and unsupported recursion fail generation with source diagnostics;
they must never be silently dropped or replaced with handwritten parser logic.
The intermediate format is build tooling, not a runtime loader or stable
public interchange format in milestone one.

### Rust engine

Separate Rust crates or modules provide syntax storage/parsing and typed
semantics/execution. Exact crate boundaries are an implementation choice.

- Source, trees, catalogs, and HIR have explicit owners. Views borrow their
  owners; compact node IDs are validated at public boundaries.
- Source is UTF-8 with byte offsets internally. Oracle comparisons convert
  .NET UTF-16 spans explicitly, including non-ASCII test cases.
- Storage is compact and arena-oriented. No global caches or per-node strings
  are required for ordinary syntax. Names and operation identities are interned
  or stored once per language/catalog.
- Guard recursion and parser/evaluator work where necessary for termination and
  usable failures. Ordinary Rust collections and allocation are acceptable;
  comprehensive memory quotas and recoverable system OOM are not first-milestone
  requirements.
- Keep parsing and semantics usable as a library, with file access and command
  handling in application adapters. Standard library facilities are allowed.
  No .NET hosting or C# assembly loading is needed by deployed applications.
- Parsing never invokes application operations. Binding/checking/lowering and
  exact host binding succeed before execution. Rejected input has zero host
  handler invocations.

Embedded-specific allocators, `no_std` compatibility, hard memory budgets, and
microcontroller support can be evaluated later if there is a concrete need.
Do not introduce abstractions solely to anticipate them.

### Application host

A small executable reads a program, invokes the engine, supplies native Rust
operations, and prints diagnostics or results. File access and formatting live
here. A Rust library embedding example demonstrates the same engine without a
subprocess. A C ABI is deferred until there is a concrete embedding consumer.

Desktop workspace indexing, LSP, editor integration, incremental reuse,
quotation, rich recovery, and dynamic language discovery are later optional
facilities. The initial executable does not include them.

## First complete language

Use a new, explicitly limited portable fixture authored in `.ngr`, compiled by
both implementations. Do not claim it is the full DateCalc language or an
existing application's drop-in replacement.

The fixture includes numeric literals, parentheses, unary negation, binary
arithmetic with precedence and associativity, sequential `let` declarations,
references, and expression statements. Include two distinct nominal types,
`Core.Scalar` and a fixture duration type, with explicit construction and typed
operations. This exercises type rejection beyond accepting numbers everywhere.

Semantic behavior is expressed with existing declarative clauses where
supported and a versioned companion catalog with explicit operation signatures.
No C# expression is translated into Rust. Where the current declarative surface
cannot express required typing, use a narrowly specified typed rule in the
companion format; document the gap rather than altering Nitrogen semantics.
The .NET oracle adapter uses the same catalog/rules and existing runtime APIs.

Scope includes forward-reference rejection under sequential declarations,
unknown names, conflicting declarations, invalid operation argument types,
missing/incompatible host bindings, and malformed source. Exact duplicate-name
policy follows the .NET reference and is frozen in fixtures before porting.
Syntax errors are rejected; repair and execution of recovered trees are not
part of this profile.

Use finite `f64` scalar values initially. Specify parsing, overflow, and
nonfinite-result rejection in shared fixtures. Do not promise bitwise floating
point reproducibility across targets; simple exact cases compare exactly and
other cases use a documented tolerance.

## Evidence and admission

Before implementing the Rust pipeline, freeze shared inputs and normalized
reference outputs from the existing .NET pipeline. Compare:

1. Grammar generation and explicit rejection of unsupported inputs.
2. Parse acceptance, rule identities, child structure, and normalized spans.
3. Declaration/reference resolution and nominal type identities.
4. Diagnostic categories and locations, and normalized typed HIR.
5. Execution results and host invocation traces, including zero calls on
   invalid inputs or failed preflight.
6. Termination and structured errors at the implemented recursion/work limits.

Include malformed, deeply nested, non-ASCII, empty, and increasingly large
programs. Internal dense IDs and diagnostic wording need not match; externally
meaningful identities, source locations, and rejection behavior must match.

Measure release builds on the same machine, OS, architecture, corpus, and
configuration. Report toolchain, target, optimization settings, linked
dependencies, executable size, baseline/peak RSS, engine allocation high-water
mark, retained tree/HIR bytes, startup, and throughput. Use the platform's
appropriate process-memory metrics and state their definitions.
Include language metadata and initialization. Keep process-level RSS separate
from engine-owned storage and .NET warm execution separate from cold startup.

No numerical footprint claim is admitted before measurement. Initial results
guide optimization priorities for the next stage. The milestone report must
say whether the footprint actually improved and whether the evidence justifies
continuing the rewrite; successful compilation alone is insufficient.

The first milestone requires execution and measurement on the available native
host. Cross-platform support is a design target; claim validation separately
for macOS, Linux, and Windows only after running there. Cross-compilation alone
is build evidence, not execution evidence. Check the final executable runs
without .NET installed; ordinary platform libraries remain acceptable.

## Staging and completion

1. Create the separate project, Rust core boundaries, shared fixture, normalized
   grammar/catalog format, and .NET oracle/export adapter.
2. Generate native parsing and compact storage; establish parse parity and
   necessary recursion/work guards.
3. Add sequential binding, nominal typing, HIR, host preflight, and execution;
   establish full fixture parity.
4. Run native deployment checks and comparative footprint measurements; record
   results and decide the next admitted language/features.

Milestone one is complete only when all four stages have evidence. Bootstrapping
or an arithmetic parser alone is partial progress. A full Nitrogen rewrite,
DateCalc compatibility, desktop cutover, recovery parity, and microcontroller
deployment remain unimplemented until separately admitted.

## Decisions left to implementation

Choose the minimum supported Rust version, internal crate names, normalized
format encoding, parser-generation strategy for the admitted subset, data
layout, and suitable test tooling. Keep dependencies proportionate and record
choices with their measured implications. No package
publication, repository publication, or replacement of existing Nitrogen tools
is authorized by this design.
