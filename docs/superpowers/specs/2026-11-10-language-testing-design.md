# Language-author testing workflow

## Intent and scope

The user agreed to the next milestone: a `nitride test` workflow covering accepted
and rejected syntax, diagnostic codes/spans and evaluated values; starter tests
in generated projects; and running/inspecting those tests from VS Code.

This closes the create → author → test → diagnose loop. Nitride continues as an
independent engine; Nitrogen compatibility is optional comparison evidence.
The builder retains Rust/native CLI and the existing VS Code extension. This is
one feature milestone, delivered through independently verifiable increments.
No engine release or version bump is part of this work.

The user requires `.ntest` files rather than `.ntest.json`. The proposed authoring
format is a dedicated Nitride test language; JSON remains a machine protocol.
Strict parser/compiler observations, a native worker process for editor runs and
snapshot overlays are proposed implementation choices.

## 1. Suite format

A suite is a UTF-8 `*.ntest` file. It uses a dedicated grammar described in
Nitride's own `.ngr` format, rather than JSON with a renamed extension:

```ntest
tests version 1 {
  project "nitride.nproj";

  test "a binding evaluates" {
    source <[
let answer = 42;
show answer;
]>;
    expect run values [42];
  }

  test "an incomplete binding is rejected" {
    source "let answer = ;";
    expect parse reject;
  }

  test "a fixture parses" {
    file "fixtures/valid.calc";
    expect parse accept;
  }
}
```

`tests version 1` is required. The body contains exactly one `project` declaration
and one or more tests, in any declaration order. Project paths are quoted and
name one language `.nproj` or supported JSON project relative to the suite's
directory. Workspace `.nproj` files are not language projects. Each test has a
unique, nonempty quoted name and exactly one `source` or `file` declaration plus
exactly one `expect` declaration; source/file and expect may appear in either
order. Test names are stable identities within their suite; lexical test order
defines execution/report order. Unknown declarations, repeated declarations,
unknown versions/phases, empty suites, duplicate names and contradictory
assertions are suite errors with original `.ntest` source spans.

`source` accepts a normal double-quoted string or a raw `<[ ... ]>` block.
Quoted strings use JSON string escapes and Unicode decoding. Raw blocks preserve
all content bytes exactly, including leading/trailing newlines, indentation and
line endings; neither comments nor escapes are interpreted inside them. The
first `]>` closes the block. Tests containing that sequence can use a quoted
string or a fixture instead. No implicit interpolation or indentation stripping.
Outside strings/blocks, the test language accepts the engine's nitrogen trivia
policy, including line/block comments. Names, paths and diagnostic categories
use quoted strings, so target-language keywords and file names need no new
identifier rules. Parsing and decoding must use the engine grammar frontend and
retained source spans, not regular-expression extraction.

Expectations have these forms:

```ntest
expect parse accept;
expect compile accept;
expect run values [42, true, "text", [1, 2]];
expect parse reject;
expect compile reject {
  diagnostic "category-from-engine" code "code-from-engine" span [0, 6];
}
expect run reject;
```

The diagnostic example illustrates syntax; actual categories/codes come from
engine observations and must not be invented in generated tests. A successful
run always requires `values`, including `[]`; `expect run accept;` is rejected.
Parse/compile successes take `accept`; rejected expectations take `reject`, with
either a terminating semicolon or a nonempty diagnostic block. A reject without
a block accepts any ordinary error at that exact phase. A diagnostic block
matches the entire ordered strict diagnostic list. Each diagnostic requires a
quoted category and may assert `code "..."` or `code none`, and/or
`span [start, end]`; these clauses may appear in either order but not repeat.
Omitted clauses do not assert that property; `code none` means no diagnostic code.
Spans are UTF-8 byte offsets in the original test program, must be ordered and in
bounds, and must lie at UTF-8 boundaries. Messages are displayed but are not
assertion keys in version 1. A diagnostic block ends with `}` without an extra
semicolon.

Expected values use finite signed decimal/exponent numbers, double-quoted
strings, booleans and recursively nested comma-separated arrays. Their notation
resembles literal values, not a JSON document format. No objects, null values,
trailing commas, expressions, variables or embedded host code in version 1.

Structured result JSON follows the CLI's existing numbers/booleans/strings/ordered
list and record-field-array representation. Expected literals compare against
that same value model. Structural order and types matter. Numeric
comparison treats 42 and 42.0 as equal while preserving exact integer comparisons,
including integers beyond 2^53; it must not silently round distinct integers via
f64. No tolerance, unordered comparison or implicit expected-value update.

## 2. Shared runner and phase behavior

A small shared `nitride-testing` Rust crate owns the generated test-language
frontend, decoding with retained declaration/source spans, validation, test
selection, observations, comparisons and structured reports. It consumes the
existing prepare/codegen/core APIs; the CLI adapts arguments and output, and the
extension consumes the CLI protocol. It does not duplicate parser semantics. Store the test grammar and its project
under the repository's language sources, reusing the pinned lexical library
where its policies fit. Generate its native frontend using the existing preparer
and code generator; a build-generated frontend avoids a bootstrap dependency on
the test runner. Test suite parsing uses this frontend; runner validation lowers
the parsed suite into the existing typed internal test model.

Prepare/load each distinct project once per immutable run snapshot and release
it afterward, without static leaks. Each case gets fresh syntax, semantic and
execution state; no symbol/value state crosses cases. The project resolver
remains the single source of library/trivia/catalog/binding behavior.

A parse case invokes strict parsing only. A compile case parses then compiles;
a run case parses, compiles and executes. A failure at an earlier phase fails
the assertion; it cannot satisfy an expectation for a later phase. Report the
actual phase, complete diagnostics from that strict API and any available values.
Strict parsing may report one error; recovery/repair assertions and warning
coverage are separate future work. Asserted spans always address original source
bytes; editor navigation converts positions only at the UI boundary.

Preparation, malformed input, read/load errors, unsupported execution setup,
resource limits and cancellation are infrastructure outcomes, never a successful
negative case. Continue independent cases/suites after assertion failures; mark
cases blocked by a preparation error as errored. Limits retain normal engine
bounds; size/result bounds are explicit and tested, with useful errors.

Run cases use only existing built-in engine handlers. They never load process or
component bindings, install custom handlers or grant effects. Use the engine's
preflight-with-values API before any execution; unavailable/denied requirements
produce an errored test, rather than an expected source diagnostic. Built-in
Today reads use 2000-01-01, not the host clock. Existing `nitride run` and
`nitride conform` behavior is unchanged; effectful interface testing remains
with conformance tooling. Native testing works with `--no-default-features`.

## 3. CLI and protocol

`nitride test [suite.ntest ...]` runs explicit suites; without paths it uses
`tests.ntest` in the current directory. It does not search arbitrary project
trees. Repeated canonical suite paths are configuration errors.
`--case NAME` selects a named case and requires exactly one suite; an unknown
name is an error. The editor may request several cases through repeated --case.

Default output is concise per-case PASS/FAIL/ERROR and an aggregate summary.
`--json` emits one versioned structured report. `--list --json` validates suites
and lists stable suite/case identities and dependency paths, without preparing or
executing a language. `--events` emits versioned newline-delimited JSON records
for run start, case results and run finish; stderr holds ancillary errors.
JSON and events modes are mutually exclusive. Exit 0 means every selected test
passed, exit 1 means assertion failures, exit 2 means any infrastructure/config
error (takes priority over 1). An incomplete worker/event stream is never success.

Reports identify the suite, case name, source origin, requested and actual phase,
status (`passed`, `failed`, `errored`), expected/actual diagnostics or values,
and readable mismatch reasons. Public protocol fields and ordering receive
contract tests; engine text messages and wall-clock durations are not snapshots.
A run-finish event includes counts and overall status consistent with exit code.

## 4. Unsaved input snapshots

Editor tests observe the current buffers, including suite, project, grammar,
catalog, binding and source fixtures. They do not silently save files or run
older disk contents. Extend the shared preparer with an input-reader/overlay
entry point, retaining existing disk APIs as wrappers and all current no-overlay
behavior. Overlay identities are normalized physical paths, separate from the
immutable bundled library URIs. Bundled text is always registry-owned.

The CLI's `--snapshot-stdin` option accepts a versioned JSON envelope of path/text
overrides and uses disk for other inputs. It is explicit; ordinary CLI/CI runs
read disk. Invalid/duplicate identities and unsupported URI schemes fail.
Existing parent directories may contain new file-URI buffers that have not yet
been written; reading them must not require a physical file to exist.

VS Code freezes suite/project buffers, obtains dependencies through list mode,
then captures open overlays for those paths and runs that snapshot. Dependency
listing reads project declarations without compiling grammars. Inline sources
come from the frozen suite; fixture paths are included in dependency discovery.
Changes during discovery trigger rediscovery; changes after capture mark the run
as observing an older revision and must not overwrite newer diagnostics or test
results. Keep transient run versions separate from stable suite/case identities.
Untitled suites need a saved file location to establish their project directory.

## 5. VS Code Testing view

Use a standard TestController with suite items and child case items. Discovery
covers workspace `*.ntest` files, excluding `.git`, `target` and
`node_modules`, plus matching open file-URI suite buffers. Invalid suites remain
visible with a useful error. Discovery never executes cases. Suite items reveal
the suite file; fixture diagnostics can reveal the fixture at converted UTF-16
ranges. Inline-source failures display byte spans and expected/actual details;
raw-block source ranges map directly back into their `.ntest` body. For quoted
sources, use a decoded-source offset map if implemented; otherwise display byte
spans without fabricating a range into escaped text. Frozen-source locations
must be omitted or identified as stale when the current document has changed.
List-mode suite/case metadata includes original `.ntest` declaration/name spans,
so TestItems can reveal individual cases without a second TypeScript parser.

Contribute an `ntest` language with comments/brackets/strings and raw source-block
highlighting. Native list-mode syntax/validation errors populate a dedicated
suite diagnostic collection, guarded by document revision; parsing errors stay
visible while editing. Test discovery remains on its own worker path and does
not add suite execution to the LSP. JSON is used only for machine reports,
events and snapshot envelopes, not suite authoring.

Support Run All, suite and individual cases, with exclude selections honored.
Launch a separate native CLI worker per suite using the same executable selection
as the language server, with argument arrays and no shell. Feed snapshots over
stdin and consume events incrementally. The LSP analysis worker/message loop
never executes a test suite. Bound worker concurrency; default one worker.
Cancellation terminates that worker, retains completed results and marks remaining
cases skipped/cancelled; a late completion cannot change a cancelled or newer run.
Failures to launch, malformed/truncated events and abnormal exit are test errors.
The builder specifies process cleanup for Windows and Unix and verifies both.

Require workspace trust before test execution. Suite, project, dependency and
overlay paths for editor runs must remain within open workspace roots after
normalization and symlink resolution. Read-only library documents cannot be test
sources or override targets. CLI explicitly named paths retain normal filesystem
read semantics. Discovery/launch errors explain the concrete path or operation.

## 6. Starter projects and compatibility

The shared authoring planner emits `tests.ntest` for both presets. Syntax
starters include a passing assignment and a rejected malformed assignment. Typed
starters additionally check the existing sample's values `[42]` and a rejected
unresolved reference at the compile phase. Record actual engine diagnostic codes
and byte spans for at least one representative generated test; do not invent
codes. README commands cover `nitride test`, `--json` and the Testing view.
The existing exclusive creation, collision checks, rollback and manifest-last
rules apply to the extra file. Existing projects need no migration or test file;
existing prepared execution JSON/library 1.0.0 sources remain unchanged.

## 7. Acceptance and delivery

1. Self-described suite grammar and shared decoder/runner: comments, quoted
   strings, raw blocks, declaration spans, malformed syntax and decoding;
   positive and negative phase assertions, exact
   code/null/span matching, non-ASCII offsets, numeric precision, state isolation,
   invalid suite declarations and infrastructure errors. Differential observations against
   direct engine APIs; no duplicate homegrown language engine.
2. Native CLI: real temp projects, both manifest formats, multiple suites,
   selectors/list/report/events/exit codes, relocation, overlays/new buffers,
   frozen library behavior and unchanged ordinary disk APIs.
3. Authoring templates: both presets generate and pass their tests natively;
   creation remains exclusive and collision/partial-failure cases remain sound.
4. Extension: discovery, selections/exclusions, snapshots, stale revisions,
   cancellation, process/protocol failure and workspace path boundaries; focused
   helper tests and an actual packaged extension-host workflow in a disposable
   profile. Assert unsaved grammar/source edits affect tests without disk writes.
5. Full owning Rust/Clippy/format/bootstrap/extension/tooling checks; native
   packaged acceptance; one fresh whole-branch review and regression-backed
   material fix pass. Isolated branch/PR; preserve the user's main checkout.

Non-goals: JSON-authored suites, embedded host code, recovery-tree goldens,
benchmark framework, watch mode,
debugger integration, remote test executors, process/component test fixtures,
automatic golden updates, additional stdlib versions or LSP optimization.
