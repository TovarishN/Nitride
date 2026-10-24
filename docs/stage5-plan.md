# Multiple symbol namespaces

Continue the Rust rewrite inline with the Nitrogen and implementation skills.
Existing deployment, ownership and exact-host-preflight contracts remain binding
(docs/design.md). No commits, pushes or original Nitrogen product changes.

## Contract

Support multiple declared symbol kinds without erasing their identities. A
declaration introduces a name in exactly one kind; a reference lists one or more
kinds. Nitrogen's actual Project.Lookup searches reference kinds in authored
order and returns the latest visible sequential declaration of the first matching
kind. This is priority, not ambiguity across kinds. Shadowing stays per kind;
initializers see prior declarations and never the declaration being introduced.

Add optional `symbol_kinds` inventory to normalized format v1. New native/.NET
preparations emit deterministic unique module-order inventories. Codegen validates
clause kinds against it. Legacy v1 without the inventory keeps its restricted
single-kind admission for compatibility. No module qualification is invented:
Nitrogen's symbol-kind strings are the identities, with frontend import visibility.

Action::Declare gains kind; Action::Reference gains ordered kinds. Binding records
retain the selected kind; HIR symbol IDs and execution storage stay declaration
identities. Core remains dependency-free and uses no global caches.

## Tasks

1. [x] Freeze an actual Nitrogen reference using a declarative Namespaces fixture
   (value/rate namespaces, annotated sequential declarations, ordered references,
   nominal types and arithmetic). Compare syntax, selected binding kind, source
   locations, HIR, values and host traces; include no-match and forward failures.
2. [x] Add failing native/codegen/preparer tests, then implement metadata, generator
   actions and kind-keyed binding. Remove current single-kind preparation gates
   only when equivalent kind validation is present. Keep legacy data strict.
3. [x] Register the language in Cargo/CLI and nitrogen.json, generate editor catalog
   metadata and check actual workspace loading. Add frozen/live parity scripts.
4. [x] Run complete Rust/exporter/preparation/runtime gates, release examples and a
   native-only build. Record binary/allocation impact, get one final independent
   review and repair material findings with failing regressions first.

This provides a prerequisite for DateCalc, not DateCalc compatibility. Inferred
declarations, lexical scopes, imports/exports of document symbols, built-ins,
function invocation, overload selection and C# helper translation remain deferred.

Pre-flight: native/.NET exporters share inventory ordering; codegen consumes the
inventory and emits kind-bearing actions; binding consumes those actions and
produces kind-qualified resolution evidence while execution still uses unique
declaration IDs. New observations add kind only for this fixture, leaving existing
frozen output schemas unchanged.
