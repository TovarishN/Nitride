# Background document analysis and request cancellation

## Intent and authorization

The user requested a PR and continued development after the measured editor and
scheduling improvements. PR #43 contains the completed milestones. This follow-up
moves expensive document analysis off the LSP message loop while retaining fresh
request results. Nitride develops independently; compatibility with another engine
is not an acceptance criterion. Architecture choices below are engineering calls.

## Behavior

With positive debounce (the normal server), one background worker analyzes an
owned immutable job. The main thread alone owns documents, scheduling, registry
updates, output and request bookkeeping. It continues processing edits, unknown
requests, lifecycle messages and cancellation while document analysis runs.
Zero debounce retains the established synchronous scripted mode.

Document-feature requests that need pending/in-flight analysis wait asynchronously
for fresh results. Unknown methods still fail immediately. Workspace-symbol
requests wait for all pending/in-flight open-document analyses. Results are never
answered from stale text. Requests already waiting when their document changes
receive ContentModified (-32801); workspace-symbol waits are invalidated by any
document or workspace change. $/cancelRequest returns RequestCancelled (-32800)
for the matching deferred request; other requests and diagnostic work continue.
Request IDs retain numeric/string identity. At most 256 requests can wait; overflow
receives ServerCancelled (-32802). This bound is a builder choice.

An accepted edit, close/reopen or language/workspace invalidation makes relevant
old jobs obsolete. They cannot replace analysis or publish diagnostics. Generation
keys include document incarnation/update and workspace generation, not just client
version. Valid published document diagnostics include the client document version.
Language ownership and old immutable snapshots remain retained while a worker
still reads them, then release normally.

## Worker and ownership

Exactly one job is outstanding; the event loop coalesces pending URIs rather than
building an unbounded worker queue. Jobs own text, a snapshot of open buffers, a
cloned registry containing Arc<Loaded> language owners, and Option<Arc<Analysis>>
for the previous analysis. This avoids copying flat trees. Registry clone copies
small routing/error metadata, not prepared language payloads.

A job has a unique ticket, URI, document/workspace generation stamp and cancellation
flag. Obsolete queued jobs skip analysis. In-flight work is not preempted: current
parser/semantic/component work retains its existing finite limits. Cancellation
means prompt request cancellation and rejection of obsolete results, not immediate
CPU interruption. Worker completion always clears its outstanding ticket. In unwinding builds, panics
are caught and become internal-error responses for affected waiting requests and
a diagnostic; they must not strand the scheduler. Only the main thread writes RPC. Release builds retain the existing
`panic=abort` setting, so a panic there still terminates the process; no release
panic-isolation guarantee is added.

Completion and input share the message-loop event channel. Input EOF is explicit
so a worker sender cannot keep the loop waiting after client disconnect. Exit/EOF
cancel/drop outstanding work without joining a still-running bounded computation.
Shutdown stops accepting new work, finishes already accepted analyses and deferred
requests, then sends its acknowledgement. Zero-mode lifecycle remains unchanged.

## Scheduling and requests

Existing debounce deadlines still follow accepted edits and relevant inputs.
A due batch becomes ready; one job starts and completion starts the next ready
URI. While a job runs, clearing an expired deadline prevents a busy-spin loop.
New edits reset the global quiet interval; a fresh document request can expedite
its own pending work. Requests on closed/unserved documents return established
empty/null results rather than looping forever for an absent analysis.

All supported textDocument requests use a freshness barrier before ordinary
request dispatch. Deferred replay calls the ordinary request logic only when
its required analyses are settled, avoiding a synchronous parse on the event loop.
Registry preparation/reload, grammar-index construction and response formatting
remain on the main thread in this milestone. This is explicitly not a claim that
every operation is asynchronous or that in-flight parsing can be interrupted.

## Acceptance

Use controlled worker gates and channels, rather than large-file timing guesses:
while analysis is deliberately blocked, prove an unrelated request is answered,
a deferred document request can be cancelled exactly once, and accepted edits,
close/reopen with the same client version and language reload prevent old results
from publishing. Releasing the worker must yield fresh current results identical
to synchronous analysis. Cover workspace-symbol barriers, unknown languages,
shutdown acknowledgement, EOF/exit, bounded waiting requests and a worker panic.
Existing incremental, scheduling, navigation, hints, formatting, runtime owner
release and real-binary tests must remain green in both component/native modes.

An independent builder checks this design before implementation; one independent
whole-change review follows implementation. Keep this follow-up separate from
PR #43 once its hosted validation is settled. No merge or release is requested.
