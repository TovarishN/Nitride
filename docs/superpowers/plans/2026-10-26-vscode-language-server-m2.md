# Nitride Language Server, Milestone 2 (Navigation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hover, go-to-definition/declaration, references, document highlights, rename (with prepareRename) and workspace symbols in `nitride lsp`, for workspace programs and `.ngr` grammars.

**Architecture:**
- Each document analysis gains a `SymbolIndex`: definitions, references resolved to them, built-ins, and expression types.
- Typed languages fill the index from the engine's `Program`. Its `bindings` already resolve each reference to the byte start of its declaration's name, and its HIR nodes carry types.
- Syntax-only languages, and typed programs whose compile failed, fall back to a name-based binder over the grammar's `declares`/`references` clauses.
- Grammars get a `GrammarIndex` over every `.ngr` of the languages that use them, so rule navigation and rename work across files.
- LSP handlers stay thin and live in a new `navigation.rs`.

**Tech Stack:** Rust 2024, serde_json. Same crates as milestone 1.

**Spec:** `docs/superpowers/specs/2026-10-25-vscode-language-server-design.md` (milestone 2). Builds on `docs/superpowers/plans/2026-10-26-vscode-language-server-m1.md`.

## Global Constraints

- Everything in milestone 1's Global Constraints still holds: no panics on any input, `nitride-core` dependency-free, std threads only, UTF-16 positions on the wire.
- Programs are single-document: their navigation never leaves the document. Grammar navigation spans every `.ngr` of the languages that list the file, or the file alone when no manifest lists it.
- Built-in names are never renamed. `prepareRename` refuses them with the message `built-in names cannot be renamed`.
- A new name must match `[A-Za-z_][A-Za-z0-9_]*`, else rename fails with `-32602` and the message `not an identifier`.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and the bootstrap `--check` pass after every task.

## Review Focus

1. **Requests on whitespace, comments, keywords or positions past the end of the file** return `null` (or `[]`) and never an error. Test: Task 4, `nothing_under_cursor`.
2. **A program with a syntax error** still navigates in the parts that parsed, through the name-based fallback. Test: Task 2, `fallback_binds_damaged_program`.
3. **Rename in a grammar whose rule is used by another grammar file** edits both files, including files that are not open. Test: Task 6, `rename_rule_across_files`.
4. **Shadowing and redeclaration** (`let x=1; let x=x;`): each reference goes to the declaration the engine resolved, not the first one by spelling. Test: Task 2, `redeclaration_follows_engine`.
5. **Non-ASCII names and emoji before a symbol**: edits and ranges use correct UTF-16 columns. Test: Task 6, `rename_after_emoji`.

---

### Task 1: Prepared `references` clauses and grammar outline alternatives

**Files:**
- Modify: `crates/nitride-prepare/src/{lib,normalize,parser}.rs`
- Test: `crates/nitride-prepare/tests/tolerant.rs`

**Interfaces:**
- Produces:
  - `Prepared.references: Vec<ReferenceClause>`, with `pub struct ReferenceClause { pub rule: String, pub field: String, pub kinds: Vec<String> }`. There is one per `references` clause, in both modes, collected next to `declarations`.
  - `GrammarItem` gains `"alternative"` children under `extensible` and `extend` items, for named alternatives. `name` is the alternative name and `name_span` covers it.
  - `pub fn grammar_references(source: &Source) -> Result<Vec<GrammarReference>, PrepareError>`, with `pub struct GrammarReference { pub name: String, pub span: [usize; 2], pub module: String }`. It lists every rule name used in a grammar file: `Reference` nodes, `extend syntax X`, and `Call` targets. `module` is the enclosing module.

- [ ] **Step 1: Write failing tests.**
  - `references_clauses_recorded`: tolerant-prepare `Calls.Reference.ngr`. Expect `references` to contain `{rule:"Calls.Ref", field:"Name", kinds:["value"]}`.
  - `outline_lists_alternatives`: in the `Calls.ngr` outline, `Expr` has an `alternative` child `Num`.
  - `grammar_references_found`: for `languages/power/Power.ngr`, the references include `Portable.Expr` (the `extend` target) with module `Portable.Power`, plus each `Expr` used in `Pow`.
- [ ] **Step 2: Run** `cargo test -p nitride-prepare --test tolerant`. Expected: FAIL.
- [ ] **Step 3: Implement**, reusing the decoder (`Decoder::kind/field/span`).
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Record references clauses, alternatives and rule uses for navigation"`

### Task 2: Program symbol index

**Files:**
- Create: `crates/nitride-lsp/src/index.rs`
- Modify: `crates/nitride-lsp/src/analysis.rs` (`Analysis.index`)
- Test: `crates/nitride-lsp/tests/index.rs`

**Interfaces:**
- Produces, in `index.rs`:
  - `pub struct SymbolIndex { pub definitions: Vec<Definition>, pub references: Vec<Use>, pub builtins: Vec<BuiltinUse>, pub types: Vec<([u32; 2], String)> }`.
  - `Definition { name: String, kind: String, name_span: [u32; 2], node_span: [u32; 2], ty: Option<String> }`.
  - `Use { span: [u32; 2], target: usize /* index into definitions */ }`.
  - `BuiltinUse { span: [u32; 2], kind: String, name: String }`.
  - `pub fn at(&self, offset: u32) -> Option<Target>`, with `enum Target { Definition(usize), Builtin(usize) }`. It returns the definition or built-in whose name span contains `offset` (end inclusive), checking definitions, then uses, then built-ins.
  - `pub fn occurrences(&self, definition: usize) -> Vec<([u32; 2], bool /* is declaration */)>`.
- How it is filled:
  - **Typed** (`compile_recovering` returned a program): definitions are the `DeclaredName`s. A definition's `ty` is `program.nodes[statement.value].ty` for the statement whose `symbol == Some(name_span[0])`. Uses come from `program.bindings`, mapped by `symbol == definition.name_span[0]`; built-ins come from `program.builtin_bindings`; `types` are HIR node `(span, ty)`, skipping `<error>`.
  - **Fallback** (syntax-only, or no program): definitions are the `DeclaredName`s with `ty: None`. Uses are tree nodes whose rule has a `ReferenceClause`: the labelled field's text resolves to the nearest preceding definition of a matching kind and the same name, else the first later one. Unresolved names are left out.

- [ ] **Step 1: Write failing tests** (through `analysis::analyze` over in-memory registries built as in `tests/workspace.rs`):
  - `typed_uses_resolve`: in `.calls` `let a=1;\nsum(a,a);`, two uses target `a`, and `a.ty == Some("Core.Scalar")`.
  - `redeclaration_follows_engine`: in `let x=1;\nlet x=x;\nx;`, the use in `let x=x` targets the first `x` and the final use targets the second.
  - `builtins_recorded`: `sum` in `sum(1,2);` is a `BuiltinUse` of kind `function`.
  - `fallback_binds_damaged_program`: in `.calls` `let a=1;\nlet b=;\nsum(a,a);`, two uses still target `a`.
  - `syntax_only_binds_by_name`: same shape through a `nitrogen.json` Calls.Reference language.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test index`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Index program symbols from the engine or by name"`

### Task 3: Grammar index

**Files:**
- Create: `crates/nitride-lsp/src/grammar_index.rs`
- Modify: `crates/nitride-lsp/src/registry.rs` (`pub fn grammar_set(&self, path: &Path) -> Vec<PathBuf>`: the union of `entry.grammars` over the languages that use `path`, else `[path]`)
- Test: `crates/nitride-lsp/tests/grammar_index.rs`

**Interfaces:**
- Produces:
  - `pub struct GrammarIndex { pub files: Vec<GrammarFile> }`, with `GrammarFile { path: PathBuf, text: String, outline: Vec<GrammarItem>, references: Vec<GrammarReference> }`.
  - `pub fn build(paths: &[PathBuf], open: &dyn Fn(&Path) -> Option<String>) -> GrammarIndex`. Unreadable or unparsable files are skipped.
  - `pub fn definition(&self, name: &str, module: &str) -> Option<(usize /*file*/, &GrammarItem)>`. A qualified `M.R` matches module `M` and rule `R`. A bare `R` matches the same module first, then a unique `R` in any module.
  - `pub fn at(&self, file: usize, offset: u32) -> Option<(usize, GrammarItem)>`: the rule defined or referenced at `offset`.
  - `pub fn uses(&self, file: usize, item: &GrammarItem) -> Vec<(usize, [u32; 2])>`: every reference resolving to it, in all files.

- [ ] **Step 1: Write failing tests.**
  - `extend_target_resolves_across_files`: with `Portable.ngr` + `Power.ngr`, `Portable.Expr` in `Power.ngr` resolves to `Expr` in `Portable.ngr`.
  - `bare_name_prefers_module`: `Expr` inside `Portable` resolves locally.
  - `uses_span_files`: the uses of Portable's `Expr` include spans in both files.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test grammar_index`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Index grammar rules across a language's files"`

### Task 4: Hover

**Files:**
- Create: `crates/nitride-lsp/src/navigation.rs`
- Modify: `crates/nitride-lsp/src/server.rs` (`hoverProvider: true`, dispatch)
- Test: `crates/nitride-lsp/tests/navigation.rs`

**Interfaces:**
- `textDocument/hover` returns `{contents:{kind:"markdown", value}, range}` or `null`. The `value` by case:
  - definition or use: ``` `x` : Core.Scalar ``` then a new line and *value*. Without a type it is ``` `x` ```, a new line and *value*.
  - built-in: ``` `sum` ``` then a new line and *built-in function*.
  - expression with a type, when no symbol is under the cursor: ``` `Core.Scalar` ```, from the innermost `types` span containing the offset.
  - grammar rule: a ```` ```ngr ```` block holding the first line of the rule's definition (trimmed, at most 200 characters), then a new line and *syntax rule `Calls.Let`*. The kind word follows the `GrammarItem.kind`.

- [ ] **Step 1: Write failing tests.**
  - `hover_declaration_type`, `hover_use_type`, `hover_builtin`, `hover_expression_type` (on `+` in `1+2` → `Core.Scalar`, using the Portable language).
  - `hover_rule_across_files`: on `Portable.Expr` in `Power.ngr`.
  - `nothing_under_cursor`: on a space, inside a comment, on `let`, and at line 99. Every case gives `result == null`.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test navigation`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Hover for symbols, built-ins, expressions and rules"`

### Task 5: Definition, declaration, references, highlights

**Files:**
- Modify: `crates/nitride-lsp/src/{navigation,server}.rs`
- Test: `crates/nitride-lsp/tests/navigation.rs`

**Interfaces:**
- `textDocument/definition` and `textDocument/declaration` both return one `Location` (the definition's name range), or `null` for built-ins and empty positions. For grammars the location may be in another file.
- `textDocument/references` returns `Location[]` for every use, plus the declaration when `context.includeDeclaration`, ordered by file then offset.
- `textDocument/documentHighlight` returns occurrences in this document: the declaration as kind 3 (Write), uses as kind 2 (Read).
- Capabilities: `definitionProvider`, `declarationProvider`, `referencesProvider`, `documentHighlightProvider`.

- [ ] **Step 1: Write failing tests.**
  - `definition_from_use`: from `a` in `sum(a,a)` to the `a` of `let a`.
  - `references_with_and_without_declaration`: counts 3 and 2.
  - `highlights_kinds`.
  - `rule_definition_across_files`.
  - `rule_references_across_files`.
  - `definition_of_builtin_is_null`.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test navigation`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Definition, references and highlights"`

### Task 6: Rename

**Files:**
- Modify: `crates/nitride-lsp/src/{navigation,server}.rs`
- Test: `crates/nitride-lsp/tests/rename.rs`

**Interfaces:**
- `textDocument/prepareRename` returns `{range, placeholder}` for a definition, use or grammar rule. For a built-in it returns error `-32602` with `built-in names cannot be renamed`; elsewhere it returns `null`.
- `textDocument/rename` returns `WorkspaceEdit {changes: {uri: TextEdit[]}}` covering every occurrence:
  - programs: this document only;
  - grammar rules: every file in the grammar set, read from its buffer or from disk;
  - a qualified use (`Portable.Expr`) has only its last segment replaced.
- Capability: `renameProvider: {prepareProvider: true}`.

- [ ] **Step 1: Write failing tests.**
  - `rename_program_symbol`: `let a=1;\nsum(a,a);` renamed to `total` gives 3 edits.
  - `prepare_rename_refuses_builtin`.
  - `rename_rejects_non_identifier` (`1x`).
  - `rename_rule_across_files`: rename Portable's `Expr` to `Term` with only `Power.ngr` open. Expect edits in both URIs, and in `Power.ngr` the edit replaces `Expr` in `Portable.Expr`.
  - `rename_after_emoji`: `/* 😀 */ let a=1;` puts the edit at character 12.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test rename`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Rename symbols and grammar rules"`

### Task 7: Workspace symbols, docs and end-to-end

**Files:**
- Modify: `crates/nitride-lsp/src/{navigation,server}.rs`, `README.md`, `editors/vscode/README.md`
- Modify: `crates/nitride-cli/tests/lsp_binary.rs` (add hover and definition to the session)
- Test: `crates/nitride-lsp/tests/navigation.rs`

**Interfaces:**
- `workspace/symbol` with `{query}` returns `SymbolInformation[]` from:
  - every grammar rule of every workspace language, with `containerName` set to its module;
  - the definitions of open documents.
- Matching is a case-insensitive subsequence of the name; an empty query returns everything, capped at 500 results. Kinds are as in milestone 1's outline.
- Capability: `workspaceSymbolProvider: true`.

- [ ] **Step 1: Write failing tests.**
  - `workspace_symbols_find_rules`: query `pow` finds `Pow` in `Power.ngr` with container `Portable.Power`.
  - `workspace_symbols_find_open_definitions`.
  - Extend `binary_session` with a hover on `x` that contains `Scalar` and a definition request.
- [ ] **Step 2: Run** the tests. Expected: FAIL.
- [ ] **Step 3: Implement**, then update both READMEs' feature lists.
- [ ] **Step 4: Run** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Workspace symbols; document navigation"`
