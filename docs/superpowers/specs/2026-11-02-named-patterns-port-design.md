# Port Nitrogen's named patterns and tree context to Nitride

Status: reviewed, awaiting the user's approval, 2026-11-02.

## Intent

Nitrogen PR 92 ("Named patterns, descendant search and inside/parent context",
TovarishN/Nitrogen#92, merged 2026-10-29) extended structural syntax patterns.
Nitride must accept and run that syntax too, so that **one grammar works in
both**: a `.ngr` written for Nitrogen prepares in Nitride and reports the same
diagnostics.

The concrete target is Nitrogen's updated `examples/XmlPatterns/Xml.ngr`.
Nitride's `languages/xml/Xml.ngr` is identical to that file as it was before
PR 92. Only the PR's additions differ: one named pattern, one looking-down
check (XML1002) and one looking-up check (XML1003).

The reference design is Nitrogen's
`docs/superpowers/specs/2026-10-29-named-patterns-and-tree-context-design.md`,
and the user-facing description is the "Named patterns and tree context"
section of Nitrogen's `docs/language-guide.md`. Where this document says
"as Nitrogen", those two documents and Nitrogen's implementation are the
authority. Where they conflict with this document, raise it with the user.

## Goals

1. **Grammar language.** Nitride's grammar language
   (`languages/grammar/Grammar.ngr`, then the seed regenerated with
   `cargo run -p nitride-prepare --example bootstrap`) accepts everything PR 92
   added, spelled as in Nitrogen's `Nitrogen.ngr`:
   - A module member
     `pattern Name : Category = "|"? Alternative ("|" Alternative)* ";"`, where
     an alternative is `Captures? <[ … ]>`.
   - In a capture list, `name: ..Pattern` (descendant search) and
     `name: Pattern` (a capture whose "category" is a named pattern).
   - On a `match` check, an optional context clause after the pattern:
     `("inside" | "parent") Category? Captures? <[ … ]>`.
   - A `match` check message may be a plain string (`"…"`) as well as `$"…"`.
     XML1003 in the target example uses a plain string.
2. **Semantics as Nitrogen.** Matching, reporting and limits behave as
   Nitrogen's design specifies:
   - Alternatives are tried in order, and the first success is committed (no
     retrying a named pattern's other alternatives).
   - Captures inside an alternative are private to it.
   - Patterns may recurse. A cycle of non-consuming references (a reference
     that is a whole alternative, or a descendant search) is rejected when the
     language is prepared.
   - **Looking down:** `..Pattern` searches the hole node and its descendants
     in preorder, document order. Each candidate is a backtracking choice. It
     may appear once per pattern and captures one node, not a list. The
     diagnostic reports once, on the node being checked.
   - **Looking up:** `inside` tries ancestors nearest first, and `parent` only
     the semantic parent (list and group wrapper nodes skipped). The context
     category defaults to the checked rule's. The nearest matching ancestor
     supplies the context captures, whose names must differ from the main
     pattern's. Ancestors whose syntax has errors count as not matching. The
     diagnostic reports at the checked node, once per inner item.
   - One work budget covers a check: its own match, every alternative tried,
     every descendant and ancestor visited. Running out is a check failure
     reported exactly as Nitrogen reports it: in Nitrogen the matcher throws
     `NQP0006: …` and `FileSemantics.RunChecks` publishes **NS0002**
     ("check XML1002 failed: …NQP0006…") on the checked node. Nitride publishes
     the same code and message. Running out inside a `syntax function` arm does
     whatever Nitrogen does there. (My call, following "as Nitrogen".)
3. **Everywhere patterns are used.** Named patterns work in `match` checks and
   in `syntax function` match arms. They behave the same in both, including
   the `..$x` spreads their alternatives contain (`Secret` uses them).
4. **Same diagnostic codes.** Problems are reported with Nitrogen's codes and
   meanings, so an error reads the same in both tools:
   - NGR0163: unknown named pattern
   - NGR0164: duplicate named pattern
   - NGR0165: context capture name collides with a pattern capture
   - NGR0166: invalid pattern capture, as Nitrogen defines it
   - NQP0007: not a named pattern of this language
   - NQP0008: non-consuming reference cycle, naming the cycle
   - NQP0009: a descendant capture repeated, or used on a list
   - NS0002 with `NQP0006` in its message: the work limit was exceeded (see
     goal 2)
5. **Editor.** For named patterns, the language server gives go-to-definition,
   find-references, rename (keeping qualifiers), hover (category and
   alternatives) and completion. Completion offers patterns after `..` and in
   capture declarations. The VS Code TextMate grammar
   (`editors/vscode/syntaxes/ngr.tmLanguage.json`) highlights `pattern`,
   `..Name`, `inside` and `parent`.
6. **Example and docs.** `languages/xml/Xml.ngr` and `languages/xml/sample.xml`
   become Nitrogen's updated versions. Nitride's documentation of `match`
   checks describes the new forms.

## Non-goals

- Nitrogen's C# surface: `QuoteHole.Pattern`, `QuoteHole.PatternList`,
  `QuoteHole.DescendantOf`, `Language.NamedPattern` and
  `QuotePattern.FindAll`. Nitride has no C# layer, and nothing in Nitride needs
  a whole-tree search API yet.
- The later Nitrogen slices: token captures, guards, optional captures and
  binding-aware comparisons.
- Inline alternation or negation inside a quote.

## Anti-goals

- **Changing what existing grammars do.** Every existing `match` check,
  `check` comparison and `syntax function` arm keeps its current results. That
  includes the arm matcher's category rule (a rule or its `owner`) and its
  structural equality for repeated captures, and the check matcher's alias
  rule (`derives`) and text equality.
- **Unbounded work.** No pattern may loop forever, overflow the stack on deep
  input, or run without a budget. Today's recursive matchers have no budget.
  The new search and recursion must not inherit that.
- **Hidden search.** Descendant search happens only where `..Pattern` asks for
  it. Ancestor search happens only where `inside`/`parent` asks for it.
- **Divergence from Nitrogen.** A grammar that prepares in one tool must not
  quietly behave differently in the other.

## Constraints and the current state

- **Two matchers today.**
  - `crates/nitride-core/src/checks.rs` runs `match` checks. It is
    recursive with backtracking, supports `..$x` spreads, accepts a node whose
    rule `derives` from the hole's category through aliases, and joins
    repeated captures by token text.
  - `match_pattern` in `crates/nitride-core/src/semantic.rs` runs function
    arms. It is exact, with no spreads; it accepts the hole's category or a
    rule whose `owner` it is, and joins repeated captures with `same_syntax`.

  Neither has a budget. The builder may unify them or keep them apart, within
  the anti-goal above.
- **Data path.** Grammar text is lowered in `crates/nitride-prepare/src/parser.rs`
  and normalized in `normalize.rs` into prepared JSON. It is then either
  generated into a static `Language` (`crates/nitride-codegen/src/lib.rs`) or
  loaded at run time (`crates/nitride-codegen/src/runtime.rs`, which also
  calls `validate_rule_checks`). Named patterns and context clauses need a
  place in all of these, and in `nitride_core::Language`.
- **Prepared JSON is Nitride's own.** Nitrogen has no prepared-JSON format of
  its own. `scripts/prepare_parity.py` compares Nitride's preparation with
  Nitride's reference tool (`tools/Nitride.Reference/Exporter.cs`), which
  reads Nitrogen's grammar model and emits no checks at all. Nitride already
  prepares XML's `check`/`match` blocks into a `checks` shape of its own
  (since f761201), so the XML comparison already differs there. The builder
  designs the JSON for named patterns and context clauses in the same spirit
  as the existing `checks` shape. Parity with Nitrogen is proven by the
  diagnostics both tools report, not by JSON equality. Extending the
  reference exporter is out of scope. (My call: the user's goal is that one
  grammar works in both, which diagnostics show directly.)
- **Diagnostic codes.** Nitride's preparation errors (`PrepareError`) mostly
  carry no code today. Carrying Nitrogen's codes may need a code on
  preparation errors and on the problems the language server publishes.
- **Self-hosting.** The grammar language is prepared from
  `languages/grammar/Grammar.ngr` into the checked-in `grammar_seed.rs`.
  `crates/nitride-prepare/tests/bootstrap.rs` checks that the seed reproduces
  itself, so a change to `Grammar.ngr` goes with a regenerated seed.
- **Editor.** Grammar navigation lives in `crates/nitride-lsp/src/grammar_index.rs`.
  Its `GrammarReference`/`RuleDef` kinds come from `nitride_prepare`, and it
  already provides definition, references, rename, hover and completion for
  rules.

## Done when

- `languages/xml/Xml.ngr` equals Nitrogen's updated example and prepares.
- On Nitrogen's updated `sample.xml`, Nitride reports what Nitrogen's PR
  reports (`nitrogen check examples/XmlPatterns/sample.xml` at or after PR 92):
  XML1001, XML1002 once (on the vault), and XML1003 twice (one per
  `<api-key>` in the search service, at both depths), with the same codes,
  severities, messages and spans. That's in the CLI and as editor diagnostics.
  A test pins this expected list.
- `scripts/prepare_parity.py` behaves for `languages/xml` as it does today:
  this work adds no new difference beyond the `checks` data Nitride already
  emits.
- The acceptance evidence in Nitrogen's design applies, minus the C# and
  `FindAll` items:
  - named patterns with several alternatives used from a check and a function
    arm, with fall-through and commit;
  - private captures don't leak and roll back;
  - recursion at several depths, and a rejected non-consuming cycle;
  - descendant hits at depth 0, at depth > 1 and none, moving on to the next
    candidate, and backtracking from an enclosing spread;
  - `inside` versus `parent`, with nearest-ancestor captures in the message;
  - deep or adversarial input hits the budget instead of hanging or
    overflowing the stack.
- Each new diagnostic code has a test.
- Rename, references, definition, hover and completion work for `Secret` in
  the XML grammar.
- Existing tests pass unchanged.

## Left to the builder

- Matcher architecture: unify the two matchers or not, the representation of
  compiled named patterns, and how budget and stack safety are achieved.
- How codes are attached to preparation errors and published problems.
- The default budget size, comparable to Nitrogen's `QuoteMatchBudget`.
- Test layout, and the wording of Nitride's documentation.
