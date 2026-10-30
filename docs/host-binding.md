# Host binding (design draft)

Status: draft for review. Adds interfaces to `.ncatalog`, a binding language
(`.nbind`) that maps an interface onto a concrete implementation, and
conformance examples that check every implementation against its interface.
Phases 1 to 6 are implemented (see Phases), with what each leaves undone
noted there.

## Why

A catalog operation ends in `= host h`, where `h` comes from a fixed vocabulary
(`zero identity neg add sub mul div pow iso-date`). Before phase 1 the
vocabulary had no single definition. It was listed or interpreted four times:
by catalog preparation (`nitride-prepare/src/catalog.rs`), by codegen
(`nitride-codegen/src/lib.rs`), by the fixture host
(`nitride-cli/src/lib.rs`) and by the editor's value previews and hovers
(`nitride-lsp/src/values.rs`). It is now declared once, in
`nitride_core::host`. A language author still cannot add an operation
without changing Rust.

Two needs are not met at all:

- **An implementation exists, and a DSL should use it.** A library in Rust,
  C#, or behind a service already does the work. The DSL needs a declared,
  checked way to call it, including how its types and failures map onto the
  catalog's.
- **A DSL describes a solution, and implementations should follow it.** The
  contract comes first. It should be implementable in any language, and an
  implementation should be registered and checked against the contract
  before any program runs on it.

Both are the same artifact read in two directions: a language-neutral
interface between the DSL and the code.

```
 program ──lowers──▶ interface (catalog) ◀──implements── binding (.nbind)
                     types, operations,                  Rust crate, WASM
                     failures, effects,                  component, process
                     examples, hash
```

In the concept catalog's terms an interface is a capability, a binding is a
realization, and a conformance run is evidence.

## Example: Dates

```ngr
catalog module Dates
{
  type Date = represented i32;
  type Duration = represented f64;

  failure InvalidDate "not a calendar date";

  interface Calendar version 1
  {
    operation ParseDate(Text) : Date fails InvalidDate;
    operation Later(Date, Duration) : Date;
    operation Earlier(Date, Duration) : Date;
    operation Between(Date, Date) : Duration;
    value today : Date effect clock;

    example Later(ParseDate("2024-01-01"), Days(3)) == ParseDate("2024-01-04");
    example Between(ParseDate("2024-03-01"), ParseDate("2024-02-01")) == Days(29);
    example ParseDate("2024-02-30") fails InvalidDate;
  }

  operation Days(Scalar) : Duration = host identity;
  operation AddScalar(Scalar, Scalar) : Scalar = host add;

  overload Plus = AddScalar | Calendar.Later;
  overload Minus = Calendar.Earlier | Calendar.Between;

  builtin value today = Calendar.today;
}
```

A binding for an existing Rust crate:

```ngr
binding Dates.Calendar for rust crate calendar_core
{
  type Date <-> chrono::NaiveDate via days_to_date / date_to_days;
  failure InvalidDate <- calendar_core::ParseError;

  ParseDate = calendar_core::parse_iso;
  Later = calendar_core::add_days;
  Earlier = calendar_core::sub_days;
  Between = calendar_core::days_between;
  today = calendar_core::today;
}
```

## Interfaces

New `.ncatalog` members:

| Member | Meaning |
|---|---|
| `type T = represented R;` | `T` crosses the binding boundary as `R` |
| `failure F "message";` | a named failure an operation may report |
| `interface I version n { … }` | a group of operations a binding implements together |
| `operation Op(T, …) : R [fails F, …] [effect e, …];` | inside an interface: no `= host`, a binding supplies it |
| `value v : T [effect e];` | inside an interface: a built-in value a binding supplies |
| `example E == E;` / `example E fails F;` | a conformance case, written in the catalog's own terms |

- Inside an interface an operation has no body. Outside one, `= host h` keeps
  its meaning, so existing catalogs are unchanged.
- Interface members are named `M.I.Op` (`Dates.Calendar.Later`). Inside the
  module, `Calendar.Later` resolves as `using` does today. `overload` and
  `builtin` may name interface members like any operation.
- `represented` gives the wire representation: `f64`, `i32`, `i64`, `bool`,
  `text`, and later records and lists. A type without it is `f64`, as all
  types are today.
- `fails` lists every failure an operation may report. A binding that reports
  anything else fails the call with a host diagnostic at the operation's span,
  as a nonfinite result does now.
- `effect` names a capability the host must grant (`clock`, `file`, `network`).
  A pure operation names none. Preflight rejects a program that needs an
  effect the host did not grant, so rejected input still has zero handler
  invocations.

### Identity

An interface's identity is `M.I`, its version, and a hash of its lowered
contract: types and representations, operation signatures, failures and
effects, in source order. Examples are not hashed, so adding one does not
invalidate bindings. A binding names the interface and is checked against the
hash. A binding built against another hash is rejected at preflight:
`binding for 'Dates.Calendar' implements contract 3f2a…, the catalog declares 9c1e…`.

## Bindings

A `.nbind` file holds one `binding M.I for <kind> <target> { … }`. The
manifest lists binding files beside catalogs:

```
language Dates
{
  extensions .dates;
  grammar Dates.ngr;
  start Dates.Program;
  catalog Dates.ncatalog;
  binding calendar.nbind;
}
```

Members:

| Member | Meaning |
|---|---|
| `Op = path;` | the operation calls this target symbol |
| `type T <-> X via to / from;` | conversions between `T`'s representation and the target type |
| `failure F <- X;` | target error `X` reports failure `F` |

- Every interface operation and value needs exactly one entry. A missing,
  extra or duplicate entry is an error at the binding's span.
- Without `type … <->`, the target must take and return the representation
  directly.
- The target symbol's signature is checked where the kind allows it: by the
  Rust compiler for `rust`, by the component's WIT for `wasm`, and only at run
  time for `process`.

### Kinds

| Kind | Target | Checked | Runs |
|---|---|---|---|
| `rust crate` | a Rust crate the host links | at host build time | in process, statically |
| `wasm component` | a `.wasm` component file | at load, against WIT | in process, sandboxed |
| `process` | an executable, JSON-RPC over stdio | at first call | out of process |

`rust` is the first kind. Codegen emits an interface as a Rust trait, and a
binding as an implementation of it:

```rust
pub trait Calendar {
    fn parse_date(&self, text: &str) -> Result<i32, CalendarFailure>;
    fn later(&self, date: i32, by: f64) -> Result<i32, CalendarFailure>;
    // …
}
```

`wasm` is the path for "any language". An interface lowers to a WIT world, and
any language with Component Model bindgen implements it. The catalog stays
the semantic layer above WIT: nominal types, failures as names, effects and
examples. WIT is only the wire contract.

`process` covers existing services and languages without a WASM toolchain.

### Direction A: binding an existing library

The binding is written by hand against the library's API. A later tool may
draft one from the library's metadata (rustdoc JSON, .NET reflection,
OpenAPI). It would not draft the interface: the interface is the DSL's
contract, and it is not derived from one implementation.

### Direction B: implementing an interface

`nitride interface <file.ncatalog> --target rust|wit|csharp` prints the
interface for that target. An implementer fills it in and writes a binding
that names it.

## Execution

- `HostBinding { signature, handler }` becomes a lookup by interface and
  operation. Preflight resolves every `Operation` node in the HIR to a bound
  entry and checks the contract hash, effects, and that no operation is
  bound twice.
- The fixed vocabulary is the built-in interface `Core.Host`, with a
  built-in Rust binding (`nitride_core::host`: `OPERATIONS`, `VALUES`,
  `PREDICATES`). Catalog preparation, codegen, the CLI and the LSP all read
  it. Handler names stay the keys, so `= host add` and the catalog JSON are
  unchanged; when `.ncatalog` interfaces land, `Core.Host` is declared in
  catalog syntax and `host add` resolves to `Core.Host.add`.
- Values stay `f64` until representations other than `f64` are admitted (see
  Phases). Then `HirKind`, the evaluator's value array, `HostHandler` and
  preflight move to a `Value` type together.

## Conformance

`nitride conform <project>` runs every `example` of every interface against
every listed binding:

- `E == E`: both sides are evaluated through the binding, and the results are
  compared under the type's representation (exact for integers and text, the
  shared tolerance for `f64`).
- `E fails F`: evaluation reports exactly `F`.
- Examples needing an effect run only when the effect is granted for the run.

A binding is admitted for execution only after its examples pass on the host
that runs it. A result is recorded per binding and contract hash, and is the
evidence a realization in the concept catalog points to. Publishing a binding
does not admit it: the receiving host runs conformance again.

## Editor

`.nbind` is a built-in language, as `.ncatalog` is. An interface member goes to
its binding entries and back. A binding against an unknown interface, a wrong
hash or a missing operation is a diagnostic. Hover on an interface operation
lists its bindings and their last conformance result.

## Phases

1. `Core.Host` as one Rust declaration with its built-in binding, replacing
   the four copies of the handler vocabulary. No new values, no `.nbind` yet.
   Parity: generated tables and every existing fixture are unchanged.
   Done: `nitride_core::host` holds each handler's name, arity or text input,
   run function and hover description. Prepare checks names and lists them
   in its errors from it. Codegen takes arity, text input and predicates
   from it. The CLI's `fixture_bindings_for` and the LSP's inlay values and
   hovers use `host::bindings`, `host::host_values` and `host::describe`. The
   CLI keeps its own `fixture_values_for` loop, because its static languages
   lend names without allocating. A handler a binding cannot find is left
   unbound, so preflight reports it, where before the call failed.
2. `interface`, `failure` and `example` in `.ncatalog`, the Rust trait from
   codegen, and conformance. Done, as a contract slice: `effect`,
   `represented` and `.nbind` are deferred (below). What was built, and where
   it departs from the sections above:
   - Grammar (`languages/catalog/Catalog.ngr`): `failure F "message";` and
     `interface I version n { operation …[fails F, …]; example …; }`.
     Examples are catalog calls with number and string literals, compared
     with `==` or expected to `fails F`.
   - Interface operations are module members like any other: `Calendar.Later`,
     not `Calendar.Gregorian.Later`. Overloads and built-ins name them by
     their bare name. Two interfaces of a module cannot share an operation
     name.
   - Lowering adds `interface` and `fails` to each interface operation (and
     no `handler`), a `failures` section, and an `interfaces` section with
     each interface's version, operations, examples and contract hash.
     Prepare type-checks examples: arity, input types, both sides of `==`
     alike and not text, and a `fails F` some operation in the example
     reports.
   - The contract hash is SHA-256 over the interface's id, version and
     operation signatures with their failures. Examples are not hashed.
   - Codegen stores `M.I@hash` as an interface operation's entry in
     `Language::handlers`, where a host operation has its handler name.
     Preflight requires such an operation to have a contract binding
     (`HostHandler::Contract`) of exactly that contract, and rejects a
     contract binding for a host operation.
   - A contract handler reports a `Failure { id, message }`. Execution turns
     it into an `execution` diagnostic whose `code` is the failure id.
   - `nitride_codegen::interfaces` emits, per interface, a module with
     `CONTRACT`, a failure enum, the trait, `bindings(language, impl)`,
     `EXAMPLES` and `conform(language, impl)`. The binder reports a failure
     the operation does not declare as `host.undeclared`.
     `nitride interface <manifest>` prints this Rust.
   - `nitride_core::host::conform` runs examples through any bindings; the
     host's built-in handlers serve the language's host operations. Results
     agree exactly or within a relative 1e-9.
   - `nitride conform calendar` runs the examples against the CLI's
     `FixtureCalendar`. A Rust binding is compiled into its host, so the CLI
     conforms only what it links; `conform(language, impl)` is how any other
     host checks its own implementation.
   - Fixture: `languages/calendar`, a Dates-shaped language whose calendar
     arithmetic is the `Calendar.Gregorian` interface; `--calendar <source>`
     runs it. Existing catalogs and the .NET oracle are unchanged.
   - Editor: the outline lists interfaces and failures. Navigation and hover
     for failure names and example calls are not done.
3. The value model. Done for scalars and text; records and lists are
   phase 3b. What was built:
   - `type T = represented f64|i64|bool|text;` (`f64` is the default and is
     not recorded). Lowering adds a `representations` section, and each
     interface's contract hash covers the representations of the types its
     signatures use, so changing one invalidates bindings.
   - Runtime values are `nitride_core::Datum` (`Number`, `Integer`,
     `Boolean`, `Text`): the evaluator's values, symbols and
     `ExecutionReport::values`, contract inputs (`Inputs::Values`) and
     results. The HIR is unchanged: literals stay `f64` constants.
   - Built-in handlers, grammar literals and built-in host values are `f64`
     only; codegen rejects them on a represented type. Values of other
     representations therefore come only from interface operations, and the
     generated binder guarantees their kind.
   - Generated traits take and return `i64`, `bool`, `&str` / `String` and
     `f64` by representation.
   - Examples gain `true` and `false`. A boolean literal stands for any bool
     type and a string literal for any text type (or `Core.Text` source
     spelling); number literals stay `Core.Scalar`, so nominal typing holds.
     Conformance compares integers exactly, and a number with an integer by
     value.
   - The CLI prints integers, booleans and text as JSON numbers, booleans and
     strings, and the editor's value hints show them. Existing languages
     only produce `Number`, so their output and the .NET oracle comparisons
     are unchanged.
   - Calendar: `Date` is `represented i64`, with `DayOfWeek(Date) : Weekday`
     (text), `IsLeapYear(Date) : LeapYear` (bool), a `PartialDay` failure and
     ten examples; the interface is `version 2`. Dates is unchanged.
   - Not measured: values are now 24 bytes rather than 8, and arguments are
     cloned per call. The memory evidence in `docs/` predates this.
3b. Records and lists. Done, in catalogs only; grammars reach them through
   ordinary `lowers` of catalog operations, so `.ngr` and the .NET exporter
   are unchanged. What was built:
   - `type Span = record { start: Date; end: Date; };` and
     `type Dates = list of Date;`. Fields and elements may be any type but
     `Text`, records and lists included; they resolve once every type is
     known. The `representations` section records
     `{"record":[[field, type], …]}` and `{"list":type}`, and a contract hash
     covers the shapes inside the types its signatures use.
   - Engine operations, checked against the types they work on:
     `= record` (inputs are the result's fields in order), `= field f`,
     `= list` (inputs are elements), `= length` (list to `Scalar`) and
     `= item` (list and `Scalar` index to element). Codegen stores a field
     as `field:N`; `nitride_core::host::structure` implements them as
     `HostHandler::Values`, and a bad index is an execution error.
   - `Datum::Record` and `Datum::List` hold `Arc<[Datum]>`. The CLI prints
     both as JSON arrays (records have no field names at run time).
   - Generated interfaces declare a struct per record type, use `Vec<T>` for
     lists, and convert through per-type `get_`/`put_` functions.
   - Calendar: `Span`, `Dates`, the engine operations, and
     `Mondays(Span) : Dates` with three more examples (interface
     `version 3`); `span(…)`, `start(…)`, `end(…)`, `mondays(…)`,
     `count(…)` and `nth(…, i)` in the grammar.
4. `.nbind` with `wasm component` bindings and WIT output. Done without a
   runtime: this machine has no wasm32 target or wasm-tools to build a test
   component, so loading components (wasmtime) is phase 4b. What was built:
   - `nitride interface --target wit <manifest>` (and
     `nitride_codegen::wit`) prints, per interface, a WIT package
     `nitride:<module>@<version>.0.0` with the interface and a world
     `implementation` that exports it. Types map to `f64`, `s64`, `bool`,
     `string`, `record` and `list<…>`, nominal types as aliases, failures as
     an enum, and operations return `result<T, I-failure>` when the interface
     declares failures. Names are kebab-case, keywords `%`-escaped
     (`nitride_codegen::wit_name`). The WIT is checked by tests against
     expected text only; no WIT parser has validated it.
   - `languages/binding/Binding.ngr` defines `.nbind`, parsed natively like
     catalogs (seed `binding_seed.rs`, bootstrapped):
     `binding M.I version n for wasm component "path" { Op = wit-name; }` or
     `…;` with no renames. Prepare checks the interface and its version,
     that renamed operations belong to it, and that an interface is bound
     once; it lowers each binding to its contract, target, component path
     (as written, relative to the `.nbind`) and every operation's export
     name.
   - Manifests list bindings: `binding x.nbind;` in `.nproj` and
     `"bindings"` in `project.json`. Prepared data gains `bindings` only when
     a project has some.
   - Calendar binds `Calendar.Gregorian` to `gregorian.wasm`, renaming
     `ParseDate` to `parse-iso`. The component does not exist.
   - Not done: the editor does not serve `.nbind`, and `rust crate` targets
     are not offered (implementing the generated trait already covers them).
4b. Component runtime. Done:
   - `crates/nitride-wasm` (wasmtime 50, component model, no WASI):
     `bindings(language, prepared, binding, component)` instantiates the
     component, finds the interface export (`binding.export`, e.g.
     `nitride:calendar/gregorian@3.0.0`) and each operation's function, checks
     parameter and result counts, and returns contract bindings. Values cross
     as `f64`, `s64`, `bool`, `string`, records and lists; a `result` error
     case maps back to the catalog failure and its message; a trap or a
     malformed value is the failure `host.component`. Calls share one store
     behind a mutex.
   - Prepared bindings gain `export`, and their `component` path is made
     relative to the manifest's folder.
   - `nitride interface --target wit-binding` prints the WIT a binding's
     component must export (the interface with the binding's renames).
   - `nitride run <manifest> <source>` runs any project, prepared at run time,
     with the engine's handlers and its bindings' components;
     `nitride conform <manifest>` runs each bound interface's examples against
     its component.
   - Calendar's component: `languages/calendar/component`, a crate that
     depends only on wit-bindgen and the checked-in WIT
     (`component/wit/gregorian.wit`, kept current by a test). `build.sh`
     builds it for `wasm32-unknown-unknown` and wraps it with
     `wasm-tools component new` into `languages/calendar/gregorian.wasm`
     (31 KB, checked in, so tests need no wasm toolchain). It conforms to all
     13 examples and runs the sample with the same values and trace as the
     Rust implementation.
   - Generated WIT is parsed by `wasm-tools` in a test when it is installed.
   - Editor: the LSP serves `.nbind` (diagnostics against the listing
     language's catalogs, a warning for a binding no language lists, outline,
     syntax colouring), and the VS Code extension registers the language.
5. Effects. Done:
   - Catalogs declare `effect Clock "reads the current date";` (module
     members, like failures), and interface operations list theirs:
     `operation Today() : Date effect Clock;`. Lowering adds an `effects`
     section and `effects` on each operation that has any; the contract hash
     covers them, and an operation without effects hashes as before.
   - The host grants effects when it builds bindings:
     `nitride_core::host::grant(bindings, needs, granted)` replaces the
     binding of an operation that needs an ungranted effect with
     `HostHandler::Denied`. Preflight rejects a program that uses one ("'…'
     needs Calendar.Clock, which the host does not grant"), so it runs no
     handler at all.
   - Generated interfaces carry `EFFECTS` and `needs(op)`, and
     `conformance(language, impl, granted)` reports failed and skipped
     examples; examples that need an ungranted effect are skipped
     (`host::conformance`). WIT documents a function's effects.
   - `--grant <effect>` grants to `nitride run`, `nitride conform` and
     `--calendar`; nothing is granted by default.
   - Calendar: `effect Clock` and `interface Clock version 1` with
     `Today() : Date effect Clock`, `today` in the grammar, implemented by the
     Rust fixture (no component: `wasm32-unknown-unknown` has no clock).
   - Effects apply to interface operations only. The built-in host value
     `today` reads the clock without an effect.
6. Process bindings, and drafting bindings from component metadata. Done:
   - `binding M.I version n for process "program" "arg"…;` binds an
     interface to an executable that answers JSON-RPC 2.0, one object per
     line on stdin and stdout (`crates/nitride-process`; protocol in its
     crate docs). Methods are the export names; values are JSON by
     representation (records as objects by field name, lists as arrays); a
     declared failure is an error with `data.failure` set to its WIT name.
     The program runs in its binding file's folder; one without a `/` is found
     on PATH. Prepared bindings record `program`, `args` and `directory`.
   - The process starts when the host binds it, even if every operation is
     then denied for want of an effect; denied operations are never called.
   - `nitride run` and `nitride conform` bind processes as they do
     components; conformance reports name the `implementation`.
   - A component target may name its instance:
     `for wasm component "x.wasm" export "acme:cal/days@1.2.0"`, for a
     component not built from Nitride's WIT.
   - `nitride binding draft <manifest> <interface> <component.wasm>`
     (`nitride_wasm::draft`) reads the component's exported instance and
     functions with wasmtime and drafts a `.nbind`: an operation goes to the
     export of its own WIT name, else to the one unused export whose
     parameter and result types fit, else stays a comment to resolve. It
     names the instance when it is not Nitride's. On Calendar's component it
     recovers `ParseDate = parse-iso` from the types alone.
   - Calendar binds `Calendar.Clock` to `clock.py` (`clock.nbind`). Tests
     that start Python skip themselves where `python3` is missing.
   - Not done: drafting from library metadata other than components
     (rustdoc JSON, .NET reflection, OpenAPI). Components are the one format
     whose types Nitride already maps.

## Open questions

- Whether an interface may span modules, or `using` an interface from another
  catalog is enough.
- Whether a binding may implement several interfaces, or several bindings of
  one interface may coexist in a project (selected by the host at run time).
- How effects are granted: today by the host application only (`--grant`
  in the CLI); whether a manifest may also grant them.
- Examples run against one interface's binding (plus the engine's
  handlers), so an example cannot mix operations of two interfaces.
- Whether `example` may use grammar syntax (`2024-01-01 + 3 days`) rather than
  catalog calls. Grammar syntax ties the interface to one language.
- Versioning: whether a `version 2` interface may declare which `version 1`
  bindings still conform.
