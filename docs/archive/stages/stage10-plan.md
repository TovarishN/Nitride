# Built-in values, text literals and source checks

The user chose this milestone after stage nine merged: the next bounded step
toward DateCalc. Work on codex/builtin-values. Ordinary Rust targets, no .NET
native build/runtime requirement, zero third-party core dependencies.

## Contract

Admit a DateCalc-shaped `Dates` profile with three new capabilities:

- `builtin value { ... }` names typed by `catalog.values[kind][name]`. References
  try every visible local kind first, then built-in values in authored kind
  order, as Nitrogen Project.Lookup does. HIR keeps a built-in reference and
  never fabricates a declaration. The host supplies each value by exact
  (kind, name, type); preflight rejects missing, duplicate or mistyped values.
  Each value is read at most once per execution, only after preflight, matching
  DateCalc's per-run evaluation context. Reads are not operation trace entries.
- `lowers Op(text Field)` passes the field's exact source spelling as a
  `Core.Text` HIR argument. Text is not a general value type here: text
  arguments only feed operations bound to text host handlers, and no
  operation may return `Core.Text`.
- `catalog.checks[rule]` lists `{code, field, predicate, message}` source checks.
  Predicates are a fixed engine vocabulary (`iso-date`, a proleptic Gregorian
  `yyyy-MM-dd` date), not translated C#. Failed checks are `check` diagnostics
  with their code. They join type diagnostics, primary-selected by source start
  with inner (postorder) facts first, which matches observed Nitrogen ordering.

The pure `Dates.ngr` builds natively. A generated C# reference adapter supplies
built-in types, the predicate and overload selection to actual Nitrogen.
No full DateCalc, built-in value calls, text-valued expressions, `today`
time-zone policy beyond an explicit host date, or C# translation.

## Tasks

- [x] Freeze actual Nitrogen parsing/binding/type/HIR/execution for built-in
  values, shadowing, text literals, invalid dates, overloads and no effects.
- [x] Extend Grammar.ngr/decoder/exporter for text arguments; regenerate the
  seed and verify the fixed point and live export parity.
- [x] Watch codegen/core tests fail; implement value tables, text HIR, checks,
  host values and text handlers.
- [x] Register native build/CLI/CI and Nitrogen editor config; add generated
  live comparisons. Run full gates, clean native build, footprint and a
  self-review; record evidence. Push and hosted CI await the user.

## Review focus

Built-in values must not shadow locals or be visible to unrelated kinds. Text
HIR must not escape into numeric operations or values. Checks must run without
host effects and before any execution. Older profiles' generated tables, HIR
layout and memory metrics must stay unchanged.

## Completion and review

Ruling: built-in value reads are host inputs, not operation trace entries,
because actual Nitrogen supplies them from its evaluation context. Tests count
reads directly: one per value per execution, none on failed preflight.

Ruling: a check whose field is absent fails, as Nitrogen's predicate would on
null text, rather than aborting compilation. The admitted Dates grammar has no
optional checked field, so this path has no fixture.

Ruling: the oracle reports diagnostics whose code is a catalog check code as
`check`; others stay `type`. This is an adapter classification of actual
Nitrogen diagnostics, not a Nitrogen category.

Final self-review against the review focus found the two issues above, both
fixed, with full Rust and live Dates parity rerun. No independent reviewer was
run in this session. Publication waits for the user's go-ahead.
