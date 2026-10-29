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

### Recorded parsing comparison

The [refreshed three-way report](docs/stage45-comparison.md) records **2026-10-28
measurements on one macOS ARM64 host**: Nitride `0954c58`, Nitrogen 0.19.0 and
Roslyn 5.0.0. Both .NET harnesses used .NET 10.0.0, Release builds, workstation GC.
It repeats the [2026-10-27 report](docs/stage30-comparison.md) with the same corpus
and harnesses. That earlier report stays as the historical baseline.

Full parses of generated valid C# 4 source, seconds (lower is better):

| Input | Nitride cold / warm | Nitrogen cold / warm | Roslyn cold / warm |
|---|---:|---:|---:|
| 256 KiB | 0.050 / 0.050 | 0.323 / 0.108 | 0.068 / 0.033 |
| 4 MiB | 0.783 / 0.781 | 1.30 / 0.759 | 0.646 / 0.134 |
| 16 MiB | 3.16 / 3.18 | 4.06 / 3.15 | 1.19 / 0.600 |

Cold means the first parse in a fresh process, including .NET JIT work; process
startup and source I/O are excluded. Warm means repeated full parsing after
warm-up, **not incremental editing**. Nitrogen language construction is excluded.

On cold parses, Nitride was fastest at 256 KiB and faster than Nitrogen at every
size. Roslyn was faster from 4 MiB up: by 1.2× at 4 MiB and 2.7× at 16 MiB.
Nitride does not speed up when warm, while Roslyn's warm parses were 5–6× faster
than Nitride's from 4 MiB up.

Single-character edits, median milliseconds, through Roslyn's `WithChangedText`
and Nitride's snapshot edits:

| File | Nitride | Roslyn |
|---|---:|---:|
| 4 MiB valid | 1.72 | 11.3 |
| 16 MiB valid | 8.55 | 52.1 |
| 4 MiB with one syntax error | 7.76 | 11.4 |
| 16 MiB with one syntax error | 32.0 | 54.5 |

Recovery requires a separate reading. In this harness the Nitrogen reference
stops at the first error and retains a partial tree, so its damaged-input timings
do not measure the same work as whole-file recovery in Nitride and Roslyn.
Diagnostic grouping also differs between Nitride and Roslyn on inserted junk.

Cold full parses with errors, against Roslyn:

| Input | Nitride s | Roslyn s |
|---|---:|---:|
| 256 KiB, one error | 0.063 | 0.074 |
| 4 MiB, one error | 0.975 | 0.641 |
| 16 MiB, one error | 3.94 | 1.22 |
| 4 MiB, junk | 0.974 | 0.676 |
| 4 MiB, dense errors | 7.73 | 0.656 |

Dense errors remain Nitride's widest gap: 12× Roslyn at 4 MiB, down from 133× on
2026-10-27.

Memory accounting differs between runtimes. Kept trees were about the same size
in all three (65–73 MiB for 4 MiB valid). Peak process memory was lowest for
Nitride at 256 KiB. From 4 MiB up it was lowest for Roslyn: Nitride's peak was
1.5× Roslyn's on 4 MiB valid input and 3× with errors.

The [raw results](docs/stage45-comparison.json) and [edit results](docs/stage45-incremental.json)
include individual samples, diagnostic counts and memory observations. The
[harness](docs/stage30-harness) contains host-specific paths requiring adjustment.
This generated corpus on one host does not establish performance across real
projects or full compilation, and Roslyn's parser does more work than this
comparison uses: it covers every C# version and builds full-fidelity trees.

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

The server reads `nitride.json`, falling back to `nitrogen.json`, and prepares
languages from grammar sources at runtime. Syntax mode skips Nitrogen C# semantic
members with a warning. Package and install for your host, for example:

```sh
scripts/package-vscode.sh darwin-arm64
code --install-extension editors/vscode/nitride-darwin-arm64-0.1.0.vsix
```

Other packaging targets are `darwin-x64`, `linux-x64`, and `win32-x64`.

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
| [Incremental checkpoint](docs/stage34-evidence.md) | Edit measurements and Roslyn baseline |
| [Latest recorded parser checkpoint](docs/stage44-evidence.md) | Paired timings, recovery filtering, validation |

## Credits

Nitrogen supplies the language-workbench model and independent reference engine.
The CSharp4 grammar and Mono corpus carry provenance and license records under
[languages/csharp4](languages/csharp4). Roslyn supplies an independent C# baseline;
its inclusion implies no affiliation or endorsement. Preserve accompanying
third-party notices when redistributing those sources.
