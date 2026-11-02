# Named patterns and tree context (Nitrogen PR 92 port) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Nitride accepts and runs Nitrogen PR 92's named patterns, `..Pattern` descendant search and `inside`/`parent` context clauses, so Nitrogen's updated `examples/XmlPatterns/Xml.ngr` prepares and reports the same diagnostics in both tools, with editor support for named patterns.

**Architecture:** The grammar language gains the new syntax. Preparation lowers it into prepared JSON (top-level `patterns`, plus `pattern`/`descendant` on captures and `context` on match checks), which codegen and the runtime loader carry into new `nitride_core` types. A new budgeted matcher in `nitride-core` (`quote_match.rs`) runs named-pattern references, descendant search and ancestor context. `match` checks and `syntax function` arms both use it, each through a "flavor" that keeps its current category and equality rules. The language server indexes named patterns like rules.

**Tech Stack:** Rust (workspace crates `nitride-core`, `nitride-prepare`, `nitride-codegen`, `nitride-lsp`, `nitride-cli`), the self-hosted grammar language (`languages/grammar/Grammar.ngr` → `grammar_seed.rs`), VS Code TextMate JSON.

**Spec:** `docs/superpowers/specs/2026-11-02-named-patterns-port-design.md`. Reference behavior: Nitrogen PR 92 (TovarishN/Nitrogen#92) and its design doc `docs/superpowers/specs/2026-10-29-named-patterns-and-tree-context-design.md`. Read both specs before starting; read Nitrogen's sources (`gh api repos/TovarishN/Nitrogen/contents/<path>`) wherever a task says "as Nitrogen".

## Global Constraints

- Every existing `match` check, `check` comparison and `syntax function` arm keeps its current results. Existing tests pass unchanged.
- Spelling follows Nitrogen's `Nitrogen.ngr` after PR 92:
  - `pattern Name : Category = "|"? Alternative ("|" Alternative)* ";"`, with an alternative being `Captures? <[ … ]>`.
  - A capture parameter is `Name ":" ".."? Category "*"?`.
  - The context clause is `("inside" / "parent") Category? Captures? <[ … ]>`, placed between a `match` check's pattern and `=>`.
- Grammar-check codes and messages copy Nitrogen's:
  - NGR0163: `'{name}' is not a named pattern; a descendant capture needs one`
  - NGR0164: `duplicate rule/helper/pattern '{name}'`
  - NGR0165: `capture '{name}' is already a capture of the matched pattern; rename one`
  - NGR0166: `descendant capture '{name}' finds one node; it cannot be a list` and `descendant capture '{name}' may occur only once in a pattern`
  - NQP0007: `'{name}' is not a named pattern of this language.`
  - NQP0008: `Named patterns refer to each other without consuming syntax: {A -> B -> A}.`
  - NQP0009: Nitrogen's runtime wording for descendant misuse.

  Preparation catches these mistakes first with the NGR codes, as Nitrogen's grammar validator does. `Quote::compile` (Task 3) reports NQP0007 and NQP0009 only for loaded data that preparation did not check.
- Work limit: `MAX_WORK_STEPS = 1_000_000` steps per check (Nitrogen's `QuoteMatchOptions.MaxWorkSteps`).
- Running out publishes code `NS0002` with the message `check {Code} failed: InvalidOperationException: NQP0006: The syntax-pattern match exceeded its work limit.` on the checked node, at the severity Nitrogen gives NS0002 (read `SemanticDiagnostic` and `SemanticCodes` in Nitrogen.Runtime; Task 3 records it in a constant).
- Prepared JSON for a grammar that uses none of the new syntax is byte-for-byte unchanged: new keys appear only when non-empty.
- After any change to `languages/grammar/Grammar.ngr`, regenerate seeds with `cargo run -q -p nitride-prepare --example bootstrap` and keep `cargo test -p nitride-prepare --test bootstrap` green.
- Run `cargo fmt --all` and `cargo clippy --workspace --all-targets` (no warnings) before each commit.

## Review Focus

- **A named pattern whose alternatives use `..$_` spreads, called from a `syntax function` arm.** It must match as it does in a check, even though arm patterns themselves don't support spreads (Task 5 test).
- **Ancestors with broken syntax.** A typo inside `<service>` silences XML1003 for keys under it; it doesn't fail the check or report NS0002 (Task 4 test).
- **Very deep input** (1,000 nested elements) with a recursive pattern and with `..Secret`. It must finish with a result or NS0002, without overflowing the stack (Task 3 test).
- **Editing mid-word in the editor.** `pattern Secr` (an incomplete declaration) and an unknown `..Nope` must give a diagnostic, not a crash or a lost outline (Task 7 test).
- **Rename of a pattern used qualified** (`Xml.Secret`) in another module keeps the qualifier (Task 7 test).

---

### Task 1: Core data model, codegen and runtime round-trip

**Files:**
- Modify: `crates/nitride-core/src/grammar.rs` (`Hole`, `RuleTest::Match`, `Language`; new `NamedPattern`, `PatternAlternative`, `MatchContext`)
- Modify: `crates/nitride-codegen/src/lib.rs` (emit the new fields: `Hole` at ~413, rule checks at ~487, `Language` at ~903)
- Modify: `crates/nitride-codegen/src/runtime.rs` (`hole`, `rule_check`, language assembly at ~345)
- Modify: every hand-written `Language { … }` / `Hole { … }` literal the compiler then flags
- Regenerate: `crates/nitride-prepare/src/{grammar,catalog,binding,project}_seed.rs`, plus any checked-in generated languages that `cargo build` reports
- Test: `crates/nitride-codegen/tests/runtime.rs`

**Interfaces:**
- Produces, in `nitride_core`:
  ```rust
  pub struct Hole<'g> { pub name: &'g str, pub category: u16, pub pattern: Option<u16>, pub descendant: bool }
  pub struct NamedPattern<'g> { pub name: &'g str /* qualified, e.g. "Xml.Secret" */, pub category: u16, pub alternatives: &'g [PatternAlternative<'g>] }
  pub struct PatternAlternative<'g> { pub captures: &'g [Hole<'g>], pub pattern: &'g str }
  pub struct MatchContext<'g> { pub parent_only: bool, pub category: u16, pub captures: &'g [Hole<'g>], pub pattern: &'g str }
  RuleTest::Match { captures: &'g [Hole<'g>], pattern: &'g str, context: Option<MatchContext<'g>> }
  Language { /* existing fields */, pub patterns: &'g [NamedPattern<'g>] }
  ```
  `Hole::pattern` indexes `Language::patterns`; `category` is the pattern's category when `pattern` is set.
- Produces, in prepared JSON (read by codegen):
  - top-level `"patterns": [{"name","category","alternatives":[{"captures":[…],"pattern"}]}]`
  - capture objects may carry `"pattern":"Xml.Secret"` and `"descendant":true`
  - a match check may carry `"context":{"kind":"inside"|"parent","category","captures":[…],"pattern"}`

- [ ] **Step 1: Write the failing test** `named_patterns_round_trip` in `crates/nitride-codegen/tests/runtime.rs`: hand-write prepared JSON for a two-rule grammar with one named pattern of two alternatives, one capture with `pattern` + `descendant`, and a check with an `inside` context. Generate it and load it with `LoadedLanguage::load`. Assert `language.patterns.len() == 1`, the alternatives' pattern texts, the hole's `pattern == Some(0)` and `descendant`, and `context.parent_only == false` with its pattern text.
- [ ] **Step 2: Run it** — `cargo test -p nitride-codegen --test runtime named_patterns_round_trip`, expected to fail to compile (no `patterns` field).
- [ ] **Step 3: Add the types and fields; emit and load them.** Existing JSON without the new keys yields `patterns: &[]`, `pattern: None`, `descendant: false`, `context: None`.
- [ ] **Step 4: Regenerate the seeds and build** — `cargo run -q -p nitride-prepare --example bootstrap && cargo build --workspace`, then `cargo test --workspace`. Everything passes, and the test from step 1 passes.
- [ ] **Step 5: Commit** — `feat: named pattern and match context data model`.

### Task 2: Grammar syntax, lowering, resolution and preparation diagnostics

**Files:**
- Modify: `languages/grammar/Grammar.ngr`
  - `Member` gains `NamedPattern`
  - `Parameter` gains `Descendant:DescendantMarker?`
  - `MatchCheck` gains `Context:MatchContext?` and `Message:(Interpolated / String)`
  - new rules `NamedPattern`, `NamedPatternAlternative`, `DescendantMarker`, `MatchContext`, in Nitrogen's shape
- Regenerate: `crates/nitride-prepare/src/grammar_seed.rs`
- Modify: `crates/nitride-prepare/src/parser.rs`
  - `Module.patterns: Vec<NamedPatternDecl>`
  - `parameters` keeps descendant/list for capture lists (helper parameters still reject lists)
  - `rule_check` emits `context`, and decodes a plain-string message too
- Modify: `crates/nitride-prepare/src/normalize.rs` (resolve pattern and capture categories, write the JSON from Task 1, validate)
- Modify: `crates/nitride-prepare/src/lib.rs` (`PrepareError` gains `pub code: Option<String>`, with all constructors updated; `Source::error_code(span, code, message)` helper)
- Modify: `crates/nitride-lsp/src/analysis.rs` (publish `PrepareError::code` as the problem's `code`)
- Test: `crates/nitride-prepare/tests/named_patterns.rs` (new)

**Interfaces:**
- Consumes: Task 1's JSON shape.
- Produces:
  - `PrepareError { …, pub code: Option<String> }`
  - prepared JSON as in Task 1
  - `GrammarReference` kind `"pattern"` for every pattern name used in a capture (simple or qualified), and a definition kind `"pattern"` for each declaration (consumed by Task 7)

Resolution rules, as Nitrogen:
- A capture category resolves first as a named pattern (simple name in the module and modules it `use`s, or qualified), else as a rule. A pattern name that shadows a category, or is ambiguous, is reported as Nitrogen reports it.
- Each alternative's quote and the context quote must parse in its category; failures are reported like today's "does not parse" check errors.
- Non-consuming reference cycles (an alternative that is exactly one hole referencing a pattern, or any `..` reference) report NQP0008 naming the cycle.

- [ ] **Step 1: Write the failing tests** in `named_patterns.rs`, each preparing a small inline grammar with `nitride_prepare::prepare_sources_tolerant`:
  - `prepares_named_pattern`: Nitrogen's `Secret` over a mini-XML. Assert the `patterns[0]` JSON: name `"Xml.Secret"`, category `"Xml.Element"`, two alternatives, pattern texts verbatim.
  - `descendant_capture_and_context_lower`: the XML1002 and XML1003 checks. Assert the capture JSON `{"name":"secret","category":"Xml.Element","pattern":"Xml.Secret","descendant":true}` and the context JSON `{"kind":"inside","category":"Xml.Paired","captures":[],"pattern":…}`.
  - `plain_string_message_is_accepted`: XML1003 with `"…"` prepares, and its message is kept verbatim.
  - One test per code, asserting `error.code` and the exact message from Global Constraints: `ngr0163_unknown_descendant_pattern`, `ngr0164_duplicate_pattern`, `ngr0165_context_capture_collision`, `ngr0166_descendant_list`, `ngr0166_descendant_twice`, `nqp0008_non_consuming_cycle` (expect `A -> A`), `nqp0008_cycle_through_descendant`.
  - `recursion_that_consumes_is_accepted`: `pattern Nested : Expr = | (n: Nested) <[ ($n) ]> | (l: Number) <[ $l ]>;` prepares.
  - `existing_grammars_prepare_unchanged`: the prepared JSON of `languages/xml/project.json` and `languages/portable/project.json` equals fixtures in `crates/nitride-prepare/tests/fixtures/named_patterns/`. Write those fixtures in step 2, before any code change, from `cargo run -q -p nitride-cli -- prepare <project>`. Task 6 updates the XML fixture deliberately when it replaces `Xml.ngr`.
- [ ] **Step 2: Run them** — `cargo test -p nitride-prepare --test named_patterns`. The unchanged-JSON test passes; the rest fail.
- [ ] **Step 3: Change `Grammar.ngr`, regenerate the seed, and implement lowering, resolution and validation.**
- [ ] **Step 4: Run** `cargo test -p nitride-prepare` and `cargo test -p nitride-lsp`. All pass, including `bootstrap` and the `grammar` profile in `crates/nitride-prepare/tests/nproj_parity.rs`.
- [ ] **Step 5: Commit** — `feat: grammar syntax for named patterns, descendant captures and match context`.

### Task 3: Budgeted matcher with named patterns and descendant search, used by checks

**Files:**
- Create: `crates/nitride-core/src/quote_match.rs`
- Modify: `crates/nitride-core/src/checks.rs` (`Pattern::node`/`list` move into the new matcher; `rule_diagnostics` and `validate_rule_checks` use it)
- Modify: `crates/nitride-core/src/lib.rs` (module and re-exports)
- Test: `crates/nitride-core/tests/quote_match.rs` (new)

**Interfaces:**
- Consumes: Task 1 types; `parse_template` as `checks.rs` uses it today.
- Produces:
  ```rust
  pub const MAX_WORK_STEPS: u32 = 1_000_000;
  pub struct LimitExceeded;
  pub struct Budget { /* remaining */ } // Budget::new(steps: u32), fn step(&mut self) -> Result<(), LimitExceeded>
  #[derive(Clone, Copy)] pub enum Flavor { Check, Arm }
  pub struct Patterns<'a> { /* compiled named patterns */ }
  impl<'a> Patterns<'a> { pub fn compile(language: &'a Language<'a>) -> Result<Self, Diagnostic>; }
  pub struct Quote<'a> { /* compiled quote: template tree + holes */ }
  impl<'a> Quote<'a> {
      pub fn compile(language: &'a Language<'a>, category: u16, captures: &'a [Hole<'a>], text: &str, flavor: Flavor) -> Result<Self, Diagnostic>;
      pub fn matches(&self, patterns: &Patterns<'a>, tree: &dyn SyntaxAccess<'a>, node: u32, budget: &mut Budget, bindings: &mut Vec<(&'a str, u32)>) -> Result<bool, LimitExceeded>;
  }
  ```
  - `Flavor::Check` keeps today's check rules: `..$x` spreads, `derives` alias acceptance, and repeated captures compared by token text.
  - `Flavor::Arm` keeps today's arm rules: no spreads in the arm's own quote, hole category is the rule or its `owner`, and repeated captures compared with `same_syntax`.
  - A named pattern's alternatives always compile as `Flavor::Check`, since they come from Nitrogen-style quotes that may contain spreads.

Semantics, as Nitrogen:
- A reference tries alternatives in order and commits to the first success. Its captures live in a private scope that is discarded afterwards.
- A descendant hole walks the hole node's subtree in preorder, document order. Each candidate is a backtracking choice, resumed if later structure fails.
- Every alternative tried and every descendant visited costs one `step()`.
- Recursion through references or descendants must not grow the native stack with input depth beyond what the depth test allows; the builder may use an explicit work list.

- [ ] **Step 1: Write the failing tests** in `quote_match.rs`, over a tiny test grammar built with `nitride_prepare` in the test (dev-dependency is fine) or a hand-written `Language`:
  - `alternatives_fall_through_and_commit`
  - `private_captures_do_not_leak` and `private_captures_roll_back_on_failure`
  - `recursive_pattern_matches_at_several_depths` (`Nested` over `((1))`, `(((1)))`)
  - `descendant_hits_depth_zero`, `descendant_hits_deep`, `descendant_none`
  - `descendant_moves_to_next_candidate` (first candidate matches `Secret` but fails a sibling constraint; the second succeeds)
  - `spread_backtracks_into_fresh_descendant_search`
  - `budget_exhaustion_returns_limit_exceeded` (`Budget::new(10)` on a wide tree)
  - `deep_input_does_not_overflow`: 1,000 nested elements with `..Secret` and with a recursive pattern; the result is `Ok(_)` or `Err(LimitExceeded)`, never a crash
  - `check_flavor_keeps_text_equality` and `arm_flavor_keeps_same_syntax_equality`: one case each, where the two flavors differ today
  - `compile_rejects_unknown_pattern_nqp0007` and `compile_rejects_repeated_descendant_nqp0009`: hand-built `Language` data that skipped preparation
- [ ] **Step 2: Run them** — `cargo test -p nitride-core --test quote_match`, expected to fail.
- [ ] **Step 3: Implement `quote_match.rs`, and switch `checks.rs` to it with `Flavor::Check` and `Budget::new(MAX_WORK_STEPS)` per check per node.** On `LimitExceeded`, `rule_diagnostics` pushes a `Diagnostic { code: Some("NS0002"), severity: Error, span: node span, message: "check {code} failed: InvalidOperationException: NQP0006: The syntax-pattern match exceeded its work limit." }`.
- [ ] **Step 4: Run** `cargo test -p nitride-core` and `cargo test -p nitride-lsp --test diagnostics`. All pass, including XML1001.
- [ ] **Step 5: Commit** — `feat: budgeted quote matcher with named patterns and descendant search`.

### Task 4: `inside` / `parent` context in checks

**Files:**
- Modify: `crates/nitride-core/src/checks.rs`
- Test: `crates/nitride-core/tests/quote_match.rs`

**Interfaces:**
- Consumes: `Quote`, `Patterns`, `Budget` (Task 3); `MatchContext` (Task 1).
- Produces: none new; behavior only.

Semantics, as Nitrogen's `SemanticsRule.Match`:
- Only when the main pattern matched, walk ancestors from the semantic parent outward (only the first for `parent`), calling `step()` per ancestor.
- The semantic parent skips list, group and alias wrapper nodes, so `parent` of a `<password>` element is its enclosing element. Build a parent map per tree once per `rule_diagnostics` call.
- Skip ancestors whose node doesn't fit `context.category` (alias-aware, as `derives`), and ancestors whose subtree contains a recovery or error node.
- The first ancestor the context quote matches supplies its captures, which are merged into the message's captures. No match means no diagnostic.
- The context quote is compiled in `context.category`, defaulting to the checked rule's (set during preparation).

- [ ] **Step 1: Write the failing tests:**
  - `inside_reports_once_per_inner_item`: XML1003 on Nitrogen's `sample.xml`, expecting two diagnostics, at both `<api-key>` spans in the search service.
  - `parent_checks_only_the_semantic_parent`: the direct `<api-key>` matches with `parent <[ <service …> ]>`; the one under `<settings>` doesn't.
  - `nearest_ancestor_supplies_captures`: nested matching ancestors, with a message using `{name}` from the context.
  - `broken_ancestor_is_skipped`: a recovered syntax error inside `<service>` gives no XML1003 under it, and no NS0002.
  - `context_budget_exhaustion_reports_ns0002`: a context walk over a deep chain with a tiny budget reports NS0002 with the exact message.
- [ ] **Step 2: Run them** and expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-core`; all pass.
- [ ] **Step 5: Commit** — `feat: inside and parent context for match checks`.

### Task 5: Named patterns in `syntax function` arms

**Files:**
- Modify: `crates/nitride-core/src/semantic.rs` (`parse_templates` compiles arm patterns as `Quote` with `Flavor::Arm`; `match_pattern` is replaced by `Quote::matches`; the helper-call site at ~1061 owns a `Budget`)
- Test: `crates/nitride-cli/tests/rewrites.rs` or a new `crates/nitride-core/tests/arm_patterns.rs`, whichever already builds typed languages with helpers

**Interfaces:**
- Consumes: Task 3's `Quote`, `Patterns`, `Budget`, `Flavor::Arm`.

Budget behavior in arms, as Nitrogen's quote-match helpers: read how Nitrogen's lowering handles `QuoteMatchLimitException` in a match helper, and mirror it (a failed call reported at the use, or fall-through). Write the choice in the test's doc comment.

- [ ] **Step 1: Write the failing tests:**
  - `arm_uses_named_pattern`: `| (s: Secret) <[ $s ]> => …` selects the arm for a `<password>` and falls through for `<other>`.
  - `arm_named_pattern_with_spreads`: Review Focus item 1.
  - `existing_arms_unchanged`: the current rewrites and functions samples give the same output. The existing `crates/nitride-cli/tests/rewrites.rs` and `functions.rs` must pass untouched.
  - `arm_budget_exhaustion`: asserts the mirrored Nitrogen behavior.
- [ ] **Step 2: Run them** and expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace`; all pass.
- [ ] **Step 5: Commit** — `feat: named patterns in syntax function arms`.

### Task 6: XML example parity

**Files:**
- Modify: `languages/xml/Xml.ngr` and `languages/xml/sample.xml` (replace with Nitrogen's updated `examples/XmlPatterns/Xml.ngr` and `sample.xml`, verbatim)
- Modify: whatever regenerates the CLI's built-in XML language (`crates/nitride-cli/build.rs` builds it at compile time)
- Test: `crates/nitride-lsp/tests/diagnostics.rs` and `crates/nitride-cli/tests/real_languages.rs` (or the test that already covers `--parse-xml`)

- [ ] **Step 1: Write the failing tests**
  - `xml_sample_matches_nitrogen` (LSP): open `languages/xml/sample.xml` and assert exactly these coded diagnostics, in order, with their messages and spans:
    - XML1001 on the `catalog` credentials
    - XML1002 once, on `<vault name="operations">`, message `A vault stores a plaintext secret: <password>nested-example</password>`
    - XML1003 twice, on both `<api-key>` elements of the search service
  - Take the exact spans and messages from Nitrogen at or after PR 92 (`dotnet run --project Nitrogen.Cli -- check examples/XmlPatterns/sample.xml`). Record the command and output in the test's doc comment. If no Nitrogen checkout at PR 92 is available, stop and ask.
  - The same expectations through the CLI's XML observation.
- [ ] **Step 2: Run them** and expect failures, until the files are replaced.
- [ ] **Step 3: Replace the files.**
- [ ] **Step 4: Update the XML fixture from Task 2** to the new prepared JSON, and check by eye that only `patterns`, the new check and its `context` were added. Run `cargo test --workspace`; all pass. Then run `python3 scripts/prepare_parity.py` if the reference tool builds. The only XML difference is the `checks`/`patterns` data Nitride emits; note the result in the commit message.
- [ ] **Step 5: Commit** — `feat: Nitrogen's named-pattern XML example`.

### Task 7: Editor support

**Files:**
- Modify: `crates/nitride-prepare/src/parser.rs` (outline items `kind:"pattern"`; references/locals from Task 2)
- Modify: `crates/nitride-lsp/src/grammar_index.rs` (definitions and resolution for kind `pattern`; `uses` include pattern references)
- Modify: `crates/nitride-lsp/src/navigation.rs` and `completion.rs` (hover shows `pattern Name : Category` and the alternatives; completion offers patterns after `..` and in capture categories)
- Modify: `editors/vscode/syntaxes/ngr.tmLanguage.json` (`pattern`, `inside` and `parent` as keywords; `..Name` in captures as an entity name)
- Test: `crates/nitride-lsp/tests/navigation.rs`, `crates/nitride-lsp/tests/rename.rs`, `crates/nitride-lsp/tests/completion.rs`; TextMate checked with the vscode-textmate snippet used for `.nproj` (scratch, not committed)

- [ ] **Step 1: Write the failing tests** on the XML grammar:
  - `pattern_definition` (from `..Secret` to its declaration)
  - `pattern_references` (the declaration and every use)
  - `pattern_rename` (simple and qualified `Xml.Secret` uses; the qualifier kept)
  - `pattern_hover` (contains `pattern Secret : Element`)
  - `pattern_completion_after_dots` (offers `Secret`)
  - `incomplete_pattern_keeps_outline`: `pattern Secr` at the end of the module gives a diagnostic, keeps the module's outline and doesn't crash
- [ ] **Step 2: Run them** and expect failures.
- [ ] **Step 3: Implement**, and update the TextMate grammar.
- [ ] **Step 4: Run** `cargo test -p nitride-lsp` and `cd editors/vscode && npm test`; all pass. Tokenize the new `Xml.ngr` with vscode-textmate and check that `pattern`, `inside` and `..Secret` get the expected scopes.
- [ ] **Step 5: Commit** — `feat: editor support for named patterns`.

### Task 8: Documentation and final verification

**Files:**
- Modify: Nitride's documentation of `match` checks (find it with `grep -rln "match (" docs README.md`; if none exists, add a "Named patterns and tree context" section to `docs/design.md`'s grammar-language part), and `editors/vscode/README.md` if it lists grammar features

- [ ] **Step 1: Document** the three forms, the work limit and NS0002, and the codes, in Nitride's words, pointing to Nitrogen's language guide for the full reference.
- [ ] **Step 2: Run the full gate** — `cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace --locked && cargo test --locked -p nitride-cli --no-default-features --test native_only --test process --test lsp_binary && python3 -m unittest scripts/test_release.py && (cd editors/vscode && npm test)`. Everything passes.
- [ ] **Step 3: Commit** — `docs: named patterns and tree context`.
