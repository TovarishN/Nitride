# Nitride Language Server, Milestone 4b (Signed Programs) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Files choose their language with Nitrogen's signatures, so a program or grammar works without a manifest:
- **Programs:** a program's leading `#!nitrogen language "./Calc.syntax"` (a quoted path) or `#!nitrogen language Calc` (a name found in the workspace) selects its language.
- **Grammars:** a file starting `#!nitrogen grammar` is a grammar whose `language Calc { start …; grammars …; extensions …; }` block declares a language.
- **Extensions:** a signed grammar's `extensions` also claims files by extension.

**Scope (agreed):** language headers, grammar signatures with the `language { }` declaration, and name discovery. Augment headers, `#!nitrogen region` blocks and `nitrogen.json` augmentation rules are out.

**Catalog (agreed):** a signed language is typed when a sidecar `<stem>.catalog.json` sits next to the declaring file (`Calc.catalog.json` beside `Calc.syntax`), and syntax-only otherwise. This keeps signed grammars valid for Nitrogen.

**Architecture:**
- **Header reader:** `signed.rs` ports Nitrogen's `DocumentSignatures.Read`. It reads directives only in the leading trivia (BOM, whitespace, `//`, `/* */`, `<!-- -->`, bare `#!nitrogen` lines) and masks them to spaces, keeping offsets and line breaks.
- **Grammar frontend:** it learns the `language Name { settings }` declaration. `nitride-prepare` reads it and preparation ignores it.
- **Discovery:** the registry finds signed grammars in the workspace folders (bounded scan) and adds one `LanguageState` per declaration.
- **Selection order:** analysis picks a document's language by header, then manifest extension, then signed-grammar extension.
- **Extension:** it routes `#!nitrogen grammar` documents to the `ngr` language, and syncs any file whose header may select a language.

**Tech Stack:** Rust, TypeScript. Same crates.

**Spec:** `docs/superpowers/specs/2026-10-25-vscode-language-server-design.md` (milestone 4: signed programs). Reference: Nitrogen's `Nitrogen.Grammar/DocumentSignatures.cs`, `Nitrogen.Workspace/DocumentClassification.cs`, `Nitrogen.Ngr/Nitrogen.ngr` (`LanguageDeclaration`), `examples/Signatures/README.md`, `editors/vscode/src/signatureDocuments.ts`.

## Global Constraints

- **Header syntax is Nitrogen's:**
  - `#!nitrogen grammar`, `#!nitrogen language "<path>"` or `#!nitrogen language <Name>`.
  - A name is `[letter_][letter digit _]*`; a path is relative to the containing file.
  - A grammar signature cannot be combined with another signature, and a second `language` signature is an error.
  - Errors use code `NGR0401` and Nitrogen's messages: `expected grammar, language, or augment after #!nitrogen`, `expected a non-empty quoted language path`, `expected a quoted local path or a language name`, `unexpected text after Nitrogen signature`, `incompatible or duplicate Nitrogen document signature`, `unterminated signature comment`.
  - `augment` and `region` directives are skipped: they are not an error, and they select nothing.
- **Masking:** directives are replaced by spaces except `\r` and `\n`, so every other offset is unchanged.
- **Name discovery** scans workspace folders, skipping hidden directories, `bin`, `obj`, `node_modules`, `target`, `artifacts`, `dist`. It reads at most 2,000 files of at most 1 MiB, and only files whose text contains `#!nitrogen grammar`.
  - A missing name gives `no signed grammar declares language <Name>`.
  - An ambiguous name gives `language <Name> is declared by several signed grammars: <paths>`.
  - Both diagnostics sit on the header's span.
- **Settings:**
  - Read: `start` (required: `signed language <Name> needs a start rule`), `grammars` (paths relative to the declaring file) and `extensions`.
  - Ignored without a diagnostic: `namespace`, `sources`, `usings`, `tokens`, `types`. They are C#-side, as manifest `sources` is ignored.
- **Selection order:**
  1. a document's own `language` header;
  2. a manifest extension;
  3. a signed grammar's `extensions`.
- Everything from earlier milestones' Global Constraints still holds: no panics, std threads, UTF-16 positions.

## Review Focus

1. **A header after a BOM, blank lines, prose in a block comment, or CRLF line endings** is read, and every program offset after it is unchanged. Test: Task 1, `header_variants_keep_offsets`.
2. **A program whose header names a missing or ambiguous language** gets one diagnostic on the header and no crash; its tokens are empty. Test: Task 4, `missing_and_ambiguous_names`.
3. **Editing a signed grammar** (its declaration or modules) re-prepares its language and re-analyses open programs that select it, as manifest grammars do. Test: Task 4, `editing_signed_grammar_reloads`.
4. **A signed grammar that is also listed in a manifest** is not prepared twice under two names, and its diagnostics are not published twice. Test: Task 3, `manifest_and_signature_coexist`.
5. **Workspace scan bounds:** a folder with more than 2,000 files, or with a 2 MiB file, finishes the scan and still finds a signed grammar that is within bounds. Test: Task 3, `scan_bounds`.

---

### Task 1: Header reader

**Files:**
- Create: `crates/nitride-lsp/src/signed.rs`
- Test: `crates/nitride-lsp/tests/signed.rs`

**Interfaces:**
- `pub enum Mode { Grammar, Language }`.
- `pub struct Reference { pub value: String, pub is_path: bool }`.
- `pub struct Signature { pub mode: Mode, pub reference: Option<Reference>, pub span: [u32; 2] }`.
- `pub struct Header { pub signatures: Vec<Signature>, pub masked: String, pub diagnostics: Vec<(/*span*/ [u32; 2], String)> }`.
- `pub fn read(text: &str) -> Header`. It ports Nitrogen's reader with byte offsets. `augment` and `region`/`end` lines are read and ignored.

- [ ] **Step 1: Write failing tests.**
  - `language_path_and_name`: `#!nitrogen language "./Calc.syntax"\nlet a=1;` and `#!nitrogen language Calc\n…` give a path reference and a name reference respectively.
  - `grammar_signature`.
  - `header_variants_keep_offsets`: for a BOM, blank lines, `// #!nitrogen language Calc`, `/* prose\n#!nitrogen language Calc\n*/`, `<!-- #!nitrogen language Calc -->` and CRLF:
    - `masked.len() == text.len()`;
    - every non-header byte is unchanged;
    - header bytes become spaces except line breaks.
  - `errors_are_ngr0401`: `#!nitrogen frob` and `#!nitrogen language "unterminated` each give one diagnostic with Nitrogen's message.
  - `grammar_cannot_combine`: `#!nitrogen grammar` followed by `#!nitrogen language X` gives `incompatible or duplicate Nitrogen document signature`.
  - `augment_is_ignored`.
  - `only_leading_trivia`: `let a=1;\n#!nitrogen language X` has no signatures.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test signed`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Read Nitrogen document signatures"`

### Task 2: Language declarations in the grammar frontend

**Files:**
- Modify: `languages/grammar/Grammar.ngr`:
  - `syntax File = Language:LanguageDeclaration? Modules:Module*;`
  - `LanguageDeclaration`, `LanguageSetting` (name, string, using and presentation settings, as in `Nitrogen.ngr` lines 40–52, adapted to this grammar's tokens).
- Regenerate: `crates/nitride-prepare/src/grammar_seed.rs`
- Modify: `crates/nitride-prepare/src/{lib,parser}.rs`
- Test: `crates/nitride-prepare/tests/signed.rs`

**Interfaces:**
- `pub struct LanguageDeclaration { pub name: String, pub name_span: [usize; 2], pub span: [usize; 2], pub start: Option<String>, pub grammars: Vec<String>, pub extensions: Vec<String> }`.
- `pub fn language_declaration(source: &Source) -> Result<Option<LanguageDeclaration>, PrepareError>`. The source's text is already masked.
- Preparation (strict and tolerant) and the outline accept and ignore the declaration. The outline adds it as a `GrammarItem` of kind `"language"`.

- [ ] **Step 1: Write failing tests.**
  - `declaration_read`: `language Calc { start Calc.Program; grammars "a.ngr", "b.ngr"; extensions ".calc"; namespace X.Y; sources "S.cs"; }` followed by a module gives the expected fields.
  - `preparation_ignores_declaration`: a signed grammar (masked header) prepares and its sample parses.
  - `outline_lists_language`.
- [ ] **Step 2: Run** `cargo test -p nitride-prepare --test signed`. Expected: FAIL.
- [ ] **Step 3: Implement.** Extend the grammar, run bootstrap twice (`cargo run -p nitride-prepare --example bootstrap`, then `-- --check`), then extend the decoder.
- [ ] **Step 4: Run** `cargo test --workspace` and bootstrap `--check`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Read language declarations in grammars"`

### Task 3: Signed languages in the registry

**Files:**
- Modify: `crates/nitride-lsp/src/{registry,manifest}.rs`
- Create: `languages/signed/Portable.syntax`, `languages/signed/Portable.catalog.json` (copy of `languages/portable/catalog.json`), `languages/signed/sample.txt`
- Test: `crates/nitride-lsp/tests/signed_registry.rs`

**Interfaces:**
- `registry::discover(folders: &[PathBuf], open: &dyn Fn(&Path) -> Option<String>) -> Vec<PathBuf>`: the files whose header has a grammar signature, within the scan bounds. Open buffers are included even when they are not on disk yet.
- **Each declaring file** becomes a `LanguageState`:
  - `entry.name` = the declared name;
  - `entry.grammars` = `[the file itself] + grammars` (relative to it);
  - `entry.start`, `entry.extensions`;
  - `entry.catalog` = `<stem>.catalog.json` beside it if it exists; `entry.mode` = typed with a catalog, else syntax;
  - `manifest` = the declaring file. Declaration problems (missing start, unreadable grammar) are reported on the declaring file.
- The declaring file's own text is masked by `signed::read` before preparation.
- `Registry::signed(&self, name: &str) -> Vec<&LanguageState>` finds candidates by declared name. `Registry::signed_at(&self, path: &Path)` finds the language declared by a file.
- `for_path` keeps manifest extensions first, then signed-grammar extensions.
- **`languages/signed/Portable.syntax`:**

  ```
  #!nitrogen grammar
  language Portable { start Portable.Program; grammars "../portable/Portable.ngr"; extensions ".psig"; }
  ```

  It has no modules of its own (allowed).

- [ ] **Step 1: Write failing tests.**
  - `discovers_signed_grammars`: the repository root finds `languages/signed/Portable.syntax`, typed through its sidecar catalog.
  - `sidecar_catalog_makes_typed`.
  - `missing_start_is_reported_on_declaration`.
  - `manifest_and_signature_coexist`: a folder whose `nitride.json` lists a grammar that is also a signed grammar gets one state per declaration source and no duplicate diagnostics.
  - `scan_bounds`: a scratch folder with 2,100 small files plus one 2 MiB file still discovers its signed grammar, and finishes in under 5 s in debug.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test signed_registry`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Discover signed grammars and their languages"`

### Task 4: Selecting languages by header

**Files:**
- Modify: `crates/nitride-lsp/src/{analysis,server}.rs`
- Test: `crates/nitride-lsp/tests/signed_documents.rs`

**Interfaces:**
- `analysis::analyze` reads the header first.
- **A grammar signature:** grammar analysis of the masked text, its language's preparation problems, and header diagnostics.
- **A language signature:** resolves the reference.
  - A path is resolved relative to the document; that file must declare a language (`<path> does not declare a language`).
  - A name is resolved through `Registry::signed`.
  - The program is analysed on the masked text, with header diagnostics and resolution diagnostics on the header span.
- **Neither:** milestone 1 behaviour, then signed-grammar extensions.
- **Server:** an open or changed document whose header has a grammar signature, or a watched file event for such a file, triggers a language reload, as `.ngr` documents do.

- [ ] **Step 1: Write failing tests.**
  - `program_by_path` (`languages/signed/sample.txt` with `#!nitrogen language "./Portable.syntax"`): no diagnostics, tokens present, and an inlay hint `= 42` for `x * 7;`.
  - `program_by_name` (`#!nitrogen language Portable`).
  - `missing_and_ambiguous_names` (two scratch signed grammars both declaring `Calc`): one diagnostic each, on line 0.
  - `signed_grammar_document`: opening `Portable.syntax` gives no error for the header line and outlines the language.
  - `extension_selects_signed_language`: a `.psig` file is analysed as Portable.
  - `editing_signed_grammar_reloads`: change the declared start to an unknown rule; the program gets the start error (on the declaring file); change it back and the program analyses again.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test signed_documents`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Select languages by Nitrogen signatures"`

### Task 5: Extension routing, docs, end-to-end

**Files:**
- Create: `editors/vscode/src/signatures.ts`, porting Nitrogen's `signatureDocumentMode` (grammar and language only).
- Create: `editors/vscode/src/signatures.test.ts`.
- Modify: `editors/vscode/src/extension.ts`:
  - The document selector adds `{ scheme: 'file', pattern: '**/*' }`; the server ignores files it does not serve.
  - Documents whose header is a grammar signature get `languages.setTextDocumentLanguage(document, 'ngr')` on open and change.
  - The watcher becomes `**/*`.
- Modify: `editors/vscode/package.json` (test script), `README.md` and `editors/vscode/README.md` (signed programs; note that augmentations and regions are not supported).
- Modify: `crates/nitride-cli/tests/lsp_binary.rs`: open `languages/signed/sample.txt` and assert no diagnostics and a `= 42` hint.

- [ ] **Step 1: Write failing TS tests:** `grammar header routes to ngr`, `language header is detected`, `prose in a block comment before the header`, `no header`.
- [ ] **Step 2: Implement**, then run `npm test`. Expected: PASS.
- [ ] **Step 3: Run** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` and package the `.vsix`. Expected: PASS.
- [ ] **Step 4: Commit** `git commit -m "Route signed grammars in the extension; document signed programs"`
