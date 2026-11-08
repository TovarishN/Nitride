# Nitride DSL for VS Code

Language support for `.ngr` grammars, `.ncatalog` catalogs and the languages a workspace declares,
served by `nitride lsp`, Nitride's native language server. No .NET runtime is
needed.

## Languages

The extension reads `nitride.nproj` at each workspace folder root, which lists
language folders, each holding one `.nproj` language file:

```
workspace
{
  include languages/calls;
}
```

```
language Calls
{
  extensions .calls;
  grammar Calls.ngr;
  start Calls.Program;
  catalog Calls.ncatalog;
}
```

Paths are relative to the `.nproj`. A language with a `catalog` (`.ncatalog`
files, see `docs/catalog-language.md`) is checked in typed mode; without one it
is syntax-only, and Nitrogen's C# semantic members in its grammar are skipped
with a warning. `.nproj` files get diagnostics, outline and go to definition
(files, included folders and the start rule).

Without `nitride.nproj`, the extension falls back to Nitrogen's `nitrogen.json`
(`{ "languages": [ { "name", "extensions", "grammars", "start" } ] }`), whose
languages are syntax-only.

## Features

- Diagnostics, semantic coloring with the theme's TextMate palette (as the
  Nitrogen extension colours), outline, folding and expand-selection.
- Hover with types (typed languages), go to definition, find references,
  highlights and rename, for program names, grammar rules and named patterns.
  Rule renames update every grammar file of the language.
- Workspace symbol search over grammar rules and open documents.
- Completion, signature help for built-in calls, and formatting (document,
  selection, and on typing `}`); formatting changes only whitespace.
- Inlay hints with each statement's computed value (`= 42`). Operations bound to
  a process, or needing an effect such as reading the clock, never run from the
  editor and leave no hints. The bundled server leaves out the WebAssembly
  runtime to stay small, so operations bound to a component have hints only when
  `nitride.server.path` names a `nitride` built with its default features
  (`cargo build --release -p nitride-cli`).
- Nitrogen signatures: a file can pick its language with
  `#!nitrogen language "./Calc.syntax"` or `#!nitrogen language Calc`, and a
  `#!nitrogen grammar` file declares one with `language Calc { … }`. A
  `Calc.ncatalog` beside it makes the language typed. Augmentations and
  regions are not supported.
- Quick fixes from the parser's repairs (`Insert ;`), and removal of Nitrogen
  C# semantic members from grammars (one, or all in a file).

Grammar edits apply without restarting the server. Program edits use shared
recovering syntax snapshots, with incremental UTF-16 text synchronization.
Binding, structural checks, navigation indexes and value hints are recomputed
for the whole document. Grammar/catalog/project reloads invalidate cached syntax;
frontend authoring files continue to use full analysis.
Diagnostic debounce deadlines follow accepted edits and relevant language-input
changes; unrelated traffic does not postpone them. File-watch reloads also run
when no document is pending. See [LSP scheduling](../../docs/lsp-scheduling.md)
for ordering and remaining synchronous-analysis limits.

## Settings

- `nitride.server.path`: the `nitride` executable to run. Empty runs the server
  bundled with the extension, else `nitride` from PATH.

When the Nitrogen extension is also installed, it stays inactive and Nitride
serves these files.
