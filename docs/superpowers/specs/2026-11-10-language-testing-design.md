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

The following details are proposed implementation choices, not additional user
requirements: versioned JSON suites, strict parser/compiler observations, a
native worker process for editor runs, and snapshot overlays for unsaved files.

## 1. Suite format

A suite is a UTF-8 `*.ntest.json` file with this shape:

```json
{
  "version": 1,
  "project": "nitride.nproj",
  "tests": [
    {
      "name": "a binding evaluates",
      "source": "let answer = 42; show answer;",
      "expect": { "phase": "run", "accepted": true, "values": [42] }
    },
    {
      "name": "an incomplete binding is rejected",
      "source": "let answer = ;",
      "expect": { "phase": "parse", "accepted": false }
    }
  ]
}
```

`project` is required and names one language `.nproj` or supported JSON project,
relative to the suite's directory. Workspace `.nproj` files are not language
projects. Each test has a unique, nonempty `name` and exactly one of `source`
(inline text) or `file` (UTF-8 fixture path relative to the suite). Test names are
identities within their suite; array order defines execution/report order.
Unknown fields, duplicate object keys, unknown versions/phases, empty suites,
duplicate test names and contradictory assertions are configuration errors.

`phase` is `parse`, `compile` or `run`. `accepted` is required. `values` is allowed
only with successful run expectations and compares the complete ordered show
result list. Successful run expectations require `values`, including `[]`.
An optional `diagnostics` array is allowed only with rejected expectations. If
omitted, any ordinary error at the requested phase satisfies rejection. If
present, it must be nonempty and matches the entire ordered strict diagnostic
list. Each expected diagnostic requires `category` and may assert `code`
(string or null) and/or `span` (`[start,end]` UTF-8 byte offsets in the original
test source). Omitted fields do not assert that property; explicit null means
no code. Expected spans must be ordered, in bounds and at UTF-8 boundaries.
Messages are reported but are not assertion keys in version 1.

Value JSON follows the CLI's existing numbers/booleans/strings/ordered list and
record-field-array representation. Structural order and types matter. Numeric
comparison treats 42 and 42.0 as equal while preserving exact integer comparisons,
including integers beyond 2^53; it must not silently round distinct integers via
f64. No tolerance, unordered comparison or implicit expected-value update.

## 2. Shared runner and phase behavior

A small shared `nitride-testing` Rust crate owns decoding, validation, test
selection, observations, comparisons and structured reports. It consumes the
existing prepare/codegen/core APIs; the CLI adapts arguments and output, and the
extension consumes the CLI protocol. It does not duplicate parser semantics.

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

`nitride test [suite.ntest.json ...]` runs explicit suites; without paths it uses
`tests.ntest.json` in the current directory. It does not search arbitrary project
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
covers workspace `*.ntest.json` files, excluding `.git`, `target` and
`node_modules`, plus matching open file-URI suite buffers. Invalid suites remain
visible with a useful error. Discovery never executes cases. Suite items reveal
the suite file; fixture diagnostics can reveal the fixture at converted UTF-16
ranges. Inline-source failures display byte spans and expected/actual details;
no fabricated editor range into an escaped JSON string is promised.

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

The shared authoring planner emits `tests.ntest.json` for both presets. Syntax
starters include a passing assignment and a rejected malformed assignment. Typed
starters additionally check the existing sample's values `[42]` and a rejected
unresolved reference at the compile phase. Record actual engine diagnostic codes
and byte spans for at least one representative generated test; do not invent
codes. README commands cover `nitride test`, `--json` and the Testing view.
The existing exclusive creation, collision checks, rollback and manifest-last
rules apply to the extra file. Existing projects need no migration or test file;
existing prepared execution JSON/library 1.0.0 sources remain unchanged.

## 7. Acceptance and delivery

1. Shared suite decoder/runner: positive and negative phase assertions, exact
   code/null/span matching, non-ASCII offsets, numeric precision, state isolation,
   invalid schemas and infrastructure errors. Differential observations against
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

Non-goals: a new test DSL, recovery-tree goldens, benchmark framework, watch mode,
debugger integration, remote test executors, process/component test fixtures,
automatic golden updates, additional stdlib versions or LSP optimization.
