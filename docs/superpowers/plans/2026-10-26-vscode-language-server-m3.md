# Nitride Language Server, Milestone 3 (Editing Help) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Completion, signature help, formatting (document, range, on-type), inlay hints and code actions in `nitride lsp`, for workspace programs and `.ngr` grammars.

**Architecture:**
- Each feature is a module over the existing `Analysis`, which gains three things: the program's executed statement values, recovery's repairs, and the language handle.
- **Inlay hints** show each show-statement's value, computed by executing the program with host handlers derived from the catalog's handler names (the same fixed set the CLI uses). This is Nitrogen's value-hint behaviour.
- **Quick fixes** come from the parser's own recovery repairs ("insert `;`", "remove `)`"), offered only when the repaired text parses with fewer errors. Grammars also get "remove C# semantic member" fixes. Nitride has no per-language C# fixers, so these replace Nitrogen's.
- **Completion keywords** come from parsing the text before the word with a sentinel and reading what recovery expected there. This is Nitrogen's method.

**Tech Stack:** Rust 2024, serde_json. Same crates.

**Spec:** `docs/superpowers/specs/2026-10-25-vscode-language-server-design.md` (milestone 3). Builds on milestones 1 and 2.

## Global Constraints

- Everything in milestones 1–2's Global Constraints still holds: no panics on any input, std threads, UTF-16 positions, `nitride-core` dependency-free.
- Formatting changes only whitespace at the start and end of lines, blank-line runs and the end of the file. Removing all whitespace from the input and from the output gives the same string. Lines that start inside a token (string, opaque C# block, block comment) are never touched.
- Capabilities:
  - `completionProvider: {triggerCharacters: ["."]}`
  - `signatureHelpProvider: {triggerCharacters: ["(", ","]}`
  - `documentFormattingProvider`, `documentRangeFormattingProvider`
  - `documentOnTypeFormattingProvider: {firstTriggerCharacter: "}"}`
  - `inlayHintProvider: true`
  - `codeActionProvider: {codeActionKinds: ["quickfix", "source.fixAll"]}`
- Execution for hints runs under `Limits::default()`. Any failure (preflight, a host error, a limit) gives no hints and no diagnostic.

## Review Focus

1. **Completion while the document is mid-edit** (`let a=1;\nsu`, `sum(`): names and keywords are still offered, never an error. Test: Task 2, `completion_mid_edit`.
2. **Formatting a file with CRLF line endings or mixed tabs and spaces** keeps the line endings and the non-whitespace text. Test: Task 4, `format_keeps_crlf` and `format_only_whitespace` (property over every repository sample and grammar).
3. **A program whose execution fails** (division by zero giving non-finite, a missing host value, deep recursion) gives no hints and keeps the server alive. Test: Task 1, `failed_execution_has_no_hints`.
4. **A quick fix whose edit would not help** is not offered. Test: Task 5, `unhelpful_repair_not_offered`.
5. **Signature help inside nested calls** reports the innermost call and the comma index at its depth. Test: Task 3, `nested_call_active_parameter`.

---

### Task 1: Inlay hints (statement values)

**Files:**
- Create: `crates/nitride-lsp/src/values.rs`
- Modify: `crates/nitride-lsp/src/analysis.rs` (`Analysis.values: Vec<(u32 /*end offset*/, String)>`)
- Modify: `crates/nitride-lsp/src/server.rs`
- Test: `crates/nitride-lsp/tests/hints.rs`

**Interfaces:**
- `values::bindings(language: &Language<'_>) -> Vec<HostBinding<'_>>`: handlers by catalog handler name.
  - `zero`, `identity`, `neg`, `add`, `sub`, `mul`, `div`, `pow`
  - `iso-date` (text → day number via `nitride_core::iso_date_day_number`)
  - Each closure owns its handler name.
- `values::host_values(language: &Language<'_>) -> Vec<HostValue>` for the built-in values the language declares:
  - `pi` as π, `e` as e, `tau` as τ, `today` as today's UTC day number (the computation in `nitride_cli::utc_today`, copied);
  - only when the catalog type is `Core.Scalar` (and `Dates.Date` for `today`).
  - `HostValue`'s `&'static str` fields come from string literals matched against the language's own `(kind, name, ty)`. Any other built-in value has no host value, so preflight fails and there are no hints.
- `values::compute(program: &Program<'_>, language: &Language<'_>) -> Vec<(u32, String)>`:
  - On success, one `(end offset of the statement value's HIR span, label)` per show statement, in order.
  - The label is `= <value>`:
    - `Core.Scalar`: shortest round-trip formatting with no trailing `.0`;
    - a type whose name ends in `Date`: the ISO date of the day number (inverse of `iso_date_day_number`);
    - any other type: `<value> <type's last segment>`.
- `textDocument/inlayHint` returns the hints whose position is inside the requested range, as `{position, label, paddingLeft: true}`.

- [ ] **Step 1: Write failing tests.**
  - `scalar_value_hint`: Portable `let x : Scalar = 6;\nx * 7;` gives one hint `= 42` at the end of `x * 7`.
  - `date_value_hint`: Dates `2026-10-05 + 2 days;` gives `= 2026-10-07`.
  - `range_filters_hints`.
  - `failed_execution_has_no_hints`: `1/0;` (non-finite) gives `[]`; a syntax-only language gives `[]`.
  - `iso_round_trip` (unit test in `values.rs`): every day from 0001-01-01 to 9999-12-31 at a step of 97 round-trips.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test hints`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Inlay hints with statement values"`

### Task 2: Completion

**Files:**
- Create: `crates/nitride-lsp/src/completion.rs`
- Test: `crates/nitride-lsp/tests/completion.rs`

**Interfaces:**
- `completion::items(analysis, text, offset, language: Option<&Language<'_>>, grammar: Option<GrammarAt<'_>>) -> Value` returns a `CompletionItem[]`.
- **The word being typed** is the run of `[A-Za-z0-9_.]` ending at the cursor. Items are filtered by case-insensitive prefix on it.
- **Program names:**
  - definitions whose `name_span[0] < word start` (kind 6 Variable, or 3 Function when the kind is `function`, `detail` = the type);
  - then built-in names from the language's `Action::Call { builtins }` and `Action::Reference { values }` (kind 3 or 6, `detail` = `built-in`).
- **Grammar names:** every rule of the grammar set (kind 7 Class, 21 Constant for tokens, 3 Function for functions/helpers, `detail` = the module).
- **Keywords** (kind 14):
  - Parse `text[..word start] + "\u{1}"` with `parse_recovering`.
  - Take every `Repair::Missing { expected: Expected::Literal(w), at }` with `at == word start` (after skipping whitespace back to it) and `w` a word.
  - The grammar language for `.ngr` files is `nitride_prepare::grammar_language()`.
- Order: names first, then keywords, each group sorted. Deduplicate by label.

- [ ] **Step 1: Write failing tests.**
  - `names_before_cursor`: in Calls, `let alpha=1;\nlet beta=2;\nal` offers `alpha` but not `beta`.
  - `builtins_offered`: `su` offers `sum` (detail `built-in`).
  - `keywords_at_statement_start`: `le` at a statement start offers `let`.
  - `completion_mid_edit`: `sum(` and `let a=1;\nsu` return arrays and no error.
  - `grammar_rules_offered`: in `Power.ngr`, typing `Ex` inside an alternative offers `Expr`.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test completion`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Completion of names, built-ins, rules and keywords"`

### Task 3: Signature help

**Files:**
- Create: `crates/nitride-lsp/src/signatures.rs`
- Test: `crates/nitride-lsp/tests/signatures.rs`

**Interfaces:**
- `signatures::help(text: &str, offset: u32, language: &Language<'_>, leaves: &[[u32; 2]]) -> Value`. It returns `SignatureHelp` or `null`.
- **Finding the call** (Nitrogen's text scan):
  - Walk back from the cursor. Skip characters inside token leaves other than `(`, `)`, `,`. Stop at `;`, `{`, `}`.
  - Track depth, so the innermost unclosed `name(` is found.
  - The active parameter is the number of top-level commas between `(` and the cursor.
- **Signatures:** for a callee `name` with a built-in `Action::Call` entry, one signature per candidate operation. The label is `name(T1, T2) : R`, with `parameters` as label offsets. Otherwise `null`.

- [ ] **Step 1: Write failing tests.**
  - `builtin_signature`: in Calls, `sum(1,` gives label `sum(Core.Scalar, Core.Scalar) : Core.Scalar` with `activeParameter: 1`.
  - `nested_call_active_parameter`: `sum(neg(1), ` gives `sum` with parameter 1, and `sum(neg(` gives `neg` with parameter 0.
  - `outside_call_is_null`: after `sum(1,2);` the result is `null`.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test signatures`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Signature help for built-in calls"`

### Task 4: Formatting

**Files:**
- Create: `crates/nitride-lsp/src/format.rs`
- Test: `crates/nitride-lsp/tests/format.rs`

**Interfaces:**
- `format::edits(analysis: &Analysis, lines: std::ops::RangeInclusive<u32>, tab_size: u32, insert_spaces: bool) -> Vec<Value>` returns `TextEdit`s.
- **Indentation:**
  - The level is the number of unclosed `{` in literal text (not inside tokens or comments) before the line, minus one when the line starts with `}`.
  - A line that starts inside an unclosed `(` or `[` keeps its indentation.
  - Comment-only lines take the indentation of the next code line.
- **Other rules:** trailing whitespace is removed; runs of two or more blank lines become one; the file ends with exactly one line ending, of the kind the file uses.
- Each edit replaces only leading or trailing whitespace of one line, or the end of the file.
- `textDocument/formatting` covers all lines. `textDocument/rangeFormatting` covers the requested lines. `textDocument/onTypeFormatting` with `}` covers only the line of the position.

- [ ] **Step 1: Write failing tests.**
  - `indents_blocks` (`.ngr`): a grammar with flattened indentation formats to two-space levels.
  - `uses_tabs_when_asked`.
  - `trims_and_collapses`.
  - `format_keeps_crlf`.
  - `range_only_touches_range`.
  - `on_type_dedents_closing_brace`.
  - `format_only_whitespace`: for every `languages/**/*.ngr` and `languages/*/sample.*`, apply the edits and check that the text with all whitespace removed is unchanged, and that formatting the result again gives no edits.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test format`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Document, range and on-type formatting"`

### Task 5: Code actions

**Files:**
- Create: `crates/nitride-lsp/src/actions.rs`
- Modify: `crates/nitride-lsp/src/analysis.rs` (`Analysis.repairs: Vec<Fix>`, with `enum Fix { Insert { at: u32, text: String }, Delete { span: [u32; 2] } }`, taken from `Repair::Missing` with a literal and from `Repair::Skipped`)
- Test: `crates/nitride-lsp/tests/actions.rs`

**Interfaces:**
- `textDocument/codeAction` over the requested range:
  - **Programs:** for each repair whose position lies in the range, a `quickfix` titled ``Insert `;` `` or ``Remove `)` ``.
    - It carries a `WorkspaceEdit` for this document.
    - It is offered only if the repaired text, parsed with recovery, has fewer recovery diagnostics than the current text.
    - `isPreferred: true` when it is the only fix offered.
  - **Grammars:** for each `C# semantic member is not admitted` warning in the range, a `quickfix` titled `Remove C# semantic member`. It deletes the warned span plus the rest of its line when that is whitespace (including the line ending).
  - **Grammars, whole file:** when the file has any such warnings, one `source.fixAll` action titled `Remove all C# semantic members`, deleting them all.
  - When `context.only` is given, only those kinds are returned.

- [ ] **Step 1: Write failing tests.**
  - `insert_missing_semicolon` (Calls `let a=1\nsum(a,a);`): one fix that inserts `;`.
  - `unhelpful_repair_not_offered`: build a case where the repair's text parses no better. If none exists in the repository languages, test the filter directly through `actions::helps(before, after)`.
  - `remove_semantic_member`: in `Calls.Reference.ngr`, the fix's edit removes line 5 entirely.
  - `fix_all_semantic_members`: applying the edit gives a grammar that prepares tolerantly with zero warnings.
  - `only_filters_kinds`.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test actions`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Quick fixes from recovery repairs and C# member removal"`

### Task 6: Docs and end-to-end

**Files:**
- Modify: `README.md`, `editors/vscode/README.md` (feature lists)
- Modify: `crates/nitride-cli/tests/lsp_binary.rs`: add `completion`, `inlayHint` (expect `= 512` for `x ^ 3 ^ 2` with `x = 2`) and `formatting` to the session.

- [ ] **Step 1: Extend `binary_session`.** Run it. Expected: PASS, since the features exist. Record in the ledger that this test extends coverage rather than driving new code.
- [ ] **Step 2: Update both READMEs.**
- [ ] **Step 3: Run** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 4: Commit** `git commit -m "Document editing help; extend end-to-end test"`
