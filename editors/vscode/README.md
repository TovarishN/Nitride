# Nitride DSL for VS Code

Language support for `.ngr` grammars and the languages a workspace declares,
served by `nitride lsp`, Nitride's native language server. No .NET runtime is
needed.

## Languages

The extension reads `nitride.json` at each workspace folder root, else
`nitrogen.json`. Both list languages the same way:

```json
{ "languages": [ { "name": "Calls", "extensions": [".calls"],
  "grammars": ["languages/calls/Calls.ngr"], "start": "Calls.Program",
  "catalog": "languages/calls/catalog.json", "mode": "typed" } ] }
```

`catalog` and `mode` are read from `nitride.json` only. A language with a
catalog is checked in typed mode; without one (and every `nitrogen.json`
language) it is syntax-only, and Nitrogen's C# semantic members in its grammar
are skipped with a warning.

## Features

- Diagnostics, semantic coloring with the theme's TextMate palette (as the
  Nitrogen extension colours), outline, folding and expand-selection.
- Hover with types (typed languages), go to definition, find references,
  highlights and rename, for program names and grammar rules. Rule renames
  update every grammar file of the language.
- Workspace symbol search over grammar rules and open documents.
- Completion, signature help for built-in calls, and formatting (document,
  selection, and on typing `}`); formatting changes only whitespace.
- Inlay hints with each statement's computed value (`= 42`).
- Nitrogen signatures: a file can pick its language with
  `#!nitrogen language "./Calc.syntax"` or `#!nitrogen language Calc`, and a
  `#!nitrogen grammar` file declares one with `language Calc { … }`. A
  `Calc.catalog.json` beside it makes the language typed. Augmentations and
  regions are not supported.
- Quick fixes from the parser's repairs (`Insert ;`), and removal of Nitrogen
  C# semantic members from grammars (one, or all in a file).

Grammar edits apply without restarting the server.

## Settings

- `nitride.server.path`: the `nitride` executable to run. Empty runs the server
  bundled with the extension, else `nitride` from PATH.

When the Nitrogen extension is also installed, it stays inactive and Nitride
serves these files.
