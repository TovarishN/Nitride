# LSP analysis and diagnostic scheduling

With positive debounce (150 ms by default), document analysis runs on one
background worker. The message loop owns documents, registry changes, RPC output
and request bookkeeping, and continues handling input while analysis runs. Zero
debounce retains synchronous analysis after every message for scripted sessions.

## Quiet time and fairness

Accepted edits and relevant workspace/language-input notifications set a debounce
deadline. Unrelated requests, ignored notifications, stale versions and invalid
edit transactions leave it unchanged. A due batch becomes ready; one document
runs at a time and pending URIs coalesce newer work. There is at most one
outstanding worker job, rather than an unbounded queue of old document versions.

Completed results use a bounded slot and a wake event. Each loop turn checks at
most one completion and handles one input message, so queued unrelated traffic
cannot trap diagnostics and a ready document batch cannot monopolize input.
Unsupported URIs are drained without waiting for a worker wake that cannot occur.

Relevant file-watch and workspace-folder notifications schedule language reloads
even when no document is pending. With positive debounce, initial loading is
scheduled after `initialized`. Zero mode keeps its existing immediate scripted
flush behavior, including initialization.

## Fresh requests and cancellation

Supported document requests wait asynchronously when their analysis is pending
or running. Workspace-symbol requests wait for all current open-document analyses.
Unserved languages and unsupported URIs settle with the established empty/null
results. Unknown methods still fail immediately.

- A document edit or relevant language change invalidates affected waiting
  requests with ContentModified (`-32801`); workspace-symbol waits are invalidated
  by any accepted document/workspace change.
- `$/cancelRequest` cancels the matching waiting request with RequestCancelled
  (`-32800`), exactly once. Numeric and string IDs remain distinct. It does not
  cancel unrelated requests or required diagnostic work.
- At most 256 requests wait for analysis; overflow receives ServerCancelled
  (`-32802`). This bounds the deferred queue, not all client transport buffers.
- Jobs carry document-incarnation/update and workspace generations. An obsolete
  result cannot replace cached analysis or publish diagnostics, including after
  close/reopen with the same client version. Document diagnostics include the
  current client version.

Jobs own their text, registry routing snapshot and open-buffer snapshot. Loaded
languages and previous analyses are shared with `Arc`; flat trees are not copied
for a job. Existing canonical-path lookup preserves open-buffer precedence.
Only the main thread commits a result or writes JSON-RPC.

## Lifecycle and remaining boundaries

Shutdown drains accepted analyses and waiting requests before acknowledging.
Pipelined exit/EOF after an accepted shutdown preserves that barrier; bare
exit/EOF terminates without joining in-flight analysis. Late results are dropped.

Cancellation skips obsolete work before it starts and rejects obsolete results
once it finishes. **It does not interrupt an in-flight parser or component call.**
Existing engine limits still apply. Unwinding worker panics produce an internal
error and complete affected waiters; release builds retain `panic=abort`, so no
release panic-isolation guarantee is added.

Registry preparation/reload, grammar-index construction, request response
formatting and RPC writes remain synchronous on the message loop. Continuous
accepted edits may keep resetting the global debounce interval, including edits
to another document. Per-document timing and cooperative parser interruption
remain possible follow-ups. No end-to-end GUI latency claim is made here.

## Evidence and reproduction

The original framed regressions reproduced diagnostic starvation under unrelated
traffic and unscheduled idle file-watch reloads. Background integration briefly
reintroduced starvation by placing completions behind queued input; the bounded
completion slot repairs that and retains the original passing tests.

Seventeen controlled protocol tests use channels and explicit worker gates, not
large-file timing guesses. While analysis is blocked, they prove unrelated
responses and cancellation proceed. They cover stale edits, invalid transactions,
close/reopen, language invalidation, workspace barriers, unserved/unsupported
URIs, waiting-request limits, EOF, shutdown and failure recovery. Five worker
boundary tests cover execution, logical cancellation, retained analysis release
and recovery from unwinding panics. Tests compare results with the established
analysis path; existing navigation/hints/formatting/incremental suites remain gates.

Independent whole-change review found one liveness issue: skipping an unsupported
URI could strand a due batch. A directed mixed-URI regression failed before the
pump was changed to drain skipped entries and settle their waiters. It passes
with the fix. The reviewer ran no tests/builds and did not judge GUI performance.

Fresh verification passed:

- Full workspace: 730 passed test executions, three ignored.
- Native-only LSP: 208 tests; native-only real-binary stdio: one test.
- An additional typed protocol parity test passed in both component-enabled and
  native-only modes: eleven request types across valid, invalid and edited input.
- Strict workspace all-target/all-feature clippy, formatting, and patch checks.
- Python release/benchmark tooling: 22 tests, including staged VSIX packaging.

```sh
cargo test --locked -p nitride-lsp --lib background
cargo test --locked -p nitride-lsp --lib async_tests
cargo test --locked -p nitride-lsp --test debounce
cargo test --locked -p nitride-lsp --no-default-features
cargo test --locked -p nitride-cli --no-default-features --test lsp_binary
```

Manual VS Code acceptance and hosted cross-platform results for the background
follow-up remain unverified. PR #43's preceding synchronous milestones passed
Linux, macOS, Windows, quality and release-tooling CI after the package-safe
README link fix. See the [design](superpowers/specs/2026-11-08-background-analysis-design.md)
and [plan](superpowers/plans/2026-11-08-background-analysis.md) for the follow-up scope.
