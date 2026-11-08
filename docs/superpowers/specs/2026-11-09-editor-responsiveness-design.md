# Editor responsiveness evidence

Measure the merged native LSP before selecting a new engine representation.
Use a disposable VS Code extension host with the actual extracted VSIX and its
bundled binary, a fresh user-data directory and no configured external server.
Verify diagnostics across two documents, rapid edits, grammar reload and a
feature-provider result. Record edit-to-diagnostic times as extension-host
observations, not painted-frame latency or visual acceptance. Cancellation is
covered at the protocol boundary; VS Code's public provider commands do not
expose the language client's request cancellation token.

Refresh in-process phase measurements on the same merged source for generated
256 KiB and 4 MiB C# syntax inputs, valid and damaged. Check incremental outputs
against fresh analysis outside timing. Add a real typed-language workload so
syntax-only C# does not stand in for semantics. Preserve raw samples, input and
binary hashes and build configuration. Do not run tests/builds during timing.
No performance improvement claim without a matched candidate. Record the next
optimization based on measured costs. Parser interruption and scoped reloads
remain distinct subsequent changes.

This milestone delivers reproducible acceptance/measurement tooling and its
baseline report. A representation rewrite follows the evidence as its own PR.
