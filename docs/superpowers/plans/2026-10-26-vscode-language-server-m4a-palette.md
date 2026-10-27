# Nitride Language Server, Milestone 4a (Palette Coloring) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Color tokens the way Nitrogen does: TextMate-palette semantic token types for clients that ask for them, with the same literal roles (declaration keyword, section keyword, modifier, constant, operation word, brackets, terminators, separators, operators) and the same `.ngr` scopes.

**Architecture:**
- A `palette.rs` table holds Nitrogen's palette, entry for entry: 11 general entries plus the `.ngr` entries. Each entry has a legend name, a TextMate scope and a fallback standard type.
- A `roles.rs` classifier computes each `(rule, literal)`'s role from the language's grammar expressions, with Nitrogen's `LiteralRoles` rules.
- `tokens.rs` tags each literal with a palette name. It finds the literal's role from the innermost syntax node containing it, matching the longest literal of that rule at the position (so `=>` is one token).
- `.ngr` files use Nitrogen's `NgrStyles` table, mapped to Nitride's grammar-frontend rule names.
- Encoding sends palette types to clients that set `initializationOptions.paletteTokens`, and each entry's fallback type, or no token, to the rest.

**Tech Stack:** Rust, TypeScript (extension). Same crates.

**Spec:** `docs/superpowers/specs/2026-10-25-vscode-language-server-design.md` (milestone 4: palette tokens). Reference: Nitrogen's `Nitrogen.LanguageService/Palette.cs`, `Palette.Ngr.cs`, `Nitrogen.Grammar/LiteralRoles.cs`, `Nitrogen.Cli/LspCommand.cs` (`NgrStyles`), `docs/editor-support.md` ("Coloring: the TextMate palette").

## Global Constraints

- **Legend for non-palette clients:** milestone 1's 11 standard types plus `enumMember` appended (index 11). Existing indices don't move.
- **Legend for palette clients:** those 12, then every palette entry in Nitrogen's order (general entries, then `.ngr` entries).
- **Palette names and scopes are copied verbatim** from Nitrogen's `Palette.cs`/`Palette.Ngr.cs`, and match `editors/vscode/package.json`'s `semanticTokenTypes` (`id` = name, `description` = scope).
- **Fallbacks are Nitrogen's.** An entry whose fallback is `null` (brackets, terminators, separators, `.ngr` punctuation) produces no token for non-palette clients.
- Everything from earlier milestones' Global Constraints still holds.

## Review Focus

1. **A client without `paletteTokens`** still gets valid tokens: no index beyond its legend, and no overlapping tokens. Test: Task 2, `fallback_legend_is_valid`.
2. **Multi-character literals** (`=>`, `..`, `!=`) are one token with one role, not several characters. Test: Task 2, `longest_literal_wins`.
3. **Literals inside damaged regions** (recovered trees) still get a role or the spelling fallback, never a panic. Test: Task 2, `roles_in_damaged_program`.
4. **Alias rules** (no node of their own): their literals are classified under the enclosing rule, as Nitrogen inlines them. Test: Task 1, `alias_literals_inline`.
5. **Every `.ngr` sample in the repository** encodes without overlap and with only legend types. Test: Task 3, `ngr_tokens_valid_everywhere`.

---

### Task 1: Literal roles

**Files:**
- Create: `crates/nitride-lsp/src/roles.rs`
- Test: `crates/nitride-lsp/tests/roles.rs`

**Interfaces:**
- `pub enum Role { Operation, DeclarationKeyword, SectionKeyword, ModifierWord, ConstantWord, BlockBracket, Parenthesis, SquareBracket, Terminator, Separator, Operator }`.
- `pub fn roles(language: &Language<'_>, declares: &dyn Fn(&str) -> bool) -> HashMap<(u16, String), Role>`, computed for every non-lexical rule. `declares(rule name)` is true when the rule has a `declares` clause, from `Prepared.declarations`.
- **Walk**, following `LiteralRoles.Walker`:
  - It tracks `first` (no non-nullable element yet), `in_argument` and `word_choice`.
  - `Sequence` items: `first && nullable-so-far`.
  - `Choice`: it is a word choice when it has two or more alternatives that are all word literals. A word choice that is the whole (unlabelled) body does not pass `first`.
  - `Label(l, e)` sets `in_argument` when `l` is in the rule's operation argument fields (`Action::Operation { fields }` or `Action::Overload { fields }`).
  - `Repeat` resets `word_choice`. `Predicate` is skipped.
  - `Ref` to an alias rule inlines that rule's expression, guarding against cycles.
- **Classify**, following `LiteralRoles.Classify`:
  - punctuation by spelling (`{}`, `()`, `[]`, `;`, `,`, else operator);
  - operation when the rule lowers to an operation and the literal is not in an argument;
  - constant for a word choice that is not first;
  - first: declaration keyword if the rule declares, else section keyword;
  - else modifier.
- The earliest occurrence decides.

- [ ] **Step 1: Write failing tests.**
  - `let_is_declaration_keyword` (Calls `Calls.Let`).
  - `call_is_section_keyword` (Calls `Calls.AnyCall`, word `call`).
  - `duration_is_operation_word` (Portable `Portable.Duration`).
  - `punctuation_by_spelling`.
  - `word_choice_is_constant`: an inline grammar `syntax Unit = Value:Number Scale:("days" / "weeks");` gives `days` as constant.
  - `alias_literals_inline`: in a syntax-only language, an alias's literal is classified under its user.
  - `modifier_after_element`.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test roles`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Literal roles from grammar positions"`

### Task 2: Palette table, legend and program tokens

**Files:**
- Create: `crates/nitride-lsp/src/palette.rs`
- Modify: `crates/nitride-lsp/src/{tokens,server,analysis}.rs`
- Test: `crates/nitride-lsp/tests/palette.rs`

**Interfaces:**
- `palette::ENTRIES: &[Entry]`, with `pub struct Entry { pub name: &'static str, pub scope: &'static str, pub fallback: Option<&'static str /* standard type */> }`, in Nitrogen's order.
- `palette::of(role: Role) -> &'static Entry`.
- `palette::ngr(scope: &str) -> Option<&'static Entry>`.
- The server reads `initializationOptions.paletteTokens` (bool) at `initialize` and reports the matching legend.
- Each token carries `palette: Option<&'static str>`. Encoding gives the palette index for a palette client, and the fallback type or no token otherwise.
- **Program literals:**
  - The rule is the innermost non-lexical node whose span contains the literal; the literal is the longest of that rule's literals (from `roles`) matching at the position; its role gives the palette entry.
  - With no match, use the spelling: words become `sectionKeyword`, punctuation as in Task 1.
  - Names keep their milestone 1 standard types.
- `Analysis` keeps `Arc<HashMap<(u16, String), Role>>` per loaded language, cached on `Loaded`.

- [ ] **Step 1: Write failing tests.**
  - `palette_legend_when_asked`: initialize with `paletteTokens: true` gives a legend of 12 standard types plus every name in `editors/vscode/package.json` `semanticTokenTypes`, in that order.
  - `let_is_declaration_keyword_token` (palette client).
  - `duration_is_operation_word_token` (Portable, palette client).
  - `fallback_legend_is_valid`: a non-palette client on the Calls sample gets only indices below 12, and no token for `(`, `)` or `;` (their fallback is none).
  - `longest_literal_wins`: an inline grammar with `"=>"` gives one 2-character token.
  - `roles_in_damaged_program`: `let =` and `sum(1,` on a palette client give tokens and no panic.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test palette`. Expected: FAIL.
- [ ] **Step 3: Implement**, and update milestone 1's `calls_sample_tokens`: punctuation now has no token for non-palette clients, as in Nitrogen. Record that in the ledger as a ruling.
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Palette token types for programs"`

### Task 3: `.ngr` palette

**Files:**
- Modify: `crates/nitride-lsp/src/{tokens,palette}.rs`
- Test: `crates/nitride-lsp/tests/palette.rs`

**Interfaces:**
- `palette::ngr_literal(rule: &str /* frontend rule's last segment */, literal: &str) -> Option<&'static Entry>`.
  - Nitrogen's `NgrLiterals()`: the by-spelling table (`*`) and the by-rule entries.
  - Rule names are mapped to the frontend's:
    - `Module` → `Module`
    - `CharClass` → `Class`
    - `Parenthesized`, `GroupClose`, `SeparatorTail` → `Group`
    - `EmptyGroup` → `EmptyGroup`
    - `KindGroup` → `KindGroup`
    - `LabelPrefix` → `Labelled`
    - `Any` → `Any`
    - `References` → `References`
  - Nitrogen rules with no frontend equivalent are left out.
- `.ngr` names and tokens:
  - module names: `entity.name.namespace`;
  - rule definitions and rule references: `entity.name.function.rule`;
  - field labels (`Name:`): `variable.other.member.label`;
  - `String` tokens: `string.quoted.double`; `Char` tokens: `string.quoted.single`;
  - `Integer` tokens: `constant.numeric.integer`; `QuoteText`/`TemplateString` tokens: `string.quoted.other.template`;
  - comments: `comment.line.double-slash` / `comment.block`.

- [ ] **Step 1: Write failing tests.**
  - `ngr_storage_and_rule_names`: in `Calls.ngr`, palette client: `syntax` is `ngrStorageTypeSyntax`, `Program` is `ngrEntityNameFunctionRule`, `module` is `ngrStorageTypeModule`, `"let"` is `ngrStringQuotedDouble`, `|` is `ngrKeywordOperatorAlternative`.
  - `ngr_tokens_valid_everywhere`: for every `languages/**/*.ngr`, palette and non-palette clients both get tokens that are sorted, do not overlap, and use only legend indices.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test palette`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Palette coloring for grammars"`

### Task 4: Extension, docs, end-to-end

**Files:**
- Modify: `editors/vscode/package.json`: add `contributes.semanticTokenScopes` copied from Nitrogen's `package.json`.
- Modify: `editors/vscode/src/extension.ts`: `initializationOptions: { paletteTokens: true }`.
- Create: `editors/vscode/src/palette.test.ts`: every `semanticTokenTypes` id has a scope in `semanticTokenScopes`, and the scope equals its description.
- Modify: `crates/nitride-cli/tests/lsp_binary.rs`: initialize with `paletteTokens: true` and assert a `declarationKeyword` token for `let`.
- Modify: `README.md`, `editors/vscode/README.md`.

- [ ] **Step 1: Write the failing TS test** (`npm test`: FAIL until the scopes exist) and the e2e assertion.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run** `npm test`, `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 4: Package** with `scripts/package-vscode.sh darwin-arm64`. Expected: the `.vsix` builds.
- [ ] **Step 5: Commit** `git commit -m "Extension asks for palette tokens; document palette coloring"`
