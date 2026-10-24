# Independent symbol namespaces

The Rust typed pipeline now supports multiple declared symbol kinds. The new
Namespaces fixture proves this with two independent sequential namespaces, using
the same spelling in both. This is a prerequisite for DateCalc's value/function
namespaces, not admission of DateCalc or callable function definitions.

```sh
cargo build --release --workspace --locked
./target/release/nitride prepare languages/namespaces/project.json
./target/release/nitride --namespaces languages/namespaces/sample.namespaces
```

The sample returns `[10,20,10,20]`; its binding observations select
`value,rate,value,rate`. `rate` in this fixture is a symbol kind, not a unit.

## Binding contract

- Declarations bind in one kind. The environment key is `(kind,name)`; shadowing
  and initializer visibility stay sequential and independent per kind.
- References retain their authored kind list. In this document profile,
  `(value | rate)` tries value first, then rate. This matches actual Nitrogen
  `Project.Lookup`; it is not ambiguity between kinds or most-recent declaration
  across kinds. A type mismatch in the selected kind does not retry another kind.
- Selected kind remains in binding records. HIR references and execution storage
  use the selected declaration's unique source identity, preserving initializer
  reuse without replaying effects. Nominal types remain independent of kind.
- New normalized v1 data includes an authoritative ordered `symbol_kinds`
  inventory. Codegen rejects missing/unknown clause kinds and duplicate inventory
  entries. Legacy v1 data without inventory retains single-kind admission.
- Source frontends still validate symbol-kind declarations and direct import
  visibility. No module qualification is invented for Nitrogen kind strings.

Inferred declaration types, overload selection, nested scopes, document exports,
built-ins and function invocation remain outside this profile. The syntax-only
XML semantic-block limitations are unchanged.

## Validation

- **24 cases frozen from real Nitrogen before the Rust implementation** compare
  parsing, chosen binding kind, UTF-8 spans, nominal types, HIR, values and traces.
  They include priority vs recency, fallback, independent shadowing, initializers,
  no-match/forward references, type failures and rejection before host effects.
- **59 Rust tests** pass, including two namespace pipeline tests and two codegen
  inventory tests. The initializer test proves reference reads do not replay effects.
- **64 generated namespace programs** match live Nitrogen, in addition to the
  existing 51 frozen and 128 generated runtime comparisons.
- **12 exporter tests** pass; native preparation matches **69 valid profiles**
  and both preparers reject the **14 invalid grammars** in the shared corpus.
- Formatting, strict clippy and locked release builds pass. One independent
  read-only review found no actionable correctness defects.

Workspace loading was verified with the actual root `nitrogen.json`, namespace
grammar/helper and sample registered as explicit inputs. The positive sample
passes; a temporary `let x:Scalar=1; rate(x);` document reports `NB0001 unresolved
rate 'x'`. The temporary file was removed. A file-only invocation had selected no
language, so its exit-zero result is not accepted as workspace evidence. Live
VS Code/Rider interaction remains unverified.

A fresh source-only copy, omitting `.NET tools/` and every exported `language.json`,
builds offline with dotnet absent from PATH. It prepares and executes the namespace
sample correctly. The host still has .NET installed elsewhere.

## Measured footprint

macOS arm64, Rust 1.99.1, release profile:

| Artifact | Before | After |
|---|---:|---:|
| Combined CLI | 651,328 B | 668,016 B (652.4 KiB) |
| Profiling executable | 452,496 B | 452,672 B |

The core still has no third-party dependencies. The existing portable/Power/XML
benchmarks have unchanged allocation counts, retained bytes and peaks. The old
parser baseline is preserved in [stage3-phase-memory.json](stage3-phase-memory.json);
current general results are in [phase-memory.json](phase-memory.json).

The new benchmark uses 1,000 pairs: **2,000 declarations, 2,000 references and
4,000 statements**, with 71,342 source bytes. Three runs agree on requested-byte
metrics and allocation counts:

| Phase | Retained | Incremental peak | Allocations |
|---|---:|---:|---:|
| Parse | 1,513,134 B | 1,540,656 B | 46 |
| Bind/check/HIR | 278,528 B | 839,960 B | 6,077 |
| Execution | 16,384 B | 153,616 B | 33 |

Raw results: [stage5-namespace-memory.json](stage5-namespace-memory.json). These count requested
System allocation bytes, not total process memory. Each phase has its own
baseline; peaks cannot be added as a simultaneous process peak. Caller source,
static metadata, allocator overhead and RSS are excluded. JSON is formatted after
snapshots. This is a new workload, with no equivalent pre-namespace comparison.
No startup, throughput or final combined-CLI RSS improvement is claimed.

```sh
cargo test --workspace --locked
python3 scripts/namespaces_reference.py
python3 scripts/prepare_parity.py
python3 scripts/check_reference.py
python3 scripts/check_stage2_reference.py
python3 scripts/differential.py
python3 scripts/stage2_differential.py
python3 scripts/phase_memory.py
python3 scripts/namespace_memory.py
```

Only macOS was executed. The CI matrix includes this fixture but remains unrun
on Linux/Windows. All work remains local, uncommitted and unpublished.
