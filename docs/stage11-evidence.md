# Quote rewrites and syntax-function helpers

Stage eleven admits DateCalc's rewrite forms: inline `rewrites <[ ... ]>`,
`syntax function` quote helpers and ordered `match` arms. The fixture
[`Rewrites.ngr`](../languages/rewrites/Rewrites.ngr) composes over Dates and
adds `fortnights`, `swapped`, `~n` (today plus n days), `twice(x)`, `pad(x)` and
`wrap(x)`. Native builds use no .NET component.

## Observed Nitrogen semantics

Live probes and the frozen corpus fixed the behavior before native work began:

- Quote text is the raw text between `<[` and the first `]>`, trimmed before
  parsing. Template origins are relative to the trimmed text: `pad`'s
  `<[    $Inner  +  0   ]>` reports the whole template at `[0,12]`.
- An expansion root's origins are the use span first, then the template root's
  `nitrogen:rewrite` span. Nodes created in a template carry only template
  spans. Holes keep their caller origins. A single-hole template is the
  caller's HIR, unchanged.
- Template names resolve only against built-ins: `let today = 2026-01-01; ~1;`
  still adds to the built-in `today`. Template references do not appear in the
  document's bindings.
- Nitrogen evaluates HIR trees. `twice(2 days)` traces `Days` twice, and the
  discarded caller addition in `count + 2 swapped` never runs.
- Match arms use original syntax: `(count + 2) swapped` and
  `3 fortnights swapped` take the fallback.
- Errors in an expansion are reported at the use with an
  `in the expansion of Name:` prefix. They sort with caller diagnostics by
  source start, inner first: `twice(2026-02-30)` reports the expansion error at
  0 before the caller's DATE0001 at 6. `2026-02-30 + twice(1)` reports the
  check first.

## Native design

- **Frontend.** `Grammar.ngr` gains `Helper`, `MatchBody`, arms, captures, a
  lexical `QuoteText` token and a `Rewrites` clause. Helpers normalize to a
  top-level `helpers` array, and rewrite clauses carry `template` or a resolved
  helper `target` with field `args`. The live exporter emits the same model.
  Both validate helpers in syntax mode and then omit them there. The seed was
  regenerated, and `bootstrap -- --check` holds.
- **Codegen.** `Language.helpers` holds static helpers; inline templates become
  helpers named after their rule, with the rule's syntax fields as holes.
  `Action::Rewrite` is admitted only on extensible alternatives. Helper
  parameter and result categories must match the use's fields and owner
  exactly. List parameters, plain-rule rewrites, syntax-only use and template
  profiles are rejected.
- **Template parsing.** `parse_template` runs the generated parser in template
  mode. A `$Name` hole is accepted where a rule of its category is expected,
  including as a Pratt operand, and becomes one childless node. Every template
  of every helper is parsed and validated on each compile: patterns must use
  every capture, results may use parameters and captures, templates may not
  declare names or statements, and template names must be built-ins.
- **Lowering.** `semantic.rs` lowers the document and template trees with the
  same code. A rewrite use selects an arm by structural matching (rules,
  labels, children and lexical text; repeated captures must be equal). It
  lowers the template with holes bound to the caller's HIR, then fixes origins.
  Expansions nested inside templates use the outermost use span for
  diagnostics. A rewrite reaching itself is a type error at the use; depth and
  work are bounded.
- **Re-emission.** Rewrite profiles lower into provisional HIR, then emit only
  trees reachable from statements, in evaluation order, copying shared
  subtrees. This reproduces Nitrogen's repeated computation and drops
  discarded syntax. Emission is bounded by `Limits::work` nodes; nested
  repeated holes can grow exponentially, and the test bounds 2^16 leaves.
  Older profiles keep direct emission.

## Validation

- RED first: the rewrite fixture failed native preparation ("unsupported
  semantic clause") before codegen admitted `Rewrites`. One core test
  expectation was wrong and was corrected: `$Inner` does parse as a `Program`,
  because a Program contains an Expr.
- 120 Rust tests, formatting, strict clippy and `bootstrap -- --check` pass.
  New tests cover the template parser (holes, trimming, categories), hole
  expansion and origins, self-reaching rewrites, invalid unused templates,
  reachable-only emission, copied repeated holes, a work bound on nested
  repeated holes, 12 malformed helper/rewrite catalog variants, and frozen
  parity.
- 29 frozen Rewrites observations from actual Nitrogen match native output:
  syntax, bindings, HIR with every origin and its path, values, traces and
  diagnostics. Expected values were computed independently before freezing.
  Another 64 generated programs match live Nitrogen. A `wrap(x)` alternative
  (`<[ ($Inner) ]>`) confirms that a caller node reached through a wrapper gets
  a fresh node carrying the use origin.
- `scripts/prepare_parity.py` compares 88 prepared profiles against the live
  exporter. These include Rewrites, four helper and template variants
  (qualified call, unspaced quote, capture-free arm, multi-line quote) and the
  syntax-only composition. 20 invalid grammars, including list parameters,
  missing or misplaced fallbacks and token categories, are rejected by both.
- All earlier frozen and generated comparisons still pass: 249 frozen and 512
  generated observations in total.
- Nitrogen's workspace checker, given an explicit manifest, passes the Rewrites
  sample. A temporary `2026-10-05 + 1 days swapped;` reports
  `DATE0002: in the expansion of Swapped: no exact overload` and is removed. It
  also warns (NGR0127) that rewrite alternatives define no `Type` property in
  the reference composition; their types come from expansion.

## Footprint

The macOS arm64 release CLI is 918,416 bytes, +83,200 from stage ten; it now
embeds the Rewrites tables, template parsing and expansion. The profiler is
569,760 bytes. The core still has no third-party dependencies. For older
workloads (`stage11-{namespace,inference,calls,functions,dates}-memory.json`),
allocation counts and retained bytes match stage ten. HIR-phase peaks fell
5–24% because the binding environment is now freed before lowering. The new
`stage11-rewrites-memory.json` workload at 1000 pairs allocates 41,257 times in
the HIR phase and retains 1,204,224 bytes. Template parsing on every compile,
provisional lowering and re-emission all cost memory; no reduction is claimed.
A clean source-only copy builds offline with no .NET on PATH, passes the
bootstrap check, prepares Rewrites and runs its sample.

## Limits

No list parameters or `..$` splices, grammar functions, C# quotation APIs,
`in` property forwarding, plain-rule (non-alternative) rewrites or templates
naming non-built-in symbols. Expansion-internal diagnostics keep the outermost
use span. Nested and self-reaching rewrites inside templates are covered by
native tests only, not compared with Nitrogen. Re-emission can grow
exponentially with nested repeated holes, as Nitrogen's HIR does; it is bounded
by `Limits::work`. Self-review only; no independent reviewer.
