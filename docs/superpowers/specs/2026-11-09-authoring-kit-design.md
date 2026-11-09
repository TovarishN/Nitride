# Versioned grammar library and language-authoring kit

## Intent and decisions

Authors currently repeat identifier/number rules and assemble project files by
hand. Supply reusable lexical grammar modules, a working project template and
contextual file-creation actions. Nitride advances independently; Nitrogen
compatibility does not define this milestone.

The user chose a **versioned library supplied by Nitride**, rather than editable
copies generated into each project. The following are builder proposals for
approval: offline bundling, exact version pins, the initial token set, CLI/UI
shape and the staged scope below. No implementation has started.

## 1. Explicit library references

A language project can declare:

```text
language Calculator
{
  extensions .calc;
  library Nitride.Lexical version 1.0.0;
  grammar Calculator.ngr;
  start Calculator.Program;
  catalog Calculator.ncatalog;
}
```

The project grammar gains a `library` member with a qualified name and exact
stable `MAJOR.MINOR.PATCH` version. No ranges, implicit latest, downloads or
fallback versions. Reject leading-zero/noncanonical versions. `project.json`
gains optional `libraries: [{"name":"Nitride.Lexical","version":"1.0.0"}]`;
absent/empty retains existing behavior. `.nproj` is the canonical authoring path;
legacy JSON diagnostics retain their existing location fidelity.

A project may select one version per library name. Duplicate or conflicting
pins are errors. Different projects in one workspace may select different
bundled versions. Unknown names/versions fail with available choices and a
project-located diagnostic. The editor may retain its last good language under
its existing policy, with visible project errors; that is not resolver fallback.

Library source is ordinary grammar source. Explicitly selecting a library makes
its module available; it does not inject a `using` into user grammars. Example:

```text
syntax module Calculator
{
  token Identifier = Nitride.Lexical.AsciiIdentifier except "let";
  token Number = Nitride.Lexical.UnsignedDecimal;
  // Language-specific syntax and semantics follow.
}
```

Keywords remain local through `except`. Project trivia remains local. Importing
lexical rules must not silently change comment handling, whitespace, numeric
semantics or identifier policy. There are no parser-core special token names.

## 2. Initial library contract

Ship `Nitride.Lexical@1.0.0`, module `Nitride.Lexical`, with these public tokens:

| Token | Accepted syntax |
|---|---|
| `AsciiIdentifierStart` | ASCII letters or underscore |
| `AsciiIdentifierContinue` | ASCII letters, digits or underscore |
| `AsciiIdentifier` | Start followed by zero or more Continue characters |
| `Digit` | ASCII 0–9 |
| `HexDigit` | ASCII 0–9, a–f or A–F |
| `UnsignedInteger` | One or more Digit; leading zeros allowed |
| `UnsignedDecimal` | UnsignedInteger optionally followed by `.` and one or more Digit |
| `HexInteger` | `0x` or `0X` followed by one or more HexDigit; no suffix |
| `JsonString` | Existing strict JSON string lexical syntax, including its escape rules |

UnsignedDecimal accepts integers, but no sign, exponent, leading dot or trailing
dot. Signs remain language syntax. JsonString does not perform value decoding.
Document token overlap and ordered choice: more-specific alternatives such as
HexInteger precede UnsignedDecimal when combined. These are syntax policies,
not a universal Number/String type. C# literals, Unicode identifiers, raw strings
and interpolation are not part of 1.0.0.

Every published library version's canonical source bytes and manifest are
immutable. Changes, including corrections, get a new library version. Retain
older bundled versions when introducing a successor; never silently change the
meaning of a pin. Future engine releases need not provide every future library
version. Source hashes and exported names are discoverable. Stable library
source does not promise stable internal numeric rule IDs across compiler builds.

## 3. Bundling, resolution and provenance

Introduce a small data-only `nitride-stdlib` crate with checked-in versioned
`.ngr` sources and a static registry embedded using `include_str!`. No runtime
network or user-global mutable library cache is needed. CLI, VS Code's bundled
server, native distribution and Rust library consumers use the same bytes.
Release archives need no loose library-source files for resolution.

`nitride-prepare` owns the shared project-library resolver. Keep physical project
inputs separate from bundled source identities. A resolved identity contains
library name, exact version, member name, canonical document URI and SHA-256.
`PreparedProject` exposes resolved-library metadata alongside its physical input
list. Do not turn a virtual library URI into a PathBuf to canonicalize or watch.
Existing source-only preparation APIs keep their current explicit-input behavior;
provide a resolver/source API for library consumers that need composition.

The CLI project loader and LSP registry call this same resolver, rather than
implementing two lookup rules. Library sources precede local sources in declared
library order; existing local grammar order remains intact. Conflicting module
or rule definitions use ordinary preparation errors. No global implicit import.

Fingerprints include library identity, version, source URI and content hashes,
as well as the existing local inputs. Equal bytes under different versions must
not cause stale navigation/source metadata to survive a pin change. Old loaded
languages/readers retain their selected immutable library sources safely.
Libraries are not filesystem watch inputs; changing a project pin triggers the
normal project invalidation path.

Prepared output and generated Rust contain the resolved rules, not a dependency
on finding the library at execution time. Preserve existing executable prepared
JSON compatibility; keep resolution provenance in the preparation API/registry
sidecar and include it in source-identity hashing rather than adding a required
runtime field. Old projects with no library declaration remain unchanged.

Expose `nitride library list [--json]` and
`nitride library show Nitride.Lexical@1.0.0` for discovery and exact source
inspection. The show form prints the sole 1.0.0 source verbatim; future
multi-source selection is outside this version's CLI contract. Release metadata
records bundled library identities/content hashes; native package validation
checks that all platforms report the same library manifest.

## 4. Editor library usability

Support completion/hover for library names and versions in `.nproj`, and grammar
completion/navigation for selected library exports. Definition targets use
`nitride-lib:/Nitride.Lexical/1.0.0/Lexical.ngr`. Parse this as a canonical library
identity: only known registry members resolve, never arbitrary paths or URLs.

The VS Code extension registers a read-only content provider for `nitride-lib`.
`nitride/librarySource` returns canonical embedded content for a validated URI.
Library buffers do not override bundled source, even if a client sends changed
text. Read-only views support grammar coloring, outline, hover and navigation;
rename, formatting and write-producing actions targeting library definitions
are disabled. A user wanting custom behavior defines a local wrapper or new rule.

Use an explicit initialization capability for library virtual documents. Clients
without a provider still get preparation/diagnostics/completion and version
information in hover; definition requests for bundled targets return null rather
than a fictitious filesystem path. Physical-source navigation stays unchanged.
Index identity includes library version, so references in separate projects do
not conflate different selected versions. Do not indiscriminately add the whole
stdlib to workspace-symbol results; local symbols retain their current behavior.

## 5. Shared templates

A single authoring planner produces deterministic relative files/contents for
both CLI and editor. Keep it separate from filesystem execution and LSP JSON.
Proposed CLI:

```sh
nitride new language Calculator --directory ./Calculator --extension .calc --kind typed
nitride new language GrammarOnly --directory ./GrammarOnly --extension .demo --kind syntax
```

Default kind is typed. Validate names/extensions against the actual project
frontend; names are single ASCII identifiers and extensions are one supported
`.suffix`. Generated internal filenames use those validated names. Dry-run/JSON
plan inspection is available; no timestamps, absolute host paths or random IDs
enter generated source.

Both presets create `nitride.nproj`, `<Name>.ngr`, `sample<extension>` and README.
The typed preset also creates `<Name>.ncatalog`. Both pin the exact bundled
Lexical version in their project; neither copies the library grammar. Typed
starts with nominal Scalar declarations/references/literals and a sample that
executes to 42 without components, Python, grants or custom host handlers.
Syntax-only is prepare/parse-ready. Generated README gives exact prepare/parse,
and typed run commands. Template golden files are not a substitute for executing
those commands in temporary projects.

CLI creation requires a new target directory and fails on an existing one. Use
exclusive directory/file creation; preflight, write the project manifest last,
and report partial failures. Roll back only files known to be created by this
operation and still unmodified; preserve anything uncertain or externally
modified and report retained paths. Do not promise filesystem-wide atomicity.
There is no force/overwrite option in the first version.

`Nitride: New Language` uses standard VS Code inputs and the same server-side
planner/executor through an advertised `workspace/executeCommand`. Its target
must be within an open workspace folder; CLI accepts any explicit user target.
After creation offer to open the standalone language folder in a new window.
Do not silently modify the containing Nitride workspace's include list or close
unsaved editors. Automatic registration into an existing multi-language project
is a later action, not a hidden side effect of creation.

## 6. Contextual project quick fixes

First version actions:

- Create a missing grammar file already referenced by `.nproj`.
- Create a missing catalog file already referenced by `.nproj`.
- Add the exact bundled Lexical pin when an explicitly qualified
  `Nitride.Lexical.*` reference identifies the library and one owning project is
  unambiguous. This is a versioned text edit to the project, not a file copy.

Missing-file actions create a clearly labeled scaffold. Grammar scaffolds use
the declared start module/rule when unambiguous and absent; otherwise create a
valid empty module, avoiding duplicate start declarations. Catalog scaffolds
create a valid empty catalog module. They resolve the missing-file problem, not
invent semantic operations or claim every remaining preparation error is fixed.
Each recipe must parse with the appropriate existing frontend. Do not offer a
recipe when project context is ambiguous or when it would duplicate a known rule.

Use structured project diagnostics (kind, reference span, intended target and
project identity), never parse message strings to decide a write. Actions are
offered for the actual relevant diagnostic/range. New-file actions require client
`documentChanges`/CreateFile support, use overwrite=false and ignoreIfExists=false,
and never run a host process. Missing-file references already exist in the
manifest, so these actions do not need unrelated manifest edits.

Creation targets must be within an open workspace, after checking normalized
paths and symlinked ancestors. Do not offer writes outside it or into bundled
URIs. Existing files—including unsaved open documents—must never be overwritten.
Revalidate current diagnostic/project context when resolving an action; guard
existing-document edits with versions. If context is stale, return disabled/no
edit and let the client refresh. Group resource edits for normal client undo and
respect advertised failure handling. Do not claim universal resource-edit
atomicity; failed application must remain visible through the client. Unsupported
clients get no unsafe filesystem fallback.

Binding scaffolds, host implementation generation, arbitrary unknown-rule file
inference and add-catalog-with-manifest multi-file actions are deferred until
there is enough explicit interface/project context. They are not guessed from
an unresolved short identifier.

## Staging and acceptance

Implement in three reviewable increments after design approval:

1. Embedded library registry/resolver, project grammar/JSON support, CLI
   discovery and read-only editor inspection/navigation.
2. Shared authoring planner, syntax/typed templates, CLI and New Language command.
3. Structured missing-file/library diagnostics and bounded contextual quick fixes.

Do not migrate C# or every existing example in this milestone. Demonstrate reuse
with newly generated syntax/typed projects; wider example adoption can follow.
Do not introduce a package manager, remote downloads, semver ranges, Unicode
identifier claims, implicit keyword policy or new engine semantic primitives.

Required evidence:

- Exact pins resolve offline in CLI, editor and library APIs; unavailable,
  duplicate/conflicting pins and malformed declarations fail at useful locations.
- Tests with two registry versions prove same-workspace isolation, fingerprint
  invalidation even for equal source text, and old-reader safety.
- Library source hashes are pinned; changing an existing published version's bytes
  fails its immutability gate. Exported-token acceptance/rejection, prefix
  boundaries, local keyword exclusions and trivia policies agree in runtime and
  generated parsers. JsonString keeps the established strict JSON string cases.
- Virtual definitions open exact read-only source; malformed/traversal URIs and
  virtual edits cannot redirect lookup or change preparation. Renaming bundled
  targets is rejected, including invocation from a local reference.
- Every template prepares/parses; typed executes to 42 in the distributed native
  feature configuration. CLI/editor plans agree. Existing targets, bad names,
  collisions, denied writes and mid-creation failure preserve user data.
- Quick fixes create only selected intended files, reject stale/ambiguous contexts
  and outside-workspace/symlink targets, preserve existing/open buffers and honor
  unsupported client capabilities. The diagnostic they address disappears, with
  remaining semantic problems reported normally.
- Project frontend seed regeneration/fixed-point checks, existing engine/native
  suites, extension/tooling checks and packaged editor authoring workflows pass.
  No speculative LSP optimization or performance claim is required.

Implementation may choose internal data structures/file splits and test seams,
provided the contracts above hold. Use existing module composition and parser
frontends; no regular-expression project parser or duplicated JS grammar library.
