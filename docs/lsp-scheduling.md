# LSP diagnostic scheduling

The default server waits for 150 ms of quiet after an accepted document change or
relevant workspace/language-input notification. Unrelated requests, ignored
notifications, stale versions and invalid edit transactions leave that deadline
unchanged. Once due, diagnostics run even while unrelated input remains queued.

The loop handles one queued message before enforcing expiry. An accepted edit can
reset the deadline; a close removes its pending document; exit can terminate.
Unrelated traffic cannot postpone an expired job beyond that handled message.
Requests for a document still bring its analysis up to date immediately.

Relevant file-watch and workspace-folder notifications schedule language reloads
even when no document is pending. A changed language invalidates and reanalyzes
its open programs through the existing registry path. With positive debounce,
initial loading is scheduled after the client's `initialized` notification.
Zero debounce retains the existing scripted behavior of flushing after every
message, including initialization.

## Evidence

Two framed LSP regressions failed before the fix: pending program diagnostics and
file-watch-triggered diagnostics both waited until a stream of unrelated requests
ended. Both now publish before the last unrelated response. Tests deliberately
slow output to keep input queued beyond a short debounce interval; assertions
check message ordering, not a machine-specific latency bound.

Additional tests cover accepted edits resetting the deadline, stale/invalid edits
preserving it, ignored notifications and irrelevant watched files, positive-
debounce initialization, and closing a pending document. Existing zero-debounce,
UTF-16 synchronization, reload, requests-before-debounce and shutdown tests remain
part of the LSP suites.

Fresh verification passed:

- Component-enabled LSP suite: 187 tests.
- Native-only LSP suite: 186 tests.
- Real `nitride lsp` binary over stdio: one integration test.
- Strict all-target/all-feature LSP clippy.

Reproduce the new regressions with
`cargo test --locked -p nitride-lsp --test debounce`; the deadline-state checks
run with `cargo test --locked -p nitride-lsp scheduling_tests`.

Independent static review identified the need to check expiry after handling the
next queued message; that ordering was corrected before final verification. The
reviewer did not run tests or builds. Formatting and patch checks passed
before committing. Manual VS Code and hosted cross-platform checks were not run.

## Remaining boundaries

Analysis still runs synchronously on the server thread. This fixes diagnostic
starvation and reload scheduling; it does not make long analysis interruptible.
Continuous accepted edits may continue to reset the global debounce deadline,
including edits to another document. Per-document scheduling, background analysis
and cancellation require further work. No new parse-speed or end-to-end GUI
latency measurement is claimed by these ordering tests.
