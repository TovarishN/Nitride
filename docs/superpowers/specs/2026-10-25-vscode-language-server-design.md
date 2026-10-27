# Nitride language server and VS Code extension

Status: draft for user approval (2026-10-25), revised after the builder check
and the user's answers. No implementation is included.

## Intent

Give Nitride the editor experience Nitrogen already has: full language support
in VS Code for `.ngr` grammars and for the programs of workspace languages,
served by Nitride's own Rust engine with no .NET runtime. The user wants
Nitride to be the language support they actually use; when both are installed,
Nitride wins, even where it is less capable than Nitrogen.

## Agreed with the user

- **Editor:** VS Code. Not full Visual Studio, not Rider.
- **Separate extension.** A new `nitride` VS Code extension that bundles the
  native `nitride` binary. It is not a drop-in server for Nitrogen's extension.
- **Feature target:** everything Nitrogen's language server does today, except
  debugging and emit (neither project has them).
- **Priority:** when both extensions are installed, Nitride owns `.ngr` files
  and workspace languages. To use Nitrogen, the user uninstalls Nitride.
  Nitrogen's main extension and the per-language extensions that Nitrogen's
  generator produces (`nitrogen.nitrogen-*`) are both changed so they stay
  inactive while Nitride is installed.
- **Tolerant preparation.** In syntax mode, preparation skips C# semantic
  content it cannot admit (semantic blocks, `symbol property … : <C# type>`
  members and similar), reports one diagnostic at each skipped construct, and
  still builds a working parser from the rest. This lets the `nitrogen.json`
  fallback serve real Nitrogen languages such as `Calls.Reference.ngr` and
  `Dates.Reference.ngr`, which today fail at the first such line. It is part
  of milestone 1.
- **Workspace manifest:** Nitride reads `nitride.json` at a workspace folder
  root and falls back to `nitrogen.json` when it is absent. Nitrogen never reads
  `nitride.json`. The format is the same as `nitrogen.json`; fields Nitride does
  not use (such as C# `sources`) are ignored. This lets a workspace list a
  different language set for Nitride without affecting Nitrogen.
- **Milestones, in this order**, each shipping a usable extension:
  1. Server, extension, diagnostics, semantic coloring, outline, folding and
     selection ranges, for grammars and programs.
  2. Navigation: hover, definition/declaration, references, document
     highlights, rename (with prepareRename), workspace symbols.
  3. Editing help: completion, signature help, formatting (document, range,
     on-type), inlay hints, code actions.
  4. Nitrogen's extras: palette tokens, signed programs (the
     `nitrogen-signature` routing), the Extract Block and Extract Quote
     Helper commands. (Languages inside tagged C# strings were listed here
     and dropped on 2026-10-26; see Non-goals.)

## Non-goals

- Debugging (DAP) and emit/code generation commands.
- Admitting new Nitrogen semantics to the engine just for the editor. Skipped
  C# semantic content gives no types or navigation; the editor reflects what
  Nitride admits. Unsupported constructs surface as diagnostics, not as
  silent gaps.
- Languages inside tagged C# strings (`/*lang=calc*/ "…"`, `// language=calc`).
  The user decided on 2026-10-26 that Nitride does not need them; C# files
  stay with the C# extension.
- Full Visual Studio or Rider plugins.
- Publishing to the VS Code Marketplace (a local `.vsix` is the deliverable).

## Anti-goals

- Requiring .NET, Node, or anything beyond the bundled binary for the server.
- Duplicate results in the editor when both extensions are installed.
- Pulling third-party dependencies into `nitride-core`. The core stays
  dependency-free.
- A server that leaks memory or slows down over a long editing session
  (for example, a grammar re-prepared on every keystroke that is never freed).

## Current state (evidence)

- Nitrogen's server: `Nitrogen.LanguageService` (~7.8k lines C#), JSON-RPC
  over stdio in `Lsp/LspServer.cs`. Handled methods include completion,
  hover, definition, declaration, references, documentHighlight,
  documentSymbol, foldingRange, selectionRange, formatting, rangeFormatting,
  onTypeFormatting, rename, prepareRename, inlayHint, codeAction,
  signatureHelp, semanticTokens/full, workspace/symbol, file watching, and the
  custom `nitrogen/prepare|resolveGrammarExtraction` and
  `nitrogen/prepare|resolveQuoteHelperExtraction` requests. Grammar edits take
  effect without restarting the server.
- Nitrogen's VS Code client: `Nitrogen/editors/vscode` (TypeScript,
  `vscode-languageclient`). It finds the server through `nitrogen.server.path`
  or a bundled dll, reads `nitrogen.json` for file extensions, yields
  languages owned by generated `nitrogen.nitrogen-*` plugins, paints tagged C#
  string tokens as decorations, and routes signed documents. Its
  `semanticTokenTypes` contributions (operationWord, declarationKeyword, …)
  define the custom token legend.
- Nitride today compiles selected languages ahead of time into
  `&'static Language` tables (`nitride-core/src/grammar.rs`). Runtime language
  loading was explicitly deferred in `docs/design.md`, not prohibited.
  `nitride-prepare` already parses `.ngr` and catalogs in Rust at runtime and
  reports grammar errors with file, line, column and byte span. Tests already
  build `Language` values at runtime with `Box::leak`.
- Nitride has a syntax-mode grammar for `.ngr` itself (`languages/grammar`,
  start `NitrideGrammar.File`).
- Diagnostics carry `[u32; 2]` byte spans; `Program::origins` maps HIR nodes
  back to source spans. Recovery (`nitride-core/src/recovery`) produces trees
  for broken input. There is no symbol index, no incremental reanalysis, and
  no position-to-node query API yet.
- Typed semantics come from a per-language `catalog.json` (see each
  `languages/*/project.json`), not from C# sources.

## Design

### Server

- New command `nitride lsp`: LSP 3.17 over stdio, JSON-RPC with
  `Content-Length` framing, UTF-16 positions on the wire (convert from Nitride's
  UTF-8 byte spans at the boundary).
- Lives in a new workspace crate (builder's call on name, e.g. `nitride-lsp`)
  that depends on `nitride-core` and `nitride-prepare`; `nitride-cli` adds the
  `lsp` subcommand. `serde_json` is acceptable here, as it already is in the
  CLI. Prefer std threads over an async runtime.
- **Runtime language loading.** The server prepares workspace languages from
  their grammars at runtime with `nitride-prepare`, builds a `Language` from
  the prepared data, and re-prepares when a grammar or catalog changes (open
  buffer edits and file watching), without a restart. Changing `Language` to
  support owned or reference-counted tables is in scope; repeated
  `Box::leak` per edit is not acceptable (see anti-goals).
- **Manifest.** For each workspace folder: `nitride.json` if present, else
  `nitrogen.json`. Per language: `name`, `extensions`, `grammars`, `start`.
  Nitride additionally reads optional `catalog` and `mode` (`typed` or
  `syntax`, as in `languages/*/project.json`; my call). `mode` defaults to
  `typed` when a catalog is given and `syntax` otherwise; syntax-only
  languages get parse, recovery diagnostics, structure and coloring, but no
  types or binding-based navigation (the server synthesizes an empty catalog).
  Paths are relative to the manifest. `nitrogen.json` entries carry neither,
  so they are syntax-only.
- This repository ships a `nitride.json` listing Nitride's own grammars
  (`Calls.ngr`, not `Calls.Reference.ngr`) with their catalogs and modes, so
  opening the repo exercises typed mode (my call).
- **Tolerant syntax-mode preparation** (milestone 1, in `nitride-prepare`):
  where a construct belongs to Nitrogen's C# semantics rather than syntax,
  skip it to its natural end (the member or block boundary), record a
  warning-level diagnostic with file and span, and continue. Typed mode and
  the existing `nitride prepare` CLI keep their strict behavior; the
  tolerant path is used by the server for syntax-mode languages and for
  `.ngr` diagnostics. A grammar that still fails for other reasons reports
  that error and the language gets no parser. Evidence: both
  `languages/calls/Calls.Reference.ngr` and
  `languages/dates/Dates.Reference.ngr` prepare in tolerant syntax mode and
  parse their samples.
- **`.ngr` files** are served through the built-in grammar language
  (`NitrideGrammar.File`) for structure and coloring, plus `nitride-prepare`
  for grammar diagnostics and, from milestone 2, rule navigation.
- **Diagnostics** on open/change, from parse/recovery and, in typed mode,
  semantic compile. Unsupported Nitrogen constructs are reported as
  diagnostics at their source location.
- **Semantic tokens** use the same legend as Nitrogen (standard types plus
  Nitrogen's custom types) so themes and expectations carry over.
- **Editor queries** (milestones 2–3) need engine support Nitride lacks:
  position→node lookup, a declaration/use index over binding results, and
  type information for hover. These are added to `nitride-core` as editor-
  neutral APIs. Behaviour should match Nitrogen's for the same input wherever
  Nitride admits that input; Nitrogen's `Nitrogen.LanguageService` is the
  reference for what each feature returns.
- **Incremental work:** full reanalysis per change is acceptable for
  milestone 1 if it stays responsive on the repository's samples and the
  CSharp4 corpus files; edits are debounced. Incremental reuse is the
  separate shared-subtrees effort and is not a dependency.
- Custom requests keep Nitrogen's method shapes under a `nitride/` prefix
  (milestone 4).

### Extension

- Lives in this repository at `editors/vscode`. Extension id
  `nitride.nitride` (publisher `nitride`).
- Bundles per-platform `nitride` binaries (macOS arm64/x64, Linux x64,
  Windows x64) built with `cargo build --release`; a `nitride.server.path`
  setting overrides the bundled binary, and an unpackaged development build
  falls back to `nitride` on PATH.
- Registers language `ngr` with the TextMate grammar and language
  configuration copied from Nitrogen's extension, and selects workspace
  program files by the extensions in the manifest it reads (same fallback
  order as the server).
- Contributes the same `semanticTokenTypes` as Nitrogen.
- Activation: `onLanguage:ngr`, `workspaceContains:nitride.json`,
  `workspaceContains:nitrogen.json`.

### Nitrogen changes

- `Nitrogen/editors/vscode/src/extension.ts`: if
  `extensions.getExtension('nitride.nitride')` is present, do not start the
  language client, C# string decorations or signature routing (show nothing,
  or a single status-bar note; builder's call). A test in the existing style
  (`*.test.ts`) covers it.
- `Nitrogen/Nitrogen.Cli/VsCode/VsCodeRenderer.cs`: the generated
  `nitrogen.nitrogen-*` extensions apply the same check. A test alongside the
  existing renderer tests covers the generated code.
- No other change to Nitrogen. Plugins generated before this change are not
  affected; regenerate them.

## Testing

- Rust integration tests drive `nitride lsp` over stdio with scripted
  sessions: initialize, open a sample from `languages/*`, assert diagnostics,
  tokens, symbols, folds; edit a grammar and assert the program's results
  change without restart.
- Where Nitride admits a language, compare results against Nitrogen's server
  on the same files (Nitrogen is available locally as a reference, like the
  existing `.NET` oracle tools in `tools/`).
- Extension unit tests in TypeScript for manifest reading and server
  selection, mirroring Nitrogen's `server.test.ts` / `otherExtensions.test.ts`.
- A manual check: install the `.vsix` alongside Nitrogen's and confirm only
  Nitride results appear.

## Calls I made (not discussed with the user)

- Optional `catalog` and `mode` fields in `nitride.json` entries;
  `nitrogen.json` entries are syntax-only. This repo ships a `nitride.json`.
- Signed programs (milestone 4): accept both `#!nitrogen …` and `#!nitride …`
  headers, and serve all workspace files as Nitrogen does.
- Extension id `nitride.nitride`, setting `nitride.server.path`, custom
  request prefix `nitride/`.
- Same semantic token legend as Nitrogen.
- Local `.vsix` only; platforms macOS arm64/x64, Linux x64, Windows x64.
- New crate for the server; `serde_json` allowed there, not in core.
- Full reanalysis per change is acceptable for milestone 1.

## Left to the builder

Crate and module layout, JSON-RPC implementation details, debounce interval,
how `Language` becomes owned, the exact engine APIs for position lookup and
symbol indexing, and test harness structure.
