# Independent document analysis scheduling

The user requested continued work after merging PR #44. The merged worker still
uses one global debounce deadline/ready flag, so edits to one program postpone
other programs, and a fresh request for one file expedites all pending files.
This follow-up makes document scheduling independent without adding workers,
parser preemption, dependencies or new public options. Architectural choices are
engineering calls under the continued-development request.

Each pending document has its own quiet deadline. Accepted edits reset only that
file's deadline and remove its old ready placement. Stale/invalid changes and
unrelated traffic reset no deadline. Closing a file removes its timing state.
Documents become ready independently. The single worker selects the oldest ready
time, breaking ties by URI, so repeatedly updated files or new requests cannot
jump older ready work. An already-running job still finishes under existing limits.

Registry preparation is shared. Relevant grammar/catalog/project, signed-grammar,
watch or workspace-folder changes invalidate the affected generation as before
and debounce a shared reload. All open documents wait through that shared quiet
period. A later ordinary edit can extend its own quiet period without extending
that of the other files. Loading the registry must not lose their timing state.

A document-feature request expedites only its target, loading a pending registry
first if necessary. Other files retain their deadlines. Already-due files remain
older-ready priority; the request does not jump ahead of them. Workspace-symbol
requests and shutdown intentionally expedite all pending documents. Cancellation,
version/incarnation gates, bounded requests, input/completion fairness, bare EOF/
exit and shutdown draining remain as implemented in PR #44. Zero debounce stays
synchronous and exact for scripts.

Use a small private timing unit with explicit Instant arguments so unit tests can
advance synthetic time without sleeps. It holds document deadlines, ready times
and the shared reload deadline. The existing pending set remains the source of
which analyses are dirty. Skip unsupported URIs without requiring an extra wake.
No global-ready flag or lexical-URI-first selection remains in worker dispatch.

Acceptance: reproduce cross-file starvation in framed traffic before fixing it;
prove a request for A does not start not-yet-due B; test independent deadlines,
shared reloads, oldest-ready ordering, resetting a ready file, close/reopen and
expediting all via fake time. Retain the gated worker, unsupported-URI, request
cancellation, shutdown, typed protocol parity and all LSP suites in both feature
configurations. Run real CLI stdio, strict clippy, formatting and packaging checks.
One independent design check and final implementation review complete the work.
No wall-clock GUI latency or parser speed improvement is claimed.

Work starts on codex/lsp-document-scheduling from merged main 62bcccb in the
existing isolated workspace. The original main workspace is fast-forwarded while
its three pre-existing untracked local entries remain preserved. No merge or
release is requested for this new follow-up.
