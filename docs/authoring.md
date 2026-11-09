# Authoring a language

Create a standalone project in a **new** directory:

```sh
nitride new language Calculator --directory ./Calculator --extension .calc
cd Calculator
nitride run nitride.nproj sample.calc
```

The default typed starter evaluates `let answer = 42; show answer;` and returns
`{"ok":true,"trace":[],"values":[42.0]}`. It uses the existing Scalar literal,
sequential declaration, reference and show machinery, without host operations,
processes, components or grants. `--kind syntax` creates a smaller syntax-only
assignment language; use `nitride parse` for that preset. `--dry-run --json`
returns a deterministic file plan without creating the directory.

In VS Code, **Nitride: New Language** asks for the workspace folder, language
name, extension and preset. It creates the same files and offers to open the
standalone folder in a new window. It does not alter a parent workspace's
include list. Existing directories and open planned targets are refused.

Creation uses exclusive directory/file creation and writes the project manifest
last. If a write fails, cleanup removes only files whose bytes still match this
invocation's writes; changed or uncertain partial paths are reported and kept.
There is no `--force` or overwrite mode.

## Bundled lexical library

Projects explicitly pin the offline library:

```ngr
language Calculator {
  extensions .calc;
  library Nitride.Lexical version 1.0.0;
  grammar Calculator.ngr;
  start Calculator.Program;
}
```

JSON manifests use `"libraries":[{"name":"Nitride.Lexical","version":"1.0.0"}]`.
Only exact canonical stable versions are accepted. Unknown versions, duplicate
pins and conflicting pins are errors. There is no download, version range,
implicit import or fallback to another version. The editor retains its previous
working reader during an invalid edit and displays the pin error.

Use qualified tokens or local aliases; keep keyword exclusions local:

```ngr
token Identifier = Nitride.Lexical.AsciiIdentifier except "let" "show";
token Number = Nitride.Lexical.UnsignedDecimal;
```

`nitride library list --json` reports the canonical bundled manifest, including
export names, source URIs and SHA256 hashes. `nitride library show
Nitride.Lexical@1.0.0` prints its source verbatim. Prepared tables and generated
parsers contain the resolved rules and need no library lookup at runtime.
Library identity/version/URI/hash are part of project fingerprints; physical
input paths exclude bundled sources. Future versions retain the immutable 1.0.0
entry. Numeric rule IDs are not a stable API.

| Token | Policy |
| --- | --- |
| AsciiIdentifierStart / AsciiIdentifierContinue | ASCII letters/underscore; continuation also digits |
| AsciiIdentifier | ASCII start followed by zero or more continuation characters |
| Digit / HexDigit | One decimal digit / hexadecimal digit |
| UnsignedInteger | One or more decimal digits; leading zeros allowed |
| UnsignedDecimal | Integer with optional fractional digits; no sign, exponent, leading or trailing dot |
| HexInteger | `0x` or `0X` and at least one hexadecimal digit; no suffix |
| JsonString | JSON lexical string, including escapes; no decoding or Unicode identifier policy |

Token internals do not skip whitespace/comments. A project's `trivia json;`
controls text outside tokens and is separate from lexical reuse.

## Navigation and project fixes

VS Code opens definitions at
`nitride-lib:/Nitride.Lexical/1.0.0/Lexical.ngr` through a read-only content
provider. Library documents retain grammar coloring, outline and navigation.
Formatting, code actions and rename are refused; a local reference cannot
rename a library definition. Virtual changes cannot replace canonical content.
Other clients opt into `initializationOptions.libraryDocuments`; clients without
it still receive hover metadata but no unusable library definition URI.

Project quick fixes can create a missing **referenced** grammar/catalog, or add
the exact pin for an explicit `Nitride.Lexical.Token` reference. A grammar
scaffold adds a declared missing start rule only when unambiguous and absent
from the other readable inputs; otherwise it creates an empty valid module.
An empty catalog fixes the missing file, without inventing semantic contracts.

Project actions require documentChanges and edit-resolving code action support;
file creation additionally requires the `create` resource operation. Resolve
checks the source/project snapshots, versions, workspace boundary, symlinks,
open buffers and current filesystem. CreateFile has overwrite and ignoreIfExists
both false. Existing-document pin edits are versioned; VS Code opens the
unchanged project buffer before resolving a pin action. Other clients must open
the owning project before resolving it. Stale actions are disabled. Shared
physical grammars with ambiguous project ownership have no pin edit or silently
selected library navigation context. No binding stubs or arbitrary short-name
imports are suggested. Client workspace edits retain their normal undo/failure
semantics; no global filesystem atomicity is promised.
