# Nitride

Nitride is an experimental Rust implementation of the language-engine ideas in
[Nitrogen](https://github.com/TovarishN/Nitrogen). It prepares `.ngr` grammars,
generates native parsers, and provides binding, type checking, typed intermediate
representation (HIR), and execution for explicitly supported language profiles.
It includes shared incremental syntax snapshots, error recovery, and a language
server with a VS Code extension.

The target is ordinary macOS, Linux, and Windows applications that want to embed
a language without deploying .NET. Nitrogen compatibility is partial and tested
feature by feature. Nitride is a working name; package-name availability has not
been checked.

## Why Nitride exists

Nitrogen combines composable grammars with semantic rules and explicit host
operations, allowing a language definition to serve parsing, checking, execution,
and editor tools. Nitride explores carrying that model into a native Rust library.

The motivation is deployment: a Rust application should be able to embed its DSL
engine directly. Native preparation reads supported grammars and catalogs without
starting .NET; Cargo generates parsers for bundled languages. The .NET exporter
remains an independent development reference for compatibility checks.

Rust makes ownership and retained syntax storage explicit, providing a way to
investigate memory use and subtree sharing. It does not guarantee smaller trees
or faster parsing. Those properties require measurements, and the recorded
benchmarks show substantial remaining costs, especially for recovery.

Portability has a deliberate boundary: arbitrary C# semantic helpers are not
translated. Supported semantics use declarative grammar clauses, catalog metadata,
and explicit host bindings. The native execution contract stays inspectable;
broader Nitrogen compatibility remains unfinished.

## Nitride, Nitrogen, and Roslyn

[Nitrogen](https://github.com/TovarishN/Nitrogen) is the upstream .NET language
workbench and compatibility reference. Its grammars, language model, and reference
implementation underpin Nitride. This separate implementation does not replace
existing Nitrogen tools.

[Roslyn](https://github.com/dotnet/roslyn) is Microsoft's open-source C# and Visual
Basic compiler platform. Its syntax API is an independent C# parsing baseline.
The measurements here cover parsing, rather than its full compiler and IDE stack.

| Aspect | Nitride | Nitrogen | Roslyn |
|---|---|---|---|
| Purpose | Native engine for supported `.ngr` languages | .NET language workbench | C# and Visual Basic compiler platform |
| Deployment | Rust native library and executables | C# / .NET | C# / .NET |
| Language definition | Supported `.ngr` subset and catalogs | `.ngr` with .NET semantic integration | Built-in C# and Visual Basic implementations |
| Semantics | Admitted binding, nominal types, HIR, host operations | Grammar-driven semantics, C# helpers, typed HIR | C# / Visual Basic semantic models and compilation |
| Editing | Shared syntax snapshots, recovery, LSP, VS Code | Incremental parsing, LSP, VS Code and Rider plugins | Syntax and workspace APIs for editor tooling |
| C# role here | Syntax-only CSharp4 grammar | Reference grammar through generated modules | Independent C# syntax parser baseline |

The CSharp4 fixture does not make Nitride a C# compiler. It admits a historical
grammar without Roslyn's C# type system, compilation, or modern language coverage.
XML admission is also syntax-only and omits XML semantic checks.

### Parsing assessment, 2026-10-30

The [updated assessment](docs/compiler-assessment-2026-10-30.md) measures Nitride
`fa9b5c6` on an **Apple M4 Pro**, including PGO, native CPU targeting and optional
ASCII batching, against fresh Nitrogen 0.19.0 and Roslyn 5.0.0 runs on .NET 10.
The [October 28 comparison](docs/stage45-comparison.md) remains the historical
checkpoint for memory and incremental edits.

Full parses of generated valid C# 4 source, seconds (lower is better):

| Input | Nitride portable | Nitride PGO | Nitrogen cold / warm | Roslyn cold / warm |
|---|---:|---:|---:|---:|
| 256 KiB | 0.054 | 0.035 | 0.350 / 0.112 | 0.077 / 0.042 |
| 4 MiB | 0.812 | 0.533 | 1.450 / 0.808 | 0.704 / 0.151 |
| 16 MiB | 3.385 | 2.240 | 4.207 / 3.271 | 1.411 / 0.778 |

PGO reduced Nitride parse time by **32–35%** across eight primary valid/damaged
C# cases. On valid files, this puts Nitride ahead of Nitrogen cold and warm at
all three sizes, and ahead of Roslyn cold at 256 KiB and 4 MiB. At 16 MiB,
Roslyn remains 1.6× faster cold. Roslyn's warm full parses are 3.5× faster than
PGO Nitride at 4 MiB and 2.9× at 16 MiB. Native CPU targeting adds no consistent gain.

**The SIMD feature currently uses scalar batching on Apple Silicon.** Its
explicit vector classifier uses AVX2 on x86-64; no ARM NEON classifier is
implemented. On this M4 Pro, batching makes the 1 MiB string case 2.66× faster,
while mixed-code results show only small gains and regressions. Combining
batching with PGO adds no consistent mixed-code benefit. The separate
[Windows AVX2 experiment](docs/ascii-scanner-runs.md) measured 3.45× over
default on that long-string workload, including a 1.21× gain over scalar batching.
Those results do not establish a general parser speedup or a NEON speedup.

Recovery still has a substantial gap on dense errors: at 256 KiB, PGO Nitride
is 3.1× Roslyn cold and 6.7× Roslyn warm. At 4 MiB with one error or junk, PGO
Nitride is close to Roslyn cold, while Roslyn warm is 4.4–5.0× faster. Nitrogen
stops at its first syntax error in this harness, so damaged-input timings
measure different work and are not a whole-file recovery comparison.

Cold means the first parse in a fresh process, including .NET JIT work;
process launch, source I/O and Nitrogen language construction are excluded.
Warm means repeated full parsing after two warmups, **not incremental editing**.
Native cells are medians of five fresh processes. PGO training uses separate
64 KiB inputs from the same generator family; native and .NET batches run
sequentially on identical inputs. This is one synthetic corpus on one host,
with different language coverage and tree contracts. It does not establish
real-project or full-compilation performance.

The October 28 snapshot-edit measurements remain useful historical evidence:
Nitride took about 0.16× Roslyn's time for single-digit edits in valid files
and 0.59–0.74× with one existing error. That checkpoint also recorded larger
peak memory than Roslyn on large files. Neither edits nor memory were remeasured
with PGO/batching; these parse-time gains must not be applied to them.

This assessment changes no ordinary Cargo release settings or ASCII feature
defaults. The [native release pipeline](docs/releases.md) separately applies
PGO/LTO and Apple M1 CPU tuning on macOS; its binaries and training workload
were not measured here. These gains belong to the standalone parser harness;
deployment should measure the actual executable. The assessment
links [native samples](docs/compiler-optimization-macos.json),
[.NET samples](docs/compiler-comparison-macos.json), correctness fingerprints,
assembly identities, training limits and [reproduction commands](scripts/compiler-audit/README.md).

## Try it

Install a current stable Rust toolchain and run from the repository root:

```sh
cargo build --release --locked
./target/release/nitride languages/portable/sample.portable
./target/release/nitride --power languages/power/sample.powercalc
./target/release/nitride --parse-xml languages/xml/sample.xml
./target/release/nitride --parse-csharp4 languages/csharp4/corpus/Mono2008/gtest-linq-01.cs
cargo run --locked -p nitride-cli --example embed
```

On Windows use `target/release/nitride.exe`. The CLI emits JSON observations;
the embedding example calls the Rust library directly and prints `42`.
Native builds require neither .NET nor checked-in `language.json` exports.

The portable language includes nominal types and sequential declarations:

```text
let x : Scalar = 6;
x * 7;
let t : Time = duration(3);
t;
```

`Scalar` and `Time` are distinct types. References reuse initializer results.
Execution starts only after the program and its exact host-operation bindings
pass preflight.

Prepare a supported language from its ordered manifest:

```sh
./target/release/nitride prepare languages/power/project.json
./target/release/nitride prepare languages/power/project.json --output prepared-power.json
```

The manifest selects grammars, catalog, start rule, and typed or syntax mode.
Exports are ignored by Git; grammars and catalogs remain source inputs.
Unsupported constructs and incompatible catalogs fail preparation.

## Features and boundaries

- Grammar modules and extensions, tokens, sequences, choices, repetition,
  lookahead, character classes, and Pratt-style expressions.
- Admitted typed profiles for separate symbol namespaces, inferred declarations,
  exact nominal overloads, built-in calls and values, template bodies, checked
  dates, and quote rewrites. See the examples under [languages](languages).
- Immutable syntax snapshots with shared subtrees and bounded recovery. Older
  snapshots remain readable. Export and semantic recomputation have separate
  costs; incremental syntax does not imply incremental binding.
- A native grammar frontend bootstrapped from a checked-in Rust seed, with a
  [regeneration fixed-point check](docs/stage9-evidence.md).

Full Nitrogen grammar/semantic compatibility and the complete DateCalc language
are unfinished. Arbitrary C# helpers, full quotation APIs, and list splices are
outside the admitted native profiles. Native execution uses finite `f64`;
Nitrogen's numeric literal representation differs, so broad floating-point parity
is not claimed. Embedded targets and `no_std` are not current requirements.

## Editor support

`nitride lsp` serves LSP over stdio. The [VS Code extension](editors/vscode/README.md)
bundles it, providing diagnostics, coloring, navigation, rename, completion,
formatting, hints, and parser-repair quick fixes for supported profiles.

The server reads the workspace's `nitride.nproj` and the `.nproj` language files
it includes (see `docs/superpowers/specs/2026-10-29-project-language-design.md`),
falling back to Nitrogen's `nitrogen.json`, and prepares languages from grammar
sources at runtime. Syntax mode skips Nitrogen C# semantic
members with a warning. Package and install for your host, for example:

```sh
scripts/package-vscode.sh darwin-arm64
code --install-extension editors/vscode/nitride-darwin-arm64-0.1.0.vsix
```

Other packaging targets are `linux-x64` and `win32-x64`.

A file can also choose its language with a Nitrogen signature such as
`#!nitrogen language Calc`, declared by a `#!nitrogen grammar` file; see
`languages/signed/` and the extension README.

## Development and evidence

```sh
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p nitride-prepare --example bootstrap -- --check
```

The [CI workflow](.github/workflows/ci.yml) runs native checks on Linux, macOS,
and Windows. Recorded evidence describes specific checkpoints; it is not a claim
that tests were rerun for every documentation update.

Live Nitrogen parity checks additionally require a sibling `../Nitrogen` checkout
and the .NET 10 SDK. If needed, set `-p:NitrogenRoot=/absolute/path/to/Nitrogen`
when building the reference tool:

```sh
dotnet build tools/Nitride.Reference
python3 scripts/test_reference.py
python3 scripts/check_reference.py
```

Reference adapters and helper metadata are development tools. Frozen observations
and corpus provenance remain versioned for independent compatibility checks.

| Evidence | Scope |
|---|---|
| [Design](docs/design.md) | Original rationale and architecture, including historical plans |
| [CSharp4 admission](docs/stage12-evidence.md) | Grammar provenance and reference-tree parity |
| [Recovery](docs/stage14-evidence.md) | Quality checks and recovery limits |
| [Shared snapshots](docs/stage15-evidence.md) | Storage sharing, edits, and admission costs |
| [Three-way comparison](docs/stage30-comparison.md) | Full-parse timing and memory methodology, 2026-10-27 |
| [Refreshed comparison](docs/stage45-comparison.md) | Full-parse and edit timing, memory, change since stage30 |
| [PGO and SIMD reassessment](docs/compiler-assessment-2026-10-30.md) | Apple M4 Pro compiler configurations, fresh .NET comparison, ARM SIMD boundary |
| [Incremental checkpoint](docs/stage34-evidence.md) | Edit measurements and Roslyn baseline |
| [Latest recorded parser checkpoint](docs/stage44-evidence.md) | Paired timings, recovery filtering, validation |

## Credits

Nitrogen supplies the language-workbench model and independent reference engine.
The CSharp4 grammar and Mono corpus carry provenance and license records under
[languages/csharp4](languages/csharp4). Roslyn supplies an independent C# baseline;
its inclusion implies no affiliation or endorsement. Preserve accompanying
third-party notices when redistributing those sources.

## Native releases

See [release versioning and build instructions](docs/releases.md) for the
three native VS Code packages and CLI archives, PGO/LTO/AVX2/BOLT settings,
CPU compatibility, and the native release configuration without Wasmtime.
