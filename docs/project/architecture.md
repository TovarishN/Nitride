# Engine architecture

Nitride prepares admitted `.ngr` language profiles and runs them in a native Rust
engine. Syntax-only profiles stop at syntax; typed profiles provide declarative
binding, nominal type checking, typed HIR and explicit host execution. Arbitrary
C# semantic code is outside that contract. See [current status](status.md) for
implemented scope and measured limitations.

## Crates and entry points

| Crate | Responsibility | Source |
|---|---|---|
| `nitride` | Embedding facade: strict parse/check/run API, phase-aware errors and optional Cargo build helper | [Library](../../crates/nitride/src/lib.rs) |
| `nitride-core` | Language tables, parsing, recovery, shared syntax, binding, checks, typed HIR and execution; no Cargo dependencies | [Library](../../crates/nitride-core/src/lib.rs) |
| `nitride-stdlib` | Versioned bundled grammar libraries, including lexical definitions | [Library](../../crates/nitride-stdlib/src/lib.rs) |
| `nitride-prepare` | Native project, `.ngr` and catalog frontend; validates and prepares language data | [Library](../../crates/nitride-prepare/src/lib.rs) |
| `nitride-codegen` | Emits static Rust language/parser tables; loads prepared languages at runtime and owns runtime snapshots | [Library](../../crates/nitride-codegen/src/lib.rs) |
| `nitride-process` | Host operations across a native child-process protocol | [Library](../../crates/nitride-process/src/lib.rs) |
| `nitride-wasm` | Host operations in WebAssembly components through Wasmtime | [Library](../../crates/nitride-wasm/src/lib.rs) |
| `nitride-testing` | Native language-suite parsing, assertions and evaluation | [Library](../../crates/nitride-testing/src/lib.rs) |
| `nitride-lsp` | Workspace loading, document analysis and editor protocol | [Library](../../crates/nitride-lsp/src/lib.rs) |
| `nitride-cli` | Commands, bundled sample languages, native server startup and integration examples | [Library](../../crates/nitride-cli/src/lib.rs) |

The [VS Code extension](../../editors/vscode/) launches native tools and exposes
language authoring and testing. It is separate from the Rust workspace.

## Preparation and language ownership

`nitride_prepare::prepare_project` reads an ordered project manifest and its
inputs into validated prepared data. The frontend is bootstrapped from a
checked-in seed; ordinary native preparation does not invoke .NET.

Bundled languages follow the [CLI build script](../../crates/nitride-cli/build.rs):
prepare the project, pass its data to `nitride_codegen::generate`, write generated
Rust into Cargo's `OUT_DIR`, and include those tables in the consuming crate.
The build script registers the project and source inputs with Cargo for rebuilds.

Runtime users instead load prepared data through
`nitride_codegen::LoadedLanguage`. Its owner/session machinery keeps the language
tables alive with the syntax that borrows them; `RuntimeSnapshot` supports owned
analysis and editing. Static and runtime paths use the same core contracts.
The [embedding facade](../guides/embedding.md) supplies a Cargo build helper and a
[standalone calculator](../../examples/embedded-calculator/) that owns its language.
Its default runtime dependency is core only. The older CLI embedding example
remains available, but is no longer the recommended application recipe.

## Parsing and edits

The parser consumes UTF-8 source under explicit work, depth and stack limits.
Strict parsing reports failures; recovery can retain a usable tree with syntax
problems. Generated callbacks specialize recognition while the shared backend
owns syntax construction, memoization and dependency facts.

A shared snapshot retains immutable syntax and source storage. Editing creates a
new snapshot, reusing unaffected records and recognition facts where valid. Older
readers remain usable. Materializing a flat tree and projecting editor data are
separate operations with their own costs. The flat recognizer also supplies an
independent reference for shared-backend regression checks.

The [shared backend](../../crates/nitride-core/src/parser/shared.rs) keeps state and
language-table ownership in its parent module. Its implementation is grouped by
responsibility:

- `snapshot.rs`: parse entry points, edit lifecycle, publication and stack spill.
- `recognize.rs`: rule recognition, memo reuse, budgets and syntax construction.
- `prediction.rs` and `pratt.rs`: predictive choices and precedence parsing.
- `probe.rs`: recovery probes and selected-node ownership.
- `tokens.rs`: bounded token and trivia lookahead caches.
- `children.rs`, `facts.rs`, `memo.rs`, `dispatch.rs`, `hash.rs` and `generated.rs`: child storage, recognition facts, cache keys, dispatch, hashing and generated-code bridge.
- `tests.rs`: shared-backend regression cases, including budgets, cancellation and ownership.

## Semantics and host execution

For a typed profile, `compile` binds names, validates supported semantic rules and
produces typed HIR. Nominal types and catalog operations define which programs
are admitted. Host bindings supply operation implementations with explicit
signatures; whole-program preflight prevents host effects when the program or
its required bindings fail validation. `execute` runs admitted programs under
limits. [Host binding documentation](../guides/host-binding.md) describes the native,
process and component contracts.

Incremental syntax does not imply incremental semantics. Binding, structural
checks, navigation indexes and hints currently recompute across the document.
Syntax-only CSharp4 and XML do not gain a type system from their parser coverage.

## Editor analysis

The LSP prepares workspace languages, retains document versions and snapshots,
and publishes analysis only when its document and workspace generations remain
current. Background scheduling uses independent document quiet periods and
request freshness barriers. Cooperative parser/recovery cancellation lets
obsolete work stop at checkpoints; semantic/host calls and bulk maintenance
remain interruption limits. See [scheduling](../editor/lsp-scheduling.md) and
[current status](status.md) for the exact behavior and acceptance evidence.

Use the [documentation index](../README.md) to find authoring guides, current
measurements and historical records.
