# Nitride Language Server, Milestone 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `nitride lsp` plus a separate `nitride` VS Code extension. Together they give diagnostics, semantic coloring, outline, folding and selection ranges for `.ngr` grammars and for workspace-language programs. Workspace grammars are loaded at runtime and reloaded on edit. Nitrogen's extensions stay inactive while Nitride is installed.

**Architecture:**
- Core language tables become lifetime-parameterized (`Language<'a>`), so a grammar prepared at runtime can be built into owned storage and freed when it is replaced.
- Preparation gains a tolerant syntax mode that skips Nitrogen's C# semantic members with warnings.
- A new `nitride-lsp` crate runs a std-threads JSON-RPC loop. It resolves languages from `nitride.json`/`nitrogen.json`, analyses whole documents on change (debounced), and converts byte spans to UTF-16 LSP positions at the boundary.
- A TypeScript extension in `editors/vscode` starts the bundled binary.

**Tech Stack:** Rust 2024 (stable, MSRV 1.85), `serde_json` (outside core only), TypeScript + `vscode-languageclient` 9, `@vscode/vsce`.

**Spec:** `docs/superpowers/specs/2026-10-25-vscode-language-server-design.md`

## Global Constraints

- `nitride-core` keeps zero third-party dependencies. `serde_json` is allowed in `nitride-lsp`, `nitride-cli` and `nitride-prepare` (already there).
- No async runtime; std threads and channels only.
- LSP 3.17 over stdio, `Content-Length` framing, UTF-16 positions on the wire, full document sync.
- Manifest lookup per workspace folder root: `nitride.json`, else `nitrogen.json`. Same schema plus optional `catalog` and `mode` (`typed`/`syntax`). `mode` defaults to `typed` with a catalog, else `syntax`. Paths are relative to the manifest. `nitrogen.json` entries are syntax-only.
- Typed mode and the `nitride prepare` CLI stay strict. Tolerant preparation is only for syntax mode in the server.
- No per-edit `Box::leak`. A replaced runtime language must be freed.
- Extension id `nitride.nitride` (publisher `nitride`), setting `nitride.server.path`, custom request prefix `nitride/` (none in this milestone).
- `contributes.semanticTokenTypes` is copied verbatim from `Nitrogen/editors/vscode/package.json`.
- Local platform-specific `.vsix` only (`darwin-arm64`, `darwin-x64`, `linux-x64`, `win32-x64`), each built on its own host.
- The release profile has `panic = "abort"`, so the server must not panic on any input. Errors become diagnostics or LSP error responses.
- Existing behaviour is unchanged. `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `cargo run -p nitride-prepare --example bootstrap -- --check` pass after every task.

## Review Focus

1. **Grammar edited into a broken state while a program using it is open:** the program keeps its last good language and results, and the `.ngr` shows the error. Test: Task 6, `broken_grammar_keeps_last_language`.
2. **Non-ASCII text (`é`, emoji / surrogate pairs) before a diagnostic or token:** positions are correct UTF-16 columns. Test: Task 4, `utf16_columns_count_surrogate_pairs`, and Task 7, `tokens_after_emoji`.
3. **CRLF files:** lines split on `\r\n`, and `\r` never counts as a column. Test: Task 4, `crlf_lines`.
4. **Malformed `nitride.json`, missing grammar path, or an unknown start rule:** the server keeps running and publishes a diagnostic on the manifest. Other languages still work. Test: Task 5, `bad_manifest_reports_and_continues`.
5. **A huge or deeply nested program (CSharp4 corpus file, 10k nested parens):** analysis returns limit diagnostics within the debounce budget and never aborts the process. Test: Task 6, `deep_nesting_is_a_diagnostic`.

---

### Task 1: Lifetime-parameterize the language model

No behaviour change. Generated tables become `Language<'static>`. Runtime tables (Task 2) borrow owned storage.

**Files:**
- Modify: `crates/nitride-core/src/{grammar,parser,syntax,semantic,templates,execution,lib}.rs`, `crates/nitride-core/src/recovery/*.rs`
- Modify: `crates/nitride-codegen/src/lib.rs` (emit `Language<'static>`)
- Modify: `crates/nitride-cli/src/{lib,recovery}.rs`, `crates/nitride-cli/src/bin/profile.rs`, `crates/nitride-prepare/src/parser.rs`
- Regenerate: `crates/nitride-prepare/src/grammar_seed.rs` via bootstrap
- Test: existing suites, plus `crates/nitride-core/tests/borrowed_language.rs`

**Interfaces:**
- Produces:
  - `Expr<'a>`, `Action<'a>`, `Rule<'a>`, `Builtin<'a>`, `BuiltinValue<'a>`, `Check<'a>`, `Hole<'a>`, `Arm<'a>`, `Helper<'a>`, `Signature<'a>`, `Language<'a>`. Every `&'static` becomes `&'a`, and the types stay covariant in `'a`.
  - `SyntaxTree<'a>`, `Program<'a>`, `RecoveredParse<'a>`, `RecoveredAnalysis<'a>`.
  - `parse`, `parse_with_options`, `parse_recovering`, `compile`, and `compile_recovering` take `&'a Language<'a>`.
  - `Diagnostic` stays lifetime-free: `code: Option<&'static str>` becomes `code: Option<Box<str>>`.

- [ ] **Step 1: Write failing test `borrowed_language_parses`.** In `crates/nitride-core/tests/borrowed_language.rs`, build a `Language` inside a function from a local `Vec<Rule>`. The rules are `Doc = Number*` and `token Number = ['0'..'9']+`, built the way `tests/predicates.rs` builds rules but without `Box::leak`. Parse `"12 34"`, assert `tree.nodes.len() == 3`, then drop the vec.
- [ ] **Step 2: Run** `cargo test -p nitride-core --test borrowed_language`. Expected: FAIL to compile (`borrowed value does not live long enough` / `'static` required).
- [ ] **Step 3: Introduce the lifetime parameter** across core, codegen output and callers. Codegen emits `pub static LANGUAGE: Language<'static> = …`. Run `cargo run -p nitride-prepare --example bootstrap`, then `-- --check`.
- [ ] **Step 4: Run** `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo run -p nitride-prepare --example bootstrap -- --check`. Expected: all pass, and the CLI observations for every `languages/*/sample.*` are byte-identical to before. Compare with `scripts/` outputs, or save `nitride <flag> <sample>` JSON before Step 3 and `diff` after.
- [ ] **Step 5: Commit** `git commit -m "Parameterize language tables by lifetime"`

### Task 2: Build languages at runtime from prepared data

**Files:**
- Modify: `crates/nitride-codegen/src/lib.rs`. Split `generate` into `fn model(input: &str) -> Result<Model, String>` (all current validation and derivation: ids, aliases, memoize points, surrogate pairing, progress validation) and `fn emit(&Model) -> String`.
- Create: `crates/nitride-codegen/src/runtime.rs`
- Test: `crates/nitride-codegen/tests/runtime.rs`

**Interfaces:**
- Consumes: `Language<'a>` from Task 1.
- Produces:
  - `pub struct LoadedLanguage`: owns its storage, is `Send + Sync`, and frees everything on drop.
  - `impl LoadedLanguage { pub fn load(prepared_json: &str) -> Result<LoadedLanguage, String>; pub fn language(&self) -> &Language<'_>; }`
  - `generate(input)` keeps its signature and output byte-for-byte.

- [ ] **Step 1: Write failing test `runtime_matches_generated`.** For each profile in `crates/nitride-cli/build.rs`'s list:
  - prepare `languages/<p>/project.json` with `nitride_prepare::prepare_project`;
  - `LoadedLanguage::load(&data.to_string())`;
  - parse every `languages/<p>/sample.*` (and two files from `languages/csharp4/corpus` for csharp4) with both the loaded language and the matching static table (`nitride_cli` exposes them; add `pub fn static_language(profile: &str) -> Option<&'static Language<'static>>` in `crates/nitride-cli/src/lib.rs`);
  - assert equal `nodes`, `edges`, `root`, rule names per node, and labels per edge.
- [ ] **Step 2: Write failing test `generate_unchanged`.** `generate(prepared)` equals the checked-in `grammar_seed.rs` for the grammar profile. This is the bootstrap fixed point, run as a test.
- [ ] **Step 3: Run** `cargo test -p nitride-codegen --test runtime`. Expected: FAIL (`LoadedLanguage` not found).
- [ ] **Step 4: Implement `model`/`emit`, then `LoadedLanguage`.** Storage is owned `Box<[…]>` arenas. `language()` returns the internal `Language<'static>` shortened to `&Language<'_>` (sound because `Language` is covariant). This is the only `unsafe` in the workspace: one block in `runtime.rs` with a `// SAFETY:` comment explaining that the boxes are never moved or mutated after construction and that no `'static` reference escapes. Field order guarantees `language` drops before storage.
- [ ] **Step 5: Run** `cargo test -p nitride-codegen && cargo test --workspace`. Expected: PASS.
- [ ] **Step 6: Run** `cargo +nightly miri test -p nitride-codegen --test runtime -- runtime_matches_generated --exact` restricted to the `portable` profile (gate the others behind `cfg(not(miri))`). Expected: no UB reported. If nightly/miri is unavailable, record that in the commit message and continue.
- [ ] **Step 7: Commit** `git commit -m "Load prepared languages at runtime"`

### Task 3: Tolerant syntax-mode preparation

**Files:**
- Modify: `languages/grammar/Grammar.ngr`. Add the module member `SymbolProperty`, and allow `PropertyDecl` members before the alternatives in `ExtensibleRule` and `Extend` bodies. Add the tokens `CsType` and `CsCode` and their helpers (`CsString`, `CsChar`, `CsComment`, `CsNested`), copied from `Nitrogen/Nitrogen.Ngr/Nitrogen.ngr` lines 25–29, 152, 156, 158, with names prefixed for this grammar where they clash.
- Regenerate: `crates/nitride-prepare/src/grammar_seed.rs`
- Modify: `crates/nitride-prepare/src/{parser,normalize,lib}.rs`
- Test: `crates/nitride-prepare/tests/tolerant.rs`

**Interfaces:**
- Produces, in `nitride_prepare`:
  - `pub struct Prepared { pub data: serde_json::Value, pub warnings: Vec<PrepareError>, pub declarations: Vec<Declaration> }`
  - `pub struct Declaration { pub rule: String, pub field: String, pub kinds: Vec<String> }`, one per `declares` clause in either mode. `rule` is the qualified rule name, as in prepared `rules[].name`.
  - `pub fn prepare_sources_tolerant(sources: &[Source], catalog: &str, start: &str) -> Result<Prepared, PrepareError>`, always syntax mode.
  - `pub fn grammar_outline(source: &Source) -> Result<Vec<GrammarItem>, PrepareError>`, with `pub struct GrammarItem { pub kind: &'static str /* "module"|"syntax"|"token"|"extensible"|"extend"|"function"|"helper" */, pub name: String, pub span: [usize; 2], pub name_span: [usize; 2], pub children: Vec<GrammarItem> }`. Used by Task 8.
  - The existing `prepare_sources`/`prepare_project` (strict) error on these members with the message `C# semantic member is not admitted`.

- [ ] **Step 1: Write failing tests.**
  - `calls_reference_prepares_tolerantly`: tolerant-prepare `languages/calls/Calls.Reference.ngr` with start `Calls.Program` and catalog `{"types":[],"operations":[],"statements":{}}`. Assert `Ok`, `warnings.len() >= 1`, the first warning at line 5 column 3 with message `C# semantic member is not admitted`, and a `declarations` entry `{rule:"Calls.Let", field:"Name", kinds:["value"]}`. Then `LoadedLanguage::load` the data and parse `languages/calls/sample.calls` successfully.
  - `dates_reference_prepares_tolerantly`: the same shape for `languages/dates/Dates.Reference.ngr`, its start rule and `languages/dates/sample.dates`.
  - `strict_rejects_semantic_member`: `prepare_sources` on Calls.Reference fails at 5:3 with message `C# semantic member is not admitted`. Previously it failed with `invalid source`.
  - `tolerant_reports_each_member`: a grammar with two `symbol property` lines and one `out` member gives exactly 3 warnings, in source order.
  - `grammar_outline_nests_rules`: the outline of `languages/calls/Calls.ngr` is one `module` "Calls" whose children include `token` "Number" and `extensible` "Expr", and `name_span` covers exactly the name text.
- [ ] **Step 2: Run** `cargo test -p nitride-prepare --test tolerant`. Expected: FAIL.
- [ ] **Step 3: Extend `Grammar.ngr`, run bootstrap, and implement.** The decoder records skipped members instead of erroring when tolerant. Rule semantic bodies (`End = Semantics`) are already omitted in syntax mode (`normalize.rs` `omitted_semantics`), so keep that.
- [ ] **Step 4: Run** `cargo test --workspace && cargo run -p nitride-prepare --example bootstrap -- --check`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Prepare Nitrogen grammars tolerantly in syntax mode"`

### Task 4: `nitride-lsp` crate: transport, positions, lifecycle

**Files:**
- Modify: `Cargo.toml` (workspace member), `crates/nitride-cli/Cargo.toml`, `crates/nitride-cli/src/main.rs` (`lsp` subcommand)
- Create: `crates/nitride-lsp/Cargo.toml` (deps: nitride-core, nitride-prepare, nitride-codegen, serde_json)
- Create: `crates/nitride-lsp/src/{lib,rpc,lines,server}.rs`
- Test: `crates/nitride-lsp/tests/{lifecycle,lines}.rs`, `crates/nitride-lsp/tests/support/mod.rs`

**Interfaces:**
- Produces:
  - `pub struct ServeOptions { pub debounce: std::time::Duration }`. `Default` is 150 ms. Tests use `Duration::ZERO`.
  - `pub fn serve(input: impl std::io::BufRead + Send + 'static, output: impl std::io::Write, options: ServeOptions) -> std::io::Result<()>`. It returns after `exit`, and pending analyses finish before the `shutdown` response.
  - `lines::LineIndex::new(text: &str)`; `fn position(&self, byte: u32) -> (u32 /*line*/, u32 /*utf16 col*/)`; `fn offset(&self, line: u32, utf16_col: u32) -> u32`. Bytes inside a character clamp to its start.
  - Test support: `Session::run(root: &Path, messages: Vec<serde_json::Value>) -> Vec<serde_json::Value>`. It frames the messages, appends shutdown/exit, runs `serve` in-process, and returns every server message.
- Capabilities in the `initialize` result: `textDocumentSync: 1`, and `positionEncoding: "utf-16"`. Later tasks add their own capability.

- [ ] **Step 1: Write failing tests.**
  - `initialize_shutdown_exit`: the response ids match and `serverInfo.name == "nitride"`.
  - `unknown_request_is_method_not_found`: error code `-32601`.
  - `malformed_frame_is_parse_error`: error code `-32700`, and the next valid request is still answered.
  - Lines: `utf16_columns_count_surrogate_pairs` (`"a😀b"`: byte 5 → col 3); `crlf_lines` (`"a\r\nb"`: byte 3 → (1,0), and offset(0,5) clamps to 1); `offset_roundtrip` over a mixed-script string.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp`. Expected: FAIL.
- [ ] **Step 3: Implement.** A reader thread feeds a channel. The main loop handles messages and runs debounced work when the channel is idle. Add `nitride lsp` to `main.rs` (stdin/stdout, `ServeOptions::default()`).
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`, then pipe an `initialize` frame into `./target/debug/nitride lsp`. Expected: PASS, and a framed JSON response on stdout.
- [ ] **Step 5: Commit** `git commit -m "Add nitride lsp transport and lifecycle"`

### Task 5: Workspace manifests and language registry

**Files:**
- Create: `crates/nitride-lsp/src/{manifest,registry}.rs`
- Create: `nitride.json` at the repo root. It lists every `languages/*/project.json` language with Nitride's own grammars (e.g. `Calls.ngr`), `start`, `catalog` and `mode` copied from each `project.json`, plus `extensions` from the existing `nitrogen.json` entries or the sample's extension (`.nitridegrammar` for the grammar language).
- Test: `crates/nitride-lsp/tests/workspace.rs`, fixtures under `crates/nitride-lsp/tests/fixtures/`

**Interfaces:**
- Produces:
  - `manifest::Entry { name: String, extensions: Vec<String>, grammars: Vec<PathBuf>, start: String, catalog: Option<PathBuf>, mode: Mode }`, with `enum Mode { Typed, Syntax }`.
  - `manifest::read(folder: &Path) -> (Option<PathBuf> /*manifest path*/, Result<Vec<Entry>, ManifestError>)`, where `ManifestError { span: [u32; 2], message: String }`.
  - `registry::Registry::reload(&mut self, folders: &[PathBuf], open: &dyn Fn(&Path) -> Option<String>)`. Open buffers override disk text.
  - `registry.for_path(&Path) -> Option<&LanguageState>`, matching by file extension and first manifest wins.
  - `LanguageState { entry: Entry, current: Option<Arc<Loaded>>, errors: Vec<PrepareError>, warnings: Vec<PrepareError> }`, where `Loaded { language: LoadedLanguage, prepared: Prepared }`.
  - Typed mode uses strict `prepare_sources` with the catalog. Syntax mode uses `prepare_sources_tolerant` with the catalog, or the empty catalog `{"types":[],"operations":[],"statements":{}}`.
  - When preparation fails, `current` keeps the previous `Arc<Loaded>` and `errors` holds the failure.
- Server: `workspace/didChangeWatchedFiles` and `.ngr`/catalog/manifest buffer changes trigger `reload`. Programs open in the affected language are re-analysed (Task 6).

- [ ] **Step 1: Write failing tests.**
  - `prefers_nitride_json`: with both files, the entries come from `nitride.json`.
  - `falls_back_to_nitrogen_json`: entries come from `nitrogen.json` with `mode == Syntax` and `catalog == None`, and `sources` is ignored.
  - `mode_defaults_from_catalog`.
  - `bad_manifest_reports_and_continues`: a manifest with a trailing comma, opened via `didOpen`, gets a diagnostic on the manifest's URI at the JSON error offset. A second folder's valid language still analyses.
  - `missing_grammar_is_manifest_diagnostic`: the message names the path.
  - `repo_manifest_loads`: the repo `nitride.json` loads every entry with `current.is_some()` and no errors.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test workspace`. Expected: FAIL.
- [ ] **Step 3: Implement** `manifest.rs`, `registry.rs`, and the repo `nitride.json`. Advertise `workspace.workspaceFolders.supported`.
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Read nitride.json and load workspace languages"`

### Task 6: Document analysis and diagnostics

**Files:**
- Create: `crates/nitride-lsp/src/{analysis,diagnostics}.rs`
- Test: `crates/nitride-lsp/tests/diagnostics.rs`

**Interfaces:**
- Consumes: `Registry`, `LineIndex`, `parse_recovering`, `compile_recovering`, `prepare_sources_tolerant`.
- Produces:
  - `analysis::Analysis { version: i32, text: String, lines: LineIndex, tree: Option<OwnedTree>, diagnostics: Vec<lsp Diagnostic JSON>, declarations: Vec<DeclaredName> }`.
  - `DeclaredName { name: String, kind: String, node_span: [u32; 2], name_span: [u32; 2] }`. It is computed by walking the tree for nodes whose rule has a `Declaration` (from `Prepared.declarations`) and reading the labelled field's span. It is used by Tasks 7 and 8.
  - `OwnedTree` holds `Arc<Loaded>` alongside the tree, so the tree never outlives its language. Struct field order makes the tree drop first. Use the Task 2 pattern if a self-reference is needed, or store nodes/edges/rule names as owned data.
- Diagnostic mapping:
  - severity Error for parse/recovery/semantic diagnostics, Warning for tolerant-prepare warnings;
  - `source: "nitride"`;
  - `code` from `Diagnostic.code`;
  - message unchanged.
- `.ngr` documents: tolerant-prepare the language(s) whose `grammars` include this file, using open buffer text. Publish that language's `errors` and `warnings` located in this file. A `.ngr` in no manifest is prepared alone, with start = the first syntax rule. Also parse it with the built-in grammar language (`NitrideGrammar.File`, via `static_language`) for the tree.
- Limits: use `Limits::default()`. A `limit` category diagnostic is published like any other.

- [ ] **Step 1: Write failing tests** (each one an in-process session over fixture folders):
  - `program_syntax_error`: a `.powercalc` with `let x = ;` gets one Error diagnostic whose range starts at the `;`, at line/col from `LineIndex`.
  - `typed_semantic_error`: a `.calls` calling an unknown function gets the semantic diagnostic, with its catalog `code` where one exists.
  - `reference_grammar_warnings`: opening `languages/calls/Calls.Reference.ngr` under the fallback `nitrogen.json` fixture publishes Warnings `C# semantic member is not admitted`, and a `.calls` file in that folder gets no grammar errors.
  - `broken_grammar_keeps_last_language`: open `Power.ngr` and `sample.powercalc`, then change `Power.ngr` to invalid text. The `.ngr` gets the error, the program's diagnostics are unchanged, and no crash occurs. Fix the grammar, and the program re-analyses against the new language.
  - `deep_nesting_is_a_diagnostic`: a program with 10 000 nested `(` gets a diagnostic, and the session completes.
  - `close_clears_diagnostics`: `didClose` publishes an empty list.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test diagnostics`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Publish diagnostics for programs and grammars"`

### Task 7: Semantic tokens

**Files:**
- Create: `crates/nitride-lsp/src/tokens.rs`
- Test: `crates/nitride-lsp/tests/tokens.rs`

**Interfaces:**
- Legend (`semanticTokensProvider.legend`): `tokenTypes = ["namespace","type","function","variable","parameter","property","keyword","operator","number","string","comment"]`, `tokenModifiers = ["declaration","defaultLibrary"]`. Only `full: true`.
- Classification (a simplified version of Nitrogen's `SemanticTokenBuilder.Classify`; HIR/palette colouring is milestone 2 and 4):
  1. Declared names (`Analysis.declarations[].name_span`) become `variable` + `declaration`. For `.ngr`, `GrammarItem.name_span` becomes `type` for syntax/extensible rules, `function` for functions/helpers, `namespace` for modules, and `type` for tokens.
  2. Literal text: text inside a node's span not covered by its children and not whitespace/comment. A word made of `[A-Za-z_][A-Za-z0-9_]*` becomes `keyword`, anything else `operator`.
  3. Leaf nodes whose rule's last name segment contains `Number`, `Digit` or `Integer` become `number`, and those containing `String` or `Char` become `string`.
  4. `//…` to end of line and `/*…*/` in gaps become `comment`, split per line.
  5. Tokens are sorted, non-overlapping, and multi-line tokens are split per line.

- [ ] **Step 1: Write failing tests.**
  - `calls_sample_tokens`: decode the relative encoding for `let x=duration(4);`. Expect `let`→keyword, `x`→variable+declaration, `=` `(` `)` `;`→operator, `4`→number.
  - `ngr_tokens`: in `Calls.ngr`, `syntax`→keyword, `Program`→type, `"let"`→string (from the grammar's own `String` token), and `// comment`→comment.
  - `tokens_after_emoji`: a comment containing `😀` followed by a token on the same line yields the correct UTF-16 start column.
  - `tokens_without_language_are_empty`: a `.txt` file returns `{"data": []}`.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test tokens`. Expected: FAIL.
- [ ] **Step 3: Implement** and advertise the capability.
- [ ] **Step 4: Run** `cargo test -p nitride-lsp`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Semantic tokens for programs and grammars"`

### Task 8: Outline, folding, selection ranges

**Files:**
- Create: `crates/nitride-lsp/src/structure.rs`
- Test: `crates/nitride-lsp/tests/structure.rs`

**Interfaces:**
- `textDocument/documentSymbol` returns hierarchical `DocumentSymbol[]`.
  - Programs: `DeclaredName`s, nested under the nearest declaration whose `node_span` contains theirs (Nitrogen's `OutlineBuilder`). Symbol kind is `Variable` (13); for a `function` kind, `Function` (12).
  - `.ngr`: `GrammarItem` tree. module→`Namespace` (3), syntax/extensible/extend→`Class` (5), token→`Constant` (14), function/helper→`Function` (12).
- `textDocument/foldingRange`, following Nitrogen's `NitrogenLanguageService.Structure.cs`:
  - each node spanning two or more lines folds from its start line to its end line, unless the end line equals the start line, deduplicated by start line with the outermost kept;
  - a run of two or more consecutive comment lines folds as `kind: "comment"`.
- `textDocument/selectionRange`: for each position, the chain of node spans from innermost to root, deduplicating equal spans.

- [ ] **Step 1: Write failing tests.**
  - `program_outline`: two `let`s in `.calls` give two top-level symbols with names and ranges.
  - `ngr_outline`: `Calls.ngr` gives module `Calls` with children including `Expr` as Class.
  - `folds_multiline_rule`: the extensible `Expr` block in `Calls.ngr` folds from its `extensible` line to its closing `}` line.
  - `comment_block_folds`.
  - `selection_climbs`: at `4` in `duration(4)` the chain is `4` ⊂ `duration(4)` ⊂ … ⊂ whole document.
- [ ] **Step 2: Run** `cargo test -p nitride-lsp --test structure`. Expected: FAIL.
- [ ] **Step 3: Implement** and advertise `documentSymbolProvider`, `foldingRangeProvider`, `selectionRangeProvider`.
- [ ] **Step 4: Run** `cargo test --workspace && cargo clippy --workspace -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Outline, folding and selection ranges"`

### Task 9: VS Code extension

**Files:**
- Create: `editors/vscode/{package.json,tsconfig.json,.vscodeignore,.gitignore,README.md,LICENSE,language-configuration.json}`
- Create: `editors/vscode/syntaxes/ngr.tmLanguage.json` (copied from Nitrogen)
- Create: `editors/vscode/src/{extension,server,manifest}.ts`, `editors/vscode/src/{server,manifest}.test.ts`
- Create: `scripts/package-vscode.sh`

**Interfaces:**
- `package.json`:
  - `name: "nitride"`, `publisher: "nitride"`, `displayName: "Nitride DSL"`, `version: "0.1.0"`, `engines.vscode: "^1.90.0"`;
  - activation `onLanguage:ngr`, `workspaceContains:nitride.json`, `workspaceContains:nitrogen.json`;
  - language `ngr` (`.ngr`), grammar `source.ngr`;
  - setting `nitride.server.path` (string, default `""`);
  - `semanticTokenTypes` verbatim from Nitrogen;
  - dependency `vscode-languageclient` at the same version as Nitrogen's `package.json`.
- `server.ts`: `serverCommand({ serverPath, extensionPath, platform, exists }): { command: string; args: string[] }`. Resolution order: the setting; then the bundled `server/nitride` (`nitride.exe` on win32); then `nitride` on PATH. `args` is always `["lsp"]`.
- `manifest.ts`: `manifestExtensions(folderReader: (name: string) => string | undefined): string[]`. It reads `nitride.json`, else `nitrogen.json`, and returns file extensions (with dot, deduplicated). The client's document selector is `{language:'ngr'}` plus `{scheme:'file', pattern:'**/*<ext>'}` per extension, with `synchronize.fileEvents` watching `**/{nitride.json,nitrogen.json,*.ngr,*.json}`.
- `scripts/package-vscode.sh <vsce-target>`: `cargo build --release -p nitride-cli`, copy the binary to `editors/vscode/server/`, `npm ci && npm run compile && npx vsce package --target <vsce-target>`.

- [ ] **Step 1: Write failing tests** (`npm test`, mocha like Nitrogen's):
  - `server: setting wins`, `server: bundled binary`, `server: windows exe name`, `server: PATH fallback`;
  - `manifest: prefers nitride.json`, `manifest: falls back to nitrogen.json`, `manifest: invalid JSON yields []`.
- [ ] **Step 2: Run** `cd editors/vscode && npm install && npm test`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `npm test`, then `scripts/package-vscode.sh darwin-arm64`. Expected: tests pass, and `editors/vscode/nitride-darwin-arm64-0.1.0.vsix` exists and contains `server/nitride` (`unzip -l`).
- [ ] **Step 5: Commit** `git commit -m "Add the Nitride VS Code extension"`

### Task 10: Nitrogen steps aside (Nitrogen repository)

Work in `/Users/dmitrikamenetski/work/Nitrogen` on a new branch `nitride-yields`. Do not touch its untracked files.

**Files:**
- Create: `editors/vscode/src/nitride.ts`, exporting `nitrideInstalled(getExtension: (id: string) => unknown): boolean`, true when `getExtension('nitride.nitride')` is defined.
- Modify: `editors/vscode/src/extension.ts`. At the top of `activate`, if `nitrideInstalled(id => extensions.getExtension(id))`, return before creating the client, C# strings, extraction or routing.
- Test: `editors/vscode/src/nitride.test.ts`
- Modify: `Nitrogen.Cli/VsCode/VsCodeRenderer.cs`. The generated `extension.ts` gets the same early return.
- Test: the existing VsCodeRenderer tests in `Nitrogen.Tests` (find with `grep -rl VsCodeRenderer Nitrogen.Tests`). Add `GeneratedExtensionYieldsToNitride`, asserting the rendered `extension.ts` contains `getExtension('nitride.nitride')` before `new LanguageClient(`.

- [ ] **Step 1: Write failing tests** (`nitride.test.ts`: installed → true, absent → false; plus the C# renderer test).
- [ ] **Step 2: Run** `cd editors/vscode && npm test` and `dotnet test Nitrogen.Tests --filter GeneratedExtensionYieldsToNitride`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** both again, plus the full `dotnet test Nitrogen.Tests`. Expected: PASS.
- [ ] **Step 5: Commit** in Nitrogen: `git commit -m "Stay inactive while the Nitride extension is installed"`

### Task 11: End-to-end check and docs

**Files:**
- Modify: `README.md`, adding an "Editor support" section: build and install the `.vsix`, the `nitride.json` format, the fallback rule, and what milestone 1 covers.
- Create: `crates/nitride-lsp/tests/binary.rs`

- [ ] **Step 1: Write test `binary_session`.** Spawn `env!("CARGO_BIN_EXE_nitride")`, or locate `target/<profile>/nitride`, with `lsp`. Run initialize, didOpen `languages/power/sample.powercalc` with the repo root as workspace folder, then `semanticTokens/full`, `documentSymbol`, shutdown, exit. Assert a `publishDiagnostics` arrives with an empty list, the tokens are non-empty, and the exit code is 0.
- [ ] **Step 2: Run** `cargo test --workspace`. Expected: PASS.
- [ ] **Step 3: Manual check, and record the results in the commit message.**
  - Install the Nitride `.vsix` and the Task 10 Nitrogen extension build in VS Code.
  - Open this repo and confirm diagnostics, coloring, outline and folding on `languages/calls/Calls.ngr`, `languages/calls/sample.calls` and `languages/xml/sample.xml`, with exactly one set of results (no duplicates).
  - Uninstall Nitride, reload, and confirm Nitrogen serves again.
- [ ] **Step 4: Commit** `git commit -m "Document editor support; end-to-end LSP test"`
