# Nitride composition and memory implementation plan

Approved direction: user said "lets go!" to module composition, an existing
language, isolated engine-memory measurements, and desktop platform validation.
Execute inline under the existing Rust/ordinary-deployment design. No commits,
publication, remote CI dispatch or changes to Nitrogen product code.

## Scope decisions

- Extend the development exporter to ordered multi-file manifests. Resolve local,
  imported, qualified and implicit extension-target rule references with the
  same precedence as Nitrogen. Preserve module and alternative identities.
- Keep existing typed Portable admission intact. Add a separate composed
  Portable.Power extension demonstrating right-associative exponentiation,
  exact signatures, typing and execution across modules.
- Copy the existing Base/Uses and XML grammars unchanged, with source hashes.
  Admit Base/Uses and XML parsing through an explicitly labelled syntax-only
  profile. XML's C# closing-tag checks and structural-pattern warning are
  reported as omitted; they are never silently treated as executed checks.
- XML requires negated character classes, any character and positive/negative
  lookahead. Reuse the generic parser; do not handwrite an XML parser.
- Compare normalized rule nodes, byte spans and parent relationships against
  actual generated Nitrogen parsers. Omit pure aliases on both sides.
- Add a separate profiling executable that instruments System allocation only
  while measuring parse, compile/HIR, and execution separately. Snapshot before
  formatting JSON. Keep instrumentation out of the ordinary CLI.
- Add portable Cargo CI configuration for macOS/Linux/Windows. Only locally
  executed platforms count as verified; no remote workflows will be pushed or
  triggered without authorization.

## Task 1: Multi-module preparation

Files: tools/Nitride.Reference/{Exporter,Program}.cs, languages/{power,composition},
scripts/test_reference.py, crates/nitride-codegen.

- [x] Add failing tests for imported references, extension-target implicit
  references, explicit start rule, unknown/ambiguous imports, deterministic
  manifests, and typed C# semantics rejection.
- [x] Implement `export-project <manifest> <output>`, retaining the single-file
  export command. Manifest contains mode, ordered grammar paths, catalog path
  and explicit qualified start rule.
- [x] Preserve source/module identity, import edges and supported extension
  composition; reject unresolved references and unsupported conflicts.
- [x] Generate separate static typed power and syntax-only Base/Uses languages.

## Task 2: Existing XML and reference parity

Files: core grammar/parser, reference SyntaxOracle, CLI build.rs, XML fixtures.

- [x] Add failing tests for negated classes, any character, lookahead rollback,
  syntax-only compile rejection, XML accepted/rejected cases and composed trees.
- [x] Implement generic parser forms and explicit profile metadata. Export XML
  syntax from the original grammar, preserving a list of omitted semantics.
- [x] Compile the original grammars with Nitrogen's generator for the actual
  reference. Compare normalized named-node trees/spans/parents, not merely
  acceptance, across XML documents and Base/Uses programs.
- [x] Test Portable.Power typed behavior and precedence against real Nitrogen
  binding/types/HIR/execution. Keep all stage-one tests passing.

## Task 3: Separate engine memory

Files: profiling binary and memory scripts/reports.

- [x] Test the profiler CLI stage records, retained/peak accounting invariants,
  and output for portable, composed, and XML modes.
- [x] Wrap the system allocator in the profiling executable only. Report
  allocation count, retained requested bytes, peak requested bytes and time
  per phase; source input and metadata baselines are explicit.
- [x] Measure increasing XML/config and arithmetic workloads without building
  observation JSON in the measured region. Record retained vector capacities
  separately from allocation peaks and process RSS.

## Task 4: Validation and review

- [x] Run focused TDD cycles, reference validation, workspace tests, formatter,
  clippy, release execution, and memory measurements.
- [x] Add ordinary-platform Cargo CI and record actual local platform evidence.
- [x] Run one fresh independent review, reproduce/fix material findings, then
  deliver evidence and clearly stated compatibility limits.

## Review focus

Lookahead must restore position, tree storage and speculative failure state;
extension actions retain the defining module while resolving target references;
syntax-only admission cannot be mistaken for semantic validation; supplementary
Unicode maps to byte spans; profiling excludes JSON and uses the unchanged
system allocator rather than changing the product's allocation behavior.
