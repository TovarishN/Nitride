# Nitride

An initial Rust implementation of Nitrogen's language engine, aimed at ordinary
macOS, Linux, and Windows applications with a small native deployment and no
.NET runtime requirement. Nitride is a working name; package availability has
not been checked.

The first working slice covers generated parsing, sequential binding, nominal
type checking, typed HIR, exact host binding preflight, and native execution.
It includes a deliberately restricted typed profile, a cross-module Power
extension, independent sequential symbol namespaces, inferred declaration types, exact nominal
overload selection, and syntax-only admission of existing Base/Uses and XML grammars.
This is partial Nitrogen compatibility; the full DateCalc language is not admitted.

## Run

With stable Rust/Cargo installed, from this repository:

```sh
cargo build --release
./target/release/nitride languages/portable/sample.portable
./target/release/nitride --power languages/power/sample.powercalc
./target/release/nitride --parse-xml languages/xml/sample.xml
./target/release/nitride --namespaces languages/namespaces/sample.namespaces
./target/release/nitride --inference languages/inference/sample.inferred
cargo run -p nitride-cli --example embed
```

The CLI prints a JSON observation including diagnostics or values. The embedding
example prints `42` and calls the Rust library directly, without a subprocess.
Use `nitride.exe` on Windows.

Cargo prepares the admitted `.ngr` grammar files directly in Rust and generates
static language tables. No .NET process, assembly, runtime or pre-exported
`language.json` is required to build or run the application.
The Rust core has no third-party runtime dependencies. The example CLI uses
`serde_json` to print observations. Its preparation tooling additionally uses
SHA-256 for source/catalog identity; these dependencies do not enter the core.

## Prepare a language

```sh
./target/release/nitride prepare languages/power/project.json
./target/release/nitride prepare languages/power/project.json --output /tmp/power-language.json
```

The ordered manifest supplies grammar paths, a catalog, an explicit syntax start
rule and `typed` or `syntax` mode. Paths are relative to the manifest. Preparation
prints normalized JSON to stdout by default. `--output` replaces a file only after
successful validation and protects grammar/catalog/manifest inputs from overwrite.
Repository text uses LF checkouts on all platforms via `.gitattributes`.
Source/catalog hashes identify exact text bytes; line-ending changes change identity.
Grammar errors carry a file, line, column and UTF-8 byte span. Shared profile-gate
errors currently fall back to the first source at byte zero with an error message.

Syntax-only C# blocks are opaque balanced regions: Nitride records their omission
and does not validate their C# or Nitrogen semantic-statement contents. Ordinary
quoted strings/comments are scanned to locate the block end; raw/verbatim C#
strings are outside this profile. Parsed declarative clauses are validated before
being omitted. General grammar-function and semantic-helper support remains deferred.

## Example language

```text
let x : Scalar = 6;
x * 7;
let t : Time = duration(3);
t;
```

`Scalar` and `Time` are different nominal types. `duration(3) + 2` is a type error;
there is no implicit conversion. Declarations are sequential, and a later
declaration can shadow an earlier one. References use the earlier declaration
inside a shadowing initializer. Host operations run only after the complete
program and its exact operation bindings have passed preflight. Initializer
results are stored and references do not replay their effects.

## Separate symbol namespaces

The `namespaces` fixture uses `value` and `rate` as independent binding kinds
(`rate` is a namespace here, not a numeric unit):

```text
let x : Scalar = 10;
rate x : Scalar = 20;
either(x);   // references (value | rate): 10
reverse(x);  // references (rate | value): 20
```

References search kinds in authored order and select the latest visible
sequential declaration of the first matching kind. Recency in another kind does
not override that order. Shadowing remains per kind, initializers see previous
declarations, and references reuse stored results. Kind selection does not
convert nominal types or retry another kind after a type mismatch.

New normalized v1 exports contain an authoritative `symbol_kinds` inventory.
Legacy v1 exports without that field retain single-kind admission. Native parser
and .NET exporter both enforce source-level kind declaration/import visibility.

## Inference and overloads

The inference fixture supports both annotated and inferred declarations:

```text
let x = 2;
let x = value(x) + 3;
rate x = duration(4);
either(x);
reverse(x) + duration(2);
reverse(x) - duration(1);
```

This produces `[5, 6, 3]`. The shadowing initializer reads the prior `x`.
Unannotated declarations require an explicit `"infer": true` catalog policy;
references take the nominal type of the selected prior initializer. Checking
never invokes host handlers, and execution evaluates each initializer once.

Catalog overload groups select a concrete operation by its exact nominal input
tuple. Scalar addition selects `Inference.AddScalar`; Time addition selects
`Inference.AddTime`. Time subtraction returns Scalar in this fixture. The result
context does not select an overload; conversions and namespace retries are absent.
Preparation rejects empty groups, unknown operations and duplicate input tuples,
even when their results differ. HIR retains the chosen concrete signature for
whole-program host preflight.

The pure `Inference.ngr` is the native build input. A generated development-only
`Inference.Reference.ngr` and C# helper supply inferred types and overload selection
to actual Nitrogen for reference/editor checks. Nitrogen's declarative symbol
lookup alone does not infer these types. The reference adapter also specifies
initializer consistency and primary type-error ordering; this is an explicit
fixture policy, not a claim about Nitrogen's built-in diagnostic ordering.
Adapter source hashes are recorded in `reference-provenance.json`.

## Validate

```sh
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

There are 68 Rust tests, including parity over 108 frozen observations from
actual Nitrogen parsers and, for typed profiles, binding, typing and HIR,
with the explicit inference-adapter policy described above.
The development-only exporter has 12 tests. Live differential scripts compare
256 deterministic generated programs in total. `scripts/prepare_parity.py`
compares 70 prepared profiles and 14 invalid grammars against the live exporter.

To refresh the reference using a sibling `../Nitrogen` source checkout and a
.NET 10 SDK or later:

```sh
dotnet build tools/Nitride.Reference
python3 scripts/test_reference.py
dotnet tools/Nitride.Reference/bin/Debug/net10.0/Nitride.Reference.dll export languages/portable/Portable.ngr languages/portable/catalog.json languages/portable/language.json
python3 scripts/editor_metadata.py
python3 scripts/reference.py
cargo test --workspace
```

Set `-p:NitrogenRoot=/absolute/path/to/Nitrogen` on `dotnet build` if the checkout
is elsewhere. `scripts/check_reference.py` checks the frozen observations
against live Nitrogen without overwriting them. After a native release build,
`scripts/differential.py` compares 64 deterministic generated programs to the
live reference.

The registered `nitrogen.json` and generated helper metadata let the existing
Nitrogen extension discover the portable, composed, XML, namespace and inference language fixtures. Its workspace checker was verified; live VS Code/Rider interactions
and editor completion/navigation were not exercised.

## Scope and evidence

The grammar is authored in `.ngr`. The Rust frontend resolves modules and
extensions, validates the supported profile and generates static language tables.
The development-only .NET exporter remains an independent parity reference.
The admitted profiles support ordered module imports and extensions, multiple declared symbol
namespaces, token/sequence/choice/repetition, character classes, lookahead and
Pratt-style expression alternatives.
Unsupported grammar features and incompatible catalogs fail preparation.

The portable core does not translate C# helpers. General .ngr features, recovery,
incremental editing,
quotation, workspace indexing, LSP and dynamic language loading remain future work. Embedded targets, `no_std`, and custom allocators
are not requirements.

Native arithmetic uses finite `f64`. Nitrogen's current HIR literal constants
use `float`; the shared corpus uses exactly representable values. Broader
floating-point parity is not admitted. The reference adapter uses an explicit
fixture host over real Nitrogen HIR for its nominal `Time` type, because
Nitrogen's generic numeric evaluator currently accepts only Scalar/Angle.

XML admission covers syntax only: closing-tag semantic checks and pattern
warnings remain omitted, explicitly recorded in generated metadata. Equal-rank
ambiguous parses and conflicting infix operator spellings are rejected.

See [cross-platform validation](docs/ci-evidence.md), [design](docs/design.md), [progress and review fixes](docs/progress.md), and
[inference and overload evidence](docs/stage6-evidence.md),
[namespace compatibility evidence](docs/stage5-evidence.md),
[native preparation evidence](docs/stage4-evidence.md), and
[parser allocation evidence](docs/stage3-evidence.md). The hosted CI matrix passes on macOS, Linux and Windows, including native
preparation and sample execution. Detailed footprint measurements remain macOS-only. This is a starting slice
of the rewrite, with no existing Nitrogen tools replaced.
