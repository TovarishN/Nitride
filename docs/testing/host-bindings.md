# Direct host-binding crate coverage

November 10, 2026. `nitride-process` and `nitride-wasm` now have direct tests that
run without the CLI's generated languages or host helpers. These supplement the
existing CLI Calendar integration suites.

```sh
cargo test --locked -p nitride-process -p nitride-wasm
cargo test --locked -p nitride-cli --test process --test component
```

## Process boundary

[Protocol tests](../../crates/nitride-process/tests/protocol.rs) compile a small
[std-only Rust child](../../crates/nitride-process/tests/support/host.rs) with the
active `rustc`, run it in temporary directories, and inspect actual requests and
replies. No Python installation is required. Child processes are owned by the
bindings; the tests release them before removing their fixture directories.

Coverage includes sequential request IDs, export-name normalization, Unicode
and newline text inputs, numeric/integer/bool/text parameters, nested records and
lists, EOF, malformed JSON, wrong IDs, invalid protocol versions, malformed error
objects, declared failures and unknown failure names.

The tests exposed two failures: JSON-RPC 1.0 replies were accepted, and incomplete
outgoing records were silently truncated by `zip`. Replies now require a 2.0
envelope with exactly one result or error, and errors require an integer code
and string message. Invalid replies remain `host.process` failures rather than
being mistaken for declared domain failures. Outgoing values must match their
catalog representations and record field counts; malformed fields, wrong input
counts and nonfinite scalar inputs fail before a request is written.

## Component boundary

[Component tests](../../crates/nitride-wasm/tests/component.rs) load the checked-in
[Calendar component](../../languages/calendar/gregorian.wasm) directly, with small
handwritten catalog/signature fixtures. Crate-local converter tests also inspect
actual `Val` shapes independently of the reverse conversion.

Coverage includes repeated real calls, records/lists, declared errors, fuel
exhaustion and subsequent unmetered loading, same-arity parameter/result type
mismatches, record field-name mismatches, non-string export metadata, incomplete
records and malformed record-field metadata. No `nitride-cli` dependency or
component rebuild is needed to run these tests.

Loading now validates parameter and successful result shapes, including whether
a failure-result wrapper is required. Record names and order participate in
compatibility checks. Invalid export/field metadata returns errors instead of
panicking, and outgoing values reject representation or arity mismatches before
component invocation.

## Shared naming contract

Review caught a compatibility problem in the new record-name check: the
adapter's old naming function disagreed with generated WIT for numeric suffixes
and repeated underscores. Code generation and both host adapters now share
`nitride_core::host::binding_name`. It preserves the generator's existing output:
`date_2` becomes `date2`, `start__day` becomes `start-day`. WIT source generation
continues to add keyword escapes; runtime component/protocol names omit them.
Regressions exercise conversion, real record binding and declared process
failure mapping. Existing WIT golden and parser tests remain unchanged.

## Limits of this checkpoint

This adds validation at the existing synchronous boundaries. It does not add
process response deadlines, asynchronous transport, process pooling, exhaustive
catalog-schema validation or a new component cache policy. The crate APIs still
expect prepared catalog metadata, and host/guest execution has its existing
resource limits. The Calendar component is a shared repository fixture, not a
new independently authored component implementation. Local macOS results do not
claim hosted Linux/Windows acceptance.
