# Quote rewrites and syntax-function helpers

The user asked to continue with the next step after stage ten's PR passed CI.
DateCalc's remaining syntax feature is rewrites: `rewrites <[ ... ]>`,
`syntax function` quote helpers and ordered `match` arms (`fortnights`,
`swapped`). Work on codex/quote-rewrites, stacked on codex/builtin-values.
Ordinary Rust targets, no .NET build/runtime requirement, zero third-party core
dependencies.

## Contract

Admit, in typed profiles:

- `syntax function Name(P: Category, ...) : Category = <[ template ]>;` with
  scalar syntax parameters. A `match P { | (C: Category, ...) <[ pattern ]> =>
  <[ result ]>; ... | _ => <[ fallback ]>; }` body has ordered arms and exactly
  one final fallback.
- `rewrites <[ template ]>` (holes are the use's fields) and
  `rewrites Helper(Field, ...)` on syntax rules and extensible alternatives,
  replacing `lowers`.

Semantics follow observed Nitrogen behavior:

- Templates are the raw text between `<[` and the first `]>`, trimmed. They are
  parsed by the language's own generated parser as the declared category, with
  `$Name` holes accepted where a rule of the hole's category is expected. A
  hole is one node.
- A use's value is its template's typed HIR with holes replaced by the HIR of
  the caller's syntax nodes. The root keeps the use origin first, then the
  template root's `nitrogen:rewrite` origin. Template nodes carry
  template-relative origins, and holes keep their caller origins. A template
  that is a single hole is the caller's HIR unchanged.
- Names inside templates resolve only against built-ins. Templates may not
  declare names. Caller nodes keep their own bindings.
- Match arms compare the original caller syntax structurally before any
  expansion: same rule, labels and children, exact token text, trivia
  ignored, parentheses kept. Repeated captures must be structurally equal.
- Each reachable hole use is its own computation: a repeated hole repeats
  effects, and an unused caller node runs nothing (Nitrogen evaluates HIR
  trees). Every caller node is still bound, type-checked and checked.
- Errors inside an expansion report at the use span with an
  `in the expansion of Name:` prefix. They are ordered with caller diagnostics
  by source start, inner first. A rewrite reaching itself is an error at the
  use.
- Every template is validated on each compile, including unused helpers and
  unselected arms. Invalid templates are grammar errors.

Not admitted: list parameters and `..$` splices, grammar functions (non-quote
bodies), C# quotation APIs, `in` property forwarding, and template-authored
declarations.

## Tasks

- [x] Freeze actual Nitrogen observations (syntax, bindings, HIR with full
  origins, values, traces, diagnostics) for a Rewrites profile composed over
  Dates.
- [x] Extend Grammar.ngr, decoder and exporter with helpers and rewrite
  clauses; regenerate the seed; check the fixed point and live export parity.
- [x] Watch tests fail; implement helper tables, template-mode parsing,
  matching, expansion, origins and reachable re-emission in core.
- [x] Register CLI/CI/editor config, add generated live comparisons, run full
  gates, clean native build and footprint, self-review, record evidence.

## Review focus

Holes must not resolve caller names inside templates, or template names in
the caller. Discarded caller syntax must not execute. Repeated holes must
repeat effects, exactly as Nitrogen does. Older profiles' HIR emission,
memory metrics and generated tables must stay unchanged. Template parsing and
expansion must be bounded, with no stack overflow on nested rewrites.

## Completion and review

Ruling: rewrite profiles lower into provisional HIR and re-emit reachable
trees, because Nitrogen evaluates HIR trees; older profiles keep direct
emission and unchanged allocation counts.

Ruling: helpers are validated in syntax-only mode and then omitted, on both
the native frontend and the live exporter, as C# semantic blocks are.

Self-review found that a caller node reached through a wrapper template would
have taken the use origin on its shared provisional node. It is now copied,
and the `wrap` cases confirm the behavior against Nitrogen. No independent
reviewer was run. Publication waits for the user's go-ahead.
