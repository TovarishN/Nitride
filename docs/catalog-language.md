# Catalog language (design draft)

Status: draft for review. Replaces `catalog.json` with a Nitride-defined
language. Decided so far: catalogs stay separate files, they are parsed
natively (JSON is retired), and names are module-scoped.

## Why

`catalog.json` is the semantic contract between a grammar and its host: types,
host operations, overload groups, statement policies, built-ins and source
checks. As JSON it has no comments, repeats `Dates.` on every name, and errors
come from codegen without a source position (`unknown operation type`). Editors
can neither navigate from `lowers Dates.Plus(...)` to the overload nor check a
catalog while it is typed. A small language fixes all of this, and the LSP's
grammar machinery serves it as it serves `.ngr`.

## Example: Dates

```ngr
// Semantic contract of the Dates profile: dates are day numbers, durations days.
catalog module Dates
{
  type Date;
  type Duration;

  operation ParseDate(Text) : Date = host iso-date;
  operation Days(Scalar) : Duration = host identity;
  operation InDays(Duration) : Scalar = host identity;
  operation AddScalar(Scalar, Scalar) : Scalar = host add;
  operation Later(Date, Duration) : Date = host add;
  operation AddDurations(Duration, Duration) : Duration = host add;
  operation SubScalar(Scalar, Scalar) : Scalar = host sub;
  operation Earlier(Date, Duration) : Date = host sub;
  operation Between(Date, Date) : Duration = host sub;
  operation SubDurations(Duration, Duration) : Duration = host sub;
  operation MulScalar(Scalar, Scalar) : Scalar = host mul;
  operation Scale(Scalar, Duration) : Duration = host mul;

  overload Plus = AddScalar | Later | AddDurations;
  overload Minus = SubScalar | Earlier | Between | SubDurations;
  overload Times = MulScalar | Scale;

  builtin value pi : Scalar = host pi;
  builtin value today : Date = host today;

  let Let(Value) infer;
  show Show(Value);

  check Day.Value is iso-date else DATE0001 "not a calendar date";
}
```

## Example: Calls

```ngr
catalog module Calls
{
  type Time;

  operation Duration(Scalar) : Time = host identity;
  operation NegScalar(Scalar) : Scalar = host neg;
  operation NegTime(Time) : Time = host neg;
  operation SumScalar(Scalar, Scalar) : Scalar = host add;
  operation SumTime(Time, Time) : Time = host add;
  operation Zero() : Scalar = host zero;

  builtin function neg = NegScalar | NegTime;
  builtin function sys.neg = NegScalar | NegTime;
  builtin function sum = SumScalar | SumTime;
  builtin function duration = Duration;
  builtin function zero = Zero;
  builtin function calc = Zero | NegScalar | SumScalar;

  let Let(Value) infer;
  let Local(Value) infer;
  show Show(Value);

  call Call0();
  call Call1(Arg);
  call Call2(Left, Right);
  call AnyCall(Arg);
  call FallbackCall(Arg);
}
```

## Members

Each member corresponds to one existing `catalog.json` section, so a catalog
lowers to exactly today's JSON value.

| Member | Meaning | JSON today |
|---|---|---|
| `type T;` | a semantic type | `types` |
| `operation Op(T, …) : R = host h;` | a host operation, bound to handler `h` | `operations` |
| `overload G = Op \| Op …;` | candidates for a `lowers G(...)` group | `overloads` |
| `builtin K n = Op \| Op …;` | a callable built-in of symbol kind `K` | `builtins[K][n]` |
| `builtin K n : T = host h;` | a built-in value of kind `K`, from handler `h` | `values[K][n]` |
| `let Rule(Field) [infer];` | the rule declares a symbol valued by `Field` | `statements` (`kind: let`) |
| `show Rule(Field);` | the rule shows the value of `Field` | `statements` (`kind: show`) |
| `call Rule(Arg, …);` | the rule is a call whose arguments are these fields | `calls` |
| `check Rule.Field is p else CODE "message";` | a source check with predicate `p` | `checks` |

`builtin K` mirrors the grammar's `builtin K { … }` declaration: the grammar says
that a name exists, and the catalog says what it is. A built-in either has
candidates (callable) or a type and handler (a value), never both.

Handlers and predicates are fixed host vocabularies, not catalog code:
operations use `zero identity neg add sub mul div pow iso-date`, values use `pi e
tau today`, and checks use `iso-date`. `host` marks a handler so that later
forms (an operation defined in the catalog) do not collide with it.

## Names

- `catalog module M` qualifies what it declares: `type Date;` is `M.Date`, and
  `operation Later` is `M.Later`.
- A bare type name is the module's own type, else a core type: `Scalar` and
  `Text` are `Core.Scalar` and `Core.Text`. A qualified name (`Portable.Time`)
  names another module's type.
- A bare operation name in `overload` and `builtin` is the module's own.
- Rule names in `let`, `show`, `call` and `check` are grammar rules. A bare
  name is the grammar module of the same name (`Let` in `catalog module Dates`
  is `Dates.Let`), and a qualified name is taken as written. Field names are
  checked against the rule.
- Built-in names are literal and may contain dots (`sys.neg`), as they are in
  the grammar's `builtin function { … }`.

## Order and parity

Operation order fixes operation ids, and type order fixes the generated type
table, so lowering keeps source order: `Core.Scalar` (always, since grammars
type number literals with it), then `Core.Text` if mentioned, then declared
types in order. Every existing catalog already has this order and has a single
module. A project without a catalog is a syntax-only profile. Such projects
(`grammar`, `xml`, `csharp4`, `composition`) need no catalog file, and their
manifests list no `catalogs`. Merging several modules is in "Using other
catalogs".

Migration acceptance: for every language, the new catalog lowers to a JSON
value equal to the old `catalog.json`. Generated tables, memory evidence and
Nitrogen parity are then unchanged by construction.

## Pipeline

- `languages/catalog/Catalog.ngr` defines the language, as
  `languages/grammar/Grammar.ngr` defines `.ngr`. Its compiled frontend sits in
  `nitride-prepare` beside the grammar seed.
- `nitride_prepare::catalog(&Source) -> Result<Value, PrepareError>` parses a
  catalog, resolves names with source spans, and lowers to the existing JSON
  value. Codegen is unchanged. Resolution errors (unknown type, unknown
  operation, duplicate member, field not on rule) carry catalog spans. Codegen's
  remaining checks keep their current messages.
- Manifests (`project.json`, `nitride.json`) list `.ncatalog` files in
  `catalogs`. `nitride_prepare::catalogs(&[Source])` resolves and merges them,
  and every reader of the old `catalog` path (prepare, the CLI build script,
  the LSP registry) calls it.
- The development-only .NET tools take JSON. `nitride catalog <file.ncatalog>`
  prints the lowered JSON, and the parity harness passes that output to them.

## Editor

The LSP serves `.ncatalog` as a built-in language, as it serves `.ngr`: semantic
tokens, outline, folding, diagnostics, hover and formatting. Navigation crosses
files:

- a type, operation or overload name goes to its catalog member;
- `lowers Dates.Plus(...)` in a grammar goes to `overload Plus`, and `lowers
  Dates.Days(...)` goes to `operation Days`;
- a rule or field in `let`, `show`, `call` or `check` goes to the grammar rule;
- `builtin value pi` in a catalog and `builtin value { pi }` in a grammar go to
  each other.

## Phases

1. `Catalog.ngr`, the parser, `using` resolution, merging and lowering, and a
   parity test against every `catalog.json`. Multi-module tests cover lookup,
   ambiguity, merge order and conflicts. Done: each typed language has its
   `.ncatalog` beside `catalog.json`. Fields are not yet checked against
   grammar rules, since lowering does not see the grammar; that check moves to
   phase 2, where both are prepared together. `power` restates `catalog module
   Portable` with its extra operation, because its ids are `Portable.*` and a
   module cannot span files.
2. Convert each language's catalog, switch the manifests and readers, add
   `nitride catalog`, and delete the JSON files and the empty catalogs. Done:
   manifests list `catalogs`, and the old `catalog` key is an error that names
   its replacement. Rule and field names are checked, at their catalog spans,
   just before profile validation. `catalog_hash` hashes the lowered compact
   JSON, which `nitride catalog` prints with sorted keys. The .NET exporter and
   oracles accept `.ncatalog` paths by running `nitride catalog` (`NITRIDE`
   names the executable), so the reference C# files the scripts regenerate list
   overload and built-in entries in sorted order.
3. Editor support: built-in `.ncatalog` language, diagnostics with spans, and
   cross-file navigation.

## Decisions

- Catalog files use the `.ncatalog` extension.
- Handlers keep the `host` keyword: `= host add`.
- Checks use the sentence form: `check Day.Value is iso-date else DATE0001
  "message";`.

- Catalogs can use other catalogs (`using`, below), as grammars do.

## Using other catalogs

A project's semantic contract may span several catalog modules, as its syntax
spans several grammar modules. Catalogs follow the grammar's model: the
manifest lists the files, and `using` only makes names visible.

### Manifest

`catalog` becomes `catalogs`, a list like `grammars`:

```json
{
  "grammars": ["../dates/Dates.ngr", "Rewrites.ngr"],
  "catalogs": ["../dates/Dates.ncatalog", "Rewrites.ncatalog"]
}
```

Each file holds one `catalog module`. The project's catalog is every listed
module, merged into the one JSON value codegen reads today. Ids in that value
are already qualified (`Dates.Later`), so modules never collide on them. An
empty or missing list is a syntax-only profile.

### `using M;`

```ngr
// Illustrative: Rewrites needs no members today.
catalog module Rewrites
{
  using Dates;

  type Week;
  operation Weeks(Duration) : Week = host identity;
  overload Span = Weeks | Between;
}
```

`Duration` and `Between` resolve to `Dates.Duration` and `Dates.Between`, and
`Week` to `Rewrites.Week`.

- `using M;` comes first in the module, and `M` must be a catalog module listed
  in the project. Otherwise: `catalog module 'M' is not in this project`.
- It changes name lookup only. Members are never copied: Dates' operations stay
  `Dates.*` and are lowered once, from Dates.
- Because `using` adds nothing, cycles are harmless, and two modules may use
  each other.

### Name lookup

A bare type, operation or overload name resolves, as grammar functions do, to:

1. the module's own member;
2. else the member of exactly one `using` module. Two matches are an error:
   `'Duration' is defined in both 'Dates' and 'Portable'`, fixed by qualifying;
3. else, for types, `Core.Scalar` or `Core.Text`.

A qualified name (`Dates.Scale`) needs no `using`, only that the module is in
the project. Rule names in `let`, `show`, `call` and `check` keep their rule.
A bare name is the grammar module named like the catalog (`Soon` in `catalog
module Rewrites` is `Rewrites.Soon`).

### Merge order

Ids and tables depend on order, so the merge is fixed by the manifest:

- `types`: `Core.Scalar`, then `Core.Text` if any module mentions it, then each
  module's types, module by module in manifest order;
- `operations`: each module's operations in source order, module by module.
  Appending a module or a member never renumbers earlier operations;
- `overloads`, `statements`, `calls` and `checks`: the union of every module's
  entries. `builtins` and `values` are the union per kind.

### Conflicts

The merged JSON has one entry per key, so these are errors at the second
declaration, with both spans:

- two modules give the same grammar rule a `let`/`show` policy, `call` or a
  check on the same field;
- two modules declare the same built-in (`builtin function neg`) of a kind.
  Built-in names are global to a kind, as they are in grammars;
- a module declares the same member name twice.

An overload names one group. Adding candidates to another module's group
(`Dates.Plus`) is not supported. A new group (`overload Double`) is declared
instead.

### Migration

Rewrites adds nothing to Dates today, so its manifest lists only
`../dates/Dates.ncatalog`. It produces the same JSON as before and needs no
`Rewrites.ncatalog`. Every other project lists its own single catalog.

### Editor

The LSP finds the opened catalog's project as it does for a grammar, and loads
every listed catalog. `using Dates;` and a `Dates.` qualifier go to `catalog
module Dates`. A bare name that `using` resolves goes to the member in the other
file. Hover and diagnostics name the module a member came from.
