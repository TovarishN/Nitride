# Nitride

An initial Rust implementation of Nitrogen's language engine, aimed at ordinary
macOS, Linux, and Windows applications with a small native deployment and no
.NET runtime requirement. Nitride is a working name; package availability has
not been checked.

The first working slice covers generated parsing, sequential binding, nominal
type checking, typed HIR, exact host binding preflight, and native execution.
It includes a deliberately restricted typed profile, a cross-module Power
extension, independent sequential symbol namespaces, inferred declaration types, exact nominal
overload selection, built-in callable symbols, typed user-defined templates, host-supplied
built-in values, checked text literals, quote rewrites, grammar functions, and syntax-only admission of existing Base/Uses, XML and CSharp4 grammars.
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
./target/release/nitride --calls languages/calls/sample.calls
./target/release/nitride --functions languages/functions/sample.functions
./target/release/nitride --dates languages/dates/sample.dates --today 2026-10-24
./target/release/nitride --rewrites languages/rewrites/sample.rewrites --today 2026-10-24
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

## Grammar bootstrap

The admitted `.ngr` syntax is now described in
[`Grammar.ngr`](languages/grammar/Grammar.ngr). Preparation uses Nitride's generated
parser, with Rust code converting the syntax tree into the preparation model.
The previous handwritten syntax parser and lexer have been removed.

A checked-in Rust seed breaks the build cycle. Regenerate it with the running
frontend, then rebuild and require an identical result:

```sh
cargo run -p nitride-prepare --example bootstrap
cargo run -p nitride-prepare --example bootstrap -- --check
```

The workspace tests and CI check this fixed point. This self-hosts the admitted
grammar syntax; it does not yet admit the full Nitrogen grammar language.
Literal decoding, admission and code generation remain Rust code. Generated
recognition enforces parser-frame depth/work limits and reports UTF-8 locations;
its syntax errors currently use a generic message. See
[bootstrap evidence and limits](docs/stage9-evidence.md).

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

## Built-in function calls

The Calls fixture resolves function names through actual symbol binding:

```text
let x = duration(4);
sum(x, duration(2));
neg(sum(2, 3));
zero();
sys.neg(7);
```

It returns `[6, -5, 0, -7]`. Unscoped `.ngr` `builtin function` declarations
supply names; `catalog.builtins[kind][name]` supplies exact concrete signatures,
and `catalog.calls[rule].args` lists argument fields. The fixture admits zero,
one and two arguments, including several arities under the same `calc` name.
Dotted names are explicit aliases, not module/member lookup.

All visible local declarations are tried before built-ins. Kind priority applies
within those tiers. A selected local noncallable produces a type error and does
not fall back to a hidden built-in. Calls select exact nominal input tuples,
evaluate arguments once left-to-right and lower to concrete operation HIR for
whole-program host preflight. Built-in binding observations identify the name
explicitly rather than inventing a declaration location.

Only call rules may consume built-in namespaces in this admitted typed profile.
Built-in values, scoped built-ins and callable values remain unsupported in this
profile. Authored template bodies have their own admitted profile below. A generated development-only Nitrogen adapter provides the
fixture's type properties and operation selection; native builds use pure
`Calls.ngr`. See [call evidence](docs/stage7-evidence.md).

## User-defined template bodies

The Functions profile admits typed, document-local definitions and nested calls:

```text
def sum(a: Scalar, b: Scalar) = add(a, b);
def twice(x: Scalar) = call sum(x, x);
call twice(neg(3));
```

It returns `-6`. Definitions can be used before their declaration; parameter
bindings stay within their definition. Return types follow the typed body.
Duplicate declarations, wrong arity/types, invalid unused bodies and used expansion
cycles are checked before execution. Depth and total work bounds protect expansion.

This follows Nitrogen's declarative template semantics. Repeated parameters repeat
argument computation (`neg(3)` runs twice above); unused arguments contribute no
execution effects, while still being bound, checked and lowered. Full ordered
call/definition/argument origins are retained in Program side tables.

The same pure `Functions.ngr` supplies both native and actual Nitrogen semantics.
The C# editor helper contains catalog metadata only. This profile uses fixed
catalog operations in bodies. Mixing sequential globals or arbitrary built-in-call
policies into the template profile is rejected; closures, cross-file exports and
runtime function values remain deferred. Reference diagnostic parity uses the
selected-root API; its identity-result and cyclic-result boundaries are documented
in [function-body evidence](docs/stage8-evidence.md).

## Built-in values and checked date literals

The Dates profile is shaped like DateCalc's core, without its rewrites:

```text
let start = 2026-10-05;
let sprint = 2 * 7 days;
start + sprint;
(2026-12-25 - start) in days;
let pi = pi * 2;
pi;
(today - start) in days;
```

With `--today 2026-10-24` it returns `[739907, 81, 6.283185307179586, 19]`.
Dates are day numbers counted from 0001-01-01; durations are days.

`builtin value { pi today }` names are typed by `catalog.values`. References try
every visible local first, then built-in values, so `let pi = pi * 2;` reads the
built-in and later `pi` reads the local. HIR keeps the built-in reference. The
host supplies each value by exact kind, name and type. Missing, duplicate or
mistyped values fail preflight before any effect. Each value is read at most
once per execution. The CLI's `today` defaults to the current UTC date; there is
no time-zone policy.

`lowers Dates.ParseDate(text Value)` passes the literal's exact spelling as a
`Core.Text` argument to a text host handler. Text is only an operation input:
no operation may return it, and overloads cannot take it.
`catalog.checks` attaches `iso-date` checks to the `Day` rule. `2026-02-30` fails
with `DATE0001` before execution. Check predicates are a fixed engine
vocabulary, not translated C#. Check and type diagnostics are ordered by source
position, with inner nodes first, matching actual Nitrogen.

A generated development-only adapter, `Dates.Reference.ngr` and
`DatesSemantics.cs`, supplies the same types, predicate and overload selection
to actual Nitrogen. See [built-in value evidence](docs/stage10-evidence.md).

## Quote rewrites

[`Rewrites.ngr`](languages/rewrites/Rewrites.ngr) extends Dates with DateCalc's
rewrite forms:

```text
syntax function FortnightDuration(Count: Expr) : Expr = <[ $Count * 14 days ]>;
syntax function SwapAddition(Value: Expr) : Expr = match Value
{
  | (Left: Expr, Right: Expr) <[ $Left + $Right ]> => <[ $Right + $Left ]>;
  | _ => <[ $Value ]>;
};
| Fortnights = Count:Expr "fortnights" precedence 30 rewrites FortnightDuration(Count)
| Soon = "~" Amount:Expr precedence 40 rewrites <[ today + $Amount days ]>
```

A use means its template, with each `$` hole filled by the caller's own syntax.
`3 fortnights` types, lowers and runs as `3 * 14 days`. Templates are parsed by
the language's generated parser at compile time, with holes standing for one
node of their category. Every template, used or not, is validated on each
compile. Names inside templates resolve only against built-ins, so
`let today = …; ~1;` still uses the built-in `today`.

Match arms compare the original caller syntax structurally, before any
expansion, keeping parentheses: `count + 2 swapped` is `2 + count`, while
`(count + 2) swapped` takes the fallback. Only HIR reachable from statements
is emitted, so discarded caller syntax never runs, and a repeated hole repeats
its effects, as in Nitrogen. HIR origins record the use and template-relative
`nitrogen:rewrite` spans. Errors inside an expansion are reported at the use,
as `in the expansion of Swapped: …`. List splices, grammar functions and C#
quotation APIs are not admitted. See
[rewrite evidence](docs/stage11-evidence.md).

## C# (CSharp4)

`languages/csharp4` vendors CSNitrogen's CSharp4 grammar (30 modules, with
SHA-256 provenance). Nitride prepares it natively and parses valid,
preprocessing-free C#:

```sh
./target/release/nitride --parse-csharp4 languages/csharp4/corpus/Mono2008/gtest-linq-01.cs
```

Trees match actual Nitrogen on 238 frozen cases and 64 generated programs.
This needed grammar functions (Nitrogen's compile-time `function ... :
Grammar.Rule`), `transparent` alternatives, operators that are rule references
or shared between alternatives, prefix associativity, and UTF-16 surrogate-pair
classes, which codegen turns into supplementary code points over UTF-8 text.
Tokens are parse-tree leaves. The parser has its own frame budget and a
size-relative work budget, so multi-megabyte files parse in linear time. It is
currently about 6× slower than Nitrogen's parser. See
[CSharp4 evidence](docs/stage12-evidence.md).

## Recovery

Recovery is opt-in and retains original source, missing elements and skipped
regions while continuing supported typed analysis:

```sh
./target/release/nitride --recover csharp4 broken.cs
./target/release/nitride --recover inference broken.inferred
./target/release/nitride-profile --recover csharp4 broken.cs
```

The single-file CLI returns partial syntax/HIR and diagnostics as JSON, exits 1 when there
are diagnostics, and never executes recovered input. `--recover-batch` accepts
JSONL requests with `profile` and `source`. Library callers use
`parse_recovering` and `compile_recovering`; private program diagnostics block
all execution paths. Strict APIs keep their prior behavior.

Recovery is grammar-derived and bounded. The frozen corpus passes the approved
Roslyn aggregate gates; case-level regressions, fallback and large-file costs
are explicit in [recovery evidence](docs/stage14-evidence.md). Editor recovery integration remains future work.

## Shared syntax snapshots

`parse_snapshot` opts into immutable chunked syntax and UTF-8 source storage.
Pratt memos, alternative winners and selected recovery probes retain structural
handles. An edit shares compatible subtrees with the previous snapshot:

```rust
use nitride_core::{parse_snapshot, Limits, ParseOptions, SnapshotMode, TextEdit};
fn example(language: &'static nitride_core::Language) -> Result<(), nitride_core::Diagnostic> {
let before = parse_snapshot(language, "1 + 2;", Limits::default(),
    SnapshotMode::Strict(ParseOptions::default()))?;
let after = before.edit(&TextEdit { range: [4, 5], replacement: "3".into() },
    Limits::default())?;
let root = after.root();
assert_eq!(root.text(), "1 + 3;");
Ok(())
}
```

Spans belong to snapshot occurrences; `SubtreeId` identifies shared structure.
Old snapshots remain readable. Explicit maintenance can relocate structure in
the newest snapshot while preserving every older reader. `materialize()` is the
explicit dense flat-tree/source export. Diagnostic and repair slices resolve
relative positions lazily on request; `diagnostic_count()` avoids that export.
Existing `parse`/`SyntaxTree` APIs retain
their compatibility backend.

Use `SnapshotMode::Recovering` for damaged input. The recovery VM can reuse
strict recognition and coverage blocks; uncertain or exhausted paths retain an
explicit full-parser fallback. `stats()` distinguishes that fallback, copying,
reused subtrees and maintenance. `compile_snapshot` and
`compile_snapshot_recovering` consume occurrence views and recompute semantics;
shared syntax alone does not reuse bindings. Diagnosed programs remain blocked
from execution.

The measured costs and admission failures are in
[shared-subtree evidence](docs/stage15-evidence.md). The follow-up
[predictive recognition prototype](docs/stage16-evidence.md) measures prepared
start filtering and bounded lexical reuse for cold parsing. The subsequent
[ordinary-choice dispatch checkpoint](docs/stage17-evidence.md) adds bounded
ordered eligibility routes and reports the remaining Roslyn gap. The opt-in
[generated lexical execution checkpoint](docs/stage18-evidence.md) specializes
acyclic lexical rules through `parse_snapshot_compiled`; edited ropes remain
interpreted. The following [generated syntax checkpoint](docs/stage19-evidence.md)
adds `parse_snapshot_generated` for structural bodies/infix tails and an equivalent
native trivia fast path. A subsequent [trivia-cache experiment](docs/stage20-evidence.md)
was rejected after benchmarks showed no reliable speed gain. The subsequent
[memo key hashing checkpoint](docs/stage21-evidence.md) reports a modest measured
gain from contiguous key hashing with unchanged cache-entry size. The next
[arena lookup checkpoint](docs/stage22-evidence.md) avoids directory hashing for
the current chunk after slot recycling. A later
[label-cache experiment](docs/stage23-evidence.md) was rejected after its small
large-file timing gain came with increased memo-budget eviction churn. The
[fused ASCII lexical checkpoint](docs/stage24-evidence.md) replaces first-byte-
determined scanner decisions with immutable tables and exact work/depth/read
accounting. The [seeded cold recovery checkpoint](docs/stage26-evidence.md)
reuses a failed strict attempt's recognition facts in recovery instead of
recognizing the prefix again. The [hot-path checkpoint](docs/stage27-evidence.md)
inlines the stack guard, hashes parser-private maps with a seeded fast hasher and
moves cache eviction off the backtrack path. The
[post-error checkpoint](docs/stage28-evidence.md) lets cold recovery pre-recognize
missing facts after the first error with the generated parser. The
[token-lead checkpoint](docs/stage29-evidence.md) rejects alternatives and Pratt
operators by their first two tokens and precomputes Pratt dispatch. The
[token-cache checkpoint](docs/stage31-evidence.md) reads each token once per position
and generates every choice. The [arena checkpoint](docs/stage32-evidence.md)
removes per-release stack setup and published-node directory lookups. The
[publication checkpoint](docs/stage33-evidence.md) publishes cold strict graphs in
place and predicts lexical tokens without building leaves. A
[three-way comparison](docs/stage30-comparison.md) measures Nitride, Roslyn and
Nitrogen on cold and warm C# parses with and without errors. The
[incremental checkpoint](docs/stage34-evidence.md) compares single-character edits
with Roslyn and bounds recovery's prefix reopening, which made edits in files
with an existing error and dense-error parses faster. The
[recovery publication checkpoint](docs/stage35-evidence.md) speeds cold parses
with errors by evacuating with dense tables and merging seed facts in one pass. The
[probe filtering checkpoint](docs/stage36-evidence.md) skips recovery probes of
lexical rules that cannot consume the next character. The
[exhausted-search checkpoint](docs/stage37-evidence.md) keeps shared recovery results
whose only limit is a local search exhaustion instead of rerunning flat recovery. The
[generated-failure checkpoint](docs/stage38-evidence.md) lets recovery probes take
failures from the generated parser instead of interpreting them again. The
[scanned-ends checkpoint](docs/stage39-evidence.md) does the same for predicate
probes and reads token ends from compiled scanners. The
[post-error checkpoint](docs/stage40-evidence.md) bounds fact lookups past the
seed and walks published nodes through a dense chunk index. The
[arena slot checkpoint](docs/stage41-evidence.md) maps private chunk ids to slots
densely and records where the valid path's remaining cost lies. Recovery probes otherwise
retain their original recognition work accounting. Cold construction, recovery
search, explicit export and semantic indexing are separate costs; this API
does not promise constant-time edits.


## Editor support

`nitride lsp` is a Language Server Protocol server over stdio, and
`editors/vscode` is a VS Code extension that bundles it. Build and install the
extension for this host (on macOS with Apple silicon, for example):

```sh
scripts/package-vscode.sh darwin-arm64
code --install-extension editors/vscode/nitride-darwin-arm64-0.1.0.vsix
```

Use `darwin-x64`, `linux-x64` or `win32-x64` on those hosts. The server reads
`nitride.json` at each workspace folder root, else `nitrogen.json`. Both list
languages by `name`, `extensions`, `grammars` and `start`; `nitride.json` may
also give a `catalog` and a `mode` (`typed` or `syntax`, default `typed` when a
catalog is given). Paths are relative to the manifest. This repository's
`nitride.json` lists its own languages. Languages are prepared from their
grammars at runtime and reloaded as grammars change; in syntax mode, Nitrogen's
C# semantic members are skipped with a warning.

The server provides diagnostics, semantic coloring, outline, folding and
selection ranges, plus hover (types in typed languages), go to definition,
references, highlights, rename and workspace symbols, for `.ngr` grammars and
workspace programs. Editing help: completion of names, built-ins, rules and
keywords; signature help for built-in calls; document, range and on-type
formatting (whitespace only); inlay hints with each statement's computed value;
and quick fixes from the parser's repairs, plus removing Nitrogen C# semantic
members from grammars. A client that sends `initializationOptions.paletteTokens:
true` gets Nitrogen's TextMate palette token types: literals colour by their role
in their rule (declaration keyword, section keyword, modifier, constant,
operation word, brackets, terminators, separators), and `.ngr` files by the
scopes of their TextMate grammar. Grammar rules navigate and rename across every grammar file
of a language. Syntax-only languages bind names by spelling through their
`declares` and `references` clauses. While the Nitride extension is installed,
Nitrogen's VS Code extensions stay inactive.

## Validate

```sh
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

There are 186 passing Rust tests (plus two opt-in scaling tests), including parity over 481 frozen observations from
actual Nitrogen parsers and, for typed profiles, binding, typing and HIR,
with the explicit inference-adapter policy described above.
The development exporter/composition/recovery-metric Python suite has 22 tests. Live differential scripts compare
576 deterministic generated programs in total. `scripts/prepare_parity.py`
compares 107 prepared profiles and 29 invalid grammars against the live exporter.

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
Nitrogen extension discover the portable, composed, XML, namespace, inference, call, function, dates, rewrites and grammar-language fixtures. Its workspace checker was verified; live VS Code/Rider interactions
and editor completion/navigation were not exercised.

## Scope and evidence

The grammar is authored in `.ngr`. The Rust frontend resolves modules and
extensions, validates the supported profile and generates static language tables.
The development-only .NET exporter remains an independent parity reference.
The admitted profiles support ordered module imports and extensions, multiple declared symbol
namespaces, token/sequence/choice/repetition, character classes, lookahead and
Pratt-style expression alternatives.
Unsupported grammar features and incompatible catalogs fail preparation.

The portable core does not translate C# helpers. General .ngr features, incremental editing,
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
