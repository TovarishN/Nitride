# Host binding (design draft)

Status: draft for review. Adds interfaces to `.ncatalog`, a binding language
(`.nbind`) that maps an interface onto a concrete implementation, and
conformance examples that check every implementation against its interface.
Phases 1 and 2 are implemented (see Phases); the rest is not.

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
3. The value model: `i32`, `i64`, `bool`, `text`, then records and lists,
   with `represented`. Dates moves `Date` to `represented i32`.
4. `.nbind` with `wasm component` bindings and WIT output. With `f64` values a
   `rust crate` binding adds nothing over implementing the generated trait,
   so `.nbind` starts here, where it carries the type mapping.
5. Effects: `effect` on operations, and how a host grants them.
6. `process` bindings, and drafting bindings from library metadata.

## Open questions

- Whether an interface may span modules, or `using` an interface from another
  catalog is enough.
- Whether a binding may implement several interfaces, or several bindings of
  one interface may coexist in a project (selected by the host at run time).
- How effects are granted: by the manifest, by the host application, or both.
- Whether `example` may use grammar syntax (`2024-01-01 + 3 days`) rather than
  catalog calls. Grammar syntax ties the interface to one language.
- Versioning: whether a `version 2` interface may declare which `version 1`
  bindings still conform.
