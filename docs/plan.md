# Nitride First Milestone Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Deliver a standalone Rust language engine and example that runs without .NET, with a shared Nitrogen reference corpus and measured native footprint.

**Architecture:** Bootstrap grammar preparation with the existing .NET frontend, export an explicit supported grammar/catalog format, and generate static Rust language tables on the build machine. A reusable Rust engine parses those tables and performs sequential binding, nominal type checking, HIR lowering, whole-program host preflight, and native execution. No .NET component ships with the executable.

**Tech Stack:** Stable Rust with `std`, Cargo, a .NET 10 development-only exporter/oracle, JSON build interchange and fixtures.

**Spec:** `docs/design.md` (approved in the Nitrogen checkout)

## Global Constraints

- Target ordinary macOS, Linux, and Windows applications; validate on the available native host first.
- No `no_std`, custom allocator, embedded board, or hard memory-budget requirement.
- Preserve the existing Nitrogen checkout; create Nitride as a separate repository at `/Users/dmitrikamenetski/work/Nitride`.
- No publication, pushing, or replacement of existing tools. Leave changes reviewable without committing unless requested.
- Bootstrap tooling may require .NET; deployed applications must not.
- Implement only an explicit grammar subset and reject unsupported constructs.
- User approval of the revised design authorizes executing this plan inline; do not introduce another approval round for routine implementation choices.

## Review Focus

- UTF-8 byte spans versus .NET UTF-16 spans: compare normalized byte offsets, including non-ASCII trivia and malformed text.
- Invalid later statements: complete validation/preflight must prevent earlier host effects.
- Sequential declaration visibility: initializer references must not accidentally see the declaration being introduced or later declarations.
- Backtracking/repetition: failed alternatives must roll back syntax storage, and zero-width repetitions must fail deterministically.
- Deep inputs: parsing, lowering, and evaluation must reject excessive depth/work rather than overflowing the stack.

## File Structure

All following paths are relative to the new Nitride repository.

- `Cargo.toml`, `.gitignore`, `README.md`: independent Rust workspace and usage.
- `crates/nitride-core/src/{lib,grammar,parser,syntax,semantic,execution}.rs`: reusable parsing, storage, typed HIR, and execution.
- `crates/nitride-core/tests/`: grammar-independent behavior and limit tests.
- `crates/nitride-cli/{Cargo.toml,build.rs,src/main.rs}`: build-time language generation and small native executable.
- `crates/nitride-codegen/src/lib.rs`: strict normalized-format validation and static Rust table emission.
- `tools/Nitride.Reference/`: .NET frontend exporter and reference runner; references the Nitrogen source checkout through a configurable project property.
- `languages/portable/{Portable.ngr,Portable.ncatalog,language.json}`: authoritative fixture grammar, semantic contract (see `docs/catalog-language.md`), and exported build input.
- `fixtures/`: shared valid and invalid source cases, normalized oracle outputs.
- `scripts/`: reproducible reference generation, parity checking, and footprint measurement.
- `docs/`: copied approved design, plan, admission evidence, and deferred coverage.

## Task 1: Bootstrap and freeze the language contract

**Interfaces:** `Nitride.Reference export <grammar> <catalog> <output>` produces version 1 normalized grammar input; `Nitride.Reference oracle <source>` produces a JSON observation.

- [x] Create the repository, install/locate Rust tooling, and record actual versions. Preserve unrelated files and tools.
- [x] Add shared source cases for precedence, parentheses, sequential declarations, nominal scalar/duration operations, undefined/forward names, duplicate declarations, incompatible operations, malformed input, nonfinite arithmetic, empty source, and Unicode trivia.
- [x] Author `Portable.ngr` using supported token/sequence/choice/repetition/extensible-expression and binding/lowering constructs. Register it in the new project's `nitrogen.json`.
- [x] Define catalog operation IDs, exact signatures, and explicit semantic node mappings. Unsupported semantic annotations fail export, never disappear.
- [x] Implement the exporter using `GrammarParser` and `GrammarCompiler` validation. Include format version, SHA-256 source identity, and qualified rule identities; output must be deterministic across directories.
- [x] Implement the .NET oracle using generated fixture syntax plus Nitrogen binding/type/HIR APIs and exact host bindings. Freeze normalized observations before implementing equivalent Rust behavior.
- [x] Verify export determinism, bad references, unsupported C# blocks, unsupported recursion, invalid catalog mappings, and the complete reference corpus. Run focused .NET builds/tests for this tool.

## Task 2: Generated native parsing

**Interfaces:** `generate(input: &str) -> Result<String, GenerateError>` emits Rust static grammar tables; `parse(language: &Language, source: &str, limits: Limits) -> Result<SyntaxTree, Diagnostic>` returns an owning tree with borrowed views.

- [x] Add failing tests `precedence_and_associativity`, `backtracking_rolls_back_nodes`, `zero_width_repeat_is_rejected`, `unicode_byte_spans`, and `depth_limit_returns_diagnostic`; run and confirm the intended failures.
- [x] Implement strict version/catalog/rule validation and code generation in `nitride-codegen`. Generated code uses static language tables, not per-parse grammar allocations.
- [x] Implement table-driven parsing with named-rule nodes, compact IDs/spans, explicit backtracking rollback, progress guards, and depth/work limits. Normalize purely structural wrappers for oracle comparison.
- [x] Compile the fixture from exported data in CLI `build.rs`; no .NET process is invoked by Cargo when generated language inputs are present.
- [x] Run parser and codegen tests, then compare normalized fixture syntax observations to the .NET oracle. Reject unsupported inputs explicitly rather than adding fixture-specific parser branches.

## Task 3: Typed native pipeline and host execution

**Interfaces:** `compile(language, tree, catalog, limits) -> Result<Program, Vec<Diagnostic>>`; `preflight(program, bindings) -> Result<(), Vec<Diagnostic>>`; `execute(program, bindings, limits) -> Result<ExecutionReport, Diagnostic>`.

- [x] Add failing tests for sequential visibility, nominal-type mismatch, source origins, signature mismatch, nonfinite values, and an invalid final statement causing zero handler invocations.
- [x] Bind sequential declarations with explicit symbol identities; lower expressions using catalog mappings into typed, indexed HIR. Reuse declared values without replaying initializer host effects.
- [x] Validate the entire program and exact host signatures before executing any operation. Check finite inputs/results and evaluator work limits. Keep effect traces observable in tests.
- [x] Supply explicit native arithmetic/duration handlers and the library embedding example. The CLI supports file input and machine-readable observations/results.
- [x] Compare accepted/rejected cases, binding/type observations, normalized HIR/origins, execution results, and invocation traces to the frozen reference corpus.

## Task 4: Deployment evidence and review

**Interfaces:** scripts emit reproducible parity and footprint reports including host/toolchain/configuration.

- [x] Run `cargo fmt --check`, focused tests, workspace tests, and `cargo clippy --workspace --all-targets -- -D warnings`; repair meaningful failures.
- [x] Build release CLI, execute from a clean environment without .NET on PATH, and inspect native linked dependencies. Distinguish this from a clean machine with .NET physically absent.
- [x] Compare Rust and .NET release executables on the same machine/source corpus, reporting binary/deployment sizes, cold startup, process memory, retained syntax/HIR storage, and allocations where instrumented. State metric definitions and limitations.
- [x] Record platform validation honestly: native macOS results, Linux/Windows pending unless actually run. Do not imply full DateCalc or Nitrogen compatibility.
- [x] Review all changes against the design, repair defects, and deliver exact commands/results plus remaining admission gaps. Keep the implementation uncommitted and unpublished unless separately requested.

## Plan Self-Review

The four tasks cover the approved design's stages. The five review-focus cases
are assigned to parser, semantic, and preflight tests above. Normalized rule and
semantic identities are shared through the exported format; physical storage
IDs are implementation details. Toolchain setup and cross-platform execution
availability are checked early and reported separately from product correctness.
