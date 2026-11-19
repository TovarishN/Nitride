# Nitride for Zed

Native Zed extension for Nitride authoring, using `nitride lsp` and a bundled
Tree-sitter grammar. No .NET runtime is required.

## Install locally

From the Nitride repository root:

```sh
# A smaller native CLI build; omit --no-default-features for component execution.
cargo build --release -p nitride-cli --no-default-features
python3 editors/zed/prepare-dev.py
```

In Zed, run **`zed: install dev extension`** and select the directory printed by
the preparation command (normally `editors/zed/target/dev-extension`). Zed builds
the extension and grammar; Rust via rustup is required. Zed installs the
`wasm32-wasip2` target and downloads wasi-sdk when needed. See
[Zed's development instructions](https://zed.dev/docs/extensions/developing-extensions).
Python 3.11+ and Git are required for preparation. Node and the Tree-sitter CLI
are only needed when changing the grammar; generated parser sources are included.

Make `nitride` available on Zed's worktree PATH, or set its absolute path in
Zed's settings:

```json
{
  "lsp": {
    "nitride": {
      "binary": {
        "path": "/absolute/path/to/Nitride/target/release/nitride",
        "arguments": ["lsp"]
      }
    }
  }
}
```

When `nitride` is found on PATH, the extension launches `nitride lsp`.
When setting `binary.path`, also set `binary.arguments` to `["lsp"]`: Zed
launches explicit binary overrides directly and does not use the extension's
default arguments. `binary.env` overrides inherited worktree environment
variables. `initialization_options` is forwarded to the server. Standard LSP
semantic token types are used by default; VS Code's palette-specific token types
and virtual library documents should stay disabled.

Preparation creates a private Git snapshot of the generated grammar and points
the dev extension at its exact local revision. This lets Zed load uncommitted
parser changes without committing or publishing the Nitride checkout. Keep the
prepared directory in place while the dev extension is installed. After changing
extension sources, rerun preparation and rebuild the dev extension in Zed.

## Support

| Files | Highlighting | Language server |
| --- | --- | --- |
| `.ngr` | Grammar declarations, literals, comments, operators | Existing grammar authoring features |
| `.ncatalog` | Catalog and operation declarations | Existing catalog authoring features |
| `.nbind` | Bindings, component paths, literals | Existing binding authoring features |
| `.nproj` | Workspace and language configuration | Existing project authoring features |
| `.ntest` | Test keywords, values and raw source strings | Run through the test-suite task |

The language server provides diagnostics, completion, hover, definitions,
references, rename, formatting, code actions and inlay hints where the underlying
Nitride frontend supports them. Tree-sitter provides highlighting, bracket
matching, indentation and a declaration outline even without the language server.
Its grammar is deliberately structural and tolerant; `nitride lsp` remains the
authority on syntax and semantics. Embedded source in `<[ ... ]>` is highlighted
as a string, without assuming which workspace language it contains.

`.ntest` is not an LSP frontend in the current server. With a suite open, use
**`task: spawn` → `Nitride: run test suite`** to save and run it through
`nitride test`. This task uses `nitride` from PATH, independently of the LSP binary
setting. See [language testing](../../docs/guides/language-testing.md).

The server loads languages from `nitride.nproj`, falling back to `nitrogen.json`.
Zed cannot dynamically register file associations through this extension API.
This repository's [`.zed/settings.json`](../../.zed/settings.json) associates
the example language suffixes with **Nitride Program**, including path-specific
associations for the JSON and XML samples. Open the repository root in Zed to
use these settings. Other JSON, XML and C# files keep their usual associations.
The repository settings also enable inlay hints for **Nitride Program**, so
evaluated statement values appear inline. Zed disables inlay hints by default;
in other workspaces, set `languages."Nitride Program".inlay_hints.enabled` to
`true` in Zed settings to display them.
For a workspace DSL, associate its file suffix with **Nitride Program** in Zed:

```json
{
  "file_types": {
    "Nitride Program": ["calc", "portable"]
  }
}
```

A `#!nitrogen grammar` or `#!nitrogen language ...` first line also selects the
corresponding language. Signature headers in leading comments are not detected
by Zed's first-line matcher; use an explicit file association for those files.
The generic program highlighter recognizes common tokens; DSL-specific semantic
coloring comes from the server.

Open-document grammar/catalog/project edits reach the server immediately through
LSP document synchronization. The current server does not register filesystem
watchers with generic LSP clients; restart the language server after external
changes to closed language inputs. VS Code's test explorer, new-language wizard,
and virtual standard-library source provider are not ported by this extension.

## Development

The extension is a separate Cargo workspace because Zed compiles it as a WASI
component rather than a native Nitride crate.

```sh
cargo test --manifest-path editors/zed/Cargo.toml --locked
cargo clippy --manifest-path editors/zed/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path editors/zed/Cargo.toml --target wasm32-wasip2 --locked
python3 -m unittest discover -s editors/zed/tests -v
# Install tree-sitter-cli 0.25.10, with its executable on PATH:
python3 editors/zed/check.py
```

`tree-sitter-nitride/queries/` holds the shared queries. After changing them,
copy the `.scm` files into every `languages/*/` directory; `check.py` checks for
drift. Grammar corpus tests, highlighting assertions and existing repository
source files are checked, and every Zed query is compiled against the parser.
Generated C sources and headers are committed so installation needs no parser
generation. Regenerate them with Tree-sitter CLI 0.25.10:

```sh
cd editors/zed/tree-sitter-nitride
tree-sitter generate
```

For publication, first publish the parser source in the Nitride repository and
pin `[grammars.nitride].rev` in `extension.toml` to that commit. The source manifest
tracks `main` as a development default; before these changes are published, use
the prepared local extension. Submission to the Zed extension registry is a
separate release step.
