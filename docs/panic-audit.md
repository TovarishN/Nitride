# Panic audit: interface examples

November 10, 2026. This is a focused audit of input boundaries, not a claim that
all Rust panics or unsafe-storage defects have been eliminated. Release builds
retain `panic = "abort"`.

## Findings and changes

The native parser checks stack, depth and work limits, but catalog decoding and
interface-example execution happened after those checks. Three independent
recursive walks were unbounded:

1. `nitride-prepare` built owned example-expression trees recursively from parsed catalogs.
2. Core conformance recursively searched examples for denied effects.
3. Core conformance recursively evaluated nested operation calls.

Regressions using 256 nested calls were accepted before the fixes. The tests ran
on a deliberately large thread stack to expose the missing bound through an
assertion instead of aborting the test process. This establishes the unbounded
path; it does not assert a universal stack-overflow threshold.

Catalog extraction now rejects expressions beyond 128 nesting levels with an
error at the offending source span. A shallow placeholder lets error collection
finish; module errors prevent those expressions from reaching lowering. The
owned AST, subsequent lowering and cleanup therefore have bounded nesting.

Core effect inspection and evaluation now use explicit work stacks. Existing
`host::conform` and `host::conformance` use default limits. The additive
`host::conformance_with_limits` accepts a per-example expression budget shared by
the value and expected sides. Depth is the smaller of the caller's limit and
`host::MAX_EXAMPLE_DEPTH` (128). Root depth is zero. The complete value and
expected expressions are inspected before any host operation executes.

A resource-limit violation reports a failed conformance case with a depth/work
message. It cannot satisfy an expected interface failure. Bounded cases needing
denied effects remain skipped. Operation order, literal text inputs, contract
checks, declared failures and finite-result handling retain their existing
semantics. Host effects that occur before an ordinary host failure are still
not rolled back.

## Invariants inspected and retained

The initial search included `unwrap`, `expect`, explicit assertions and
`unreachable`, followed by inspection of their callers. Raw textual counts also
include tests and do not measure reachable editor crashes.

| Area | Inspected guard or invariant | Decision |
|---|---|---|
| [Flat parser](../crates/nitride-core/src/parser.rs) and [shared snapshot publication](../crates/nitride-core/src/parser/shared/snapshot.rs) | Root unwraps follow a return on missing roots | Retain |
| [Source text](../crates/nitride-core/src/syntax/text.rs) | Range, length and UTF-8 checks precede piece lookup and slicing | Retain |
| [Semantic checking](../crates/nitride-core/src/semantic.rs) and [templates](../crates/nitride-core/src/templates.rs) | Syntax identity, span, child-range and postorder validation precede lowering; overload selection rejects missing/ambiguous choices | Retain |
| [Execution](../crates/nitride-core/src/execution.rs) | Whole-program preflight validates roots, operation/argument indices, symbol visibility and exact bindings before indexed execution | Retain |
| [Shared syntax storage](../crates/nitride-core/src/syntax/arena.rs) | Ownership, live-node and reference-count assertions detect broken internal storage contracts | Retain; no silent fallbacks |
| [Host examples](../crates/nitride-core/src/host.rs) and [catalog decoding](../crates/nitride-prepare/src/catalog.rs) | Parser guards did not bound later recursion | Add limits and iterative traversal |

These inspections do not prove every invariant. In particular, the unsafe arena
and shared-reader implementation still rely on their ownership tests and need
continued fuzzing and review. Caller-supplied Rust handlers can panic inside
application code; this change does not catch such panics or change the release
panic strategy. Work units count expression visits, not host execution time or
all binding-lookup costs. Limits reset per example, rather than bounding an
entire suite. Existing JSON, parser and language-admission limits also continue
to apply.

## Regression coverage

[Core tests](../crates/nitride-core/tests/host_examples.rs) cover:

- Oversized value and expected expressions rejected before host calls.
- Exact depth/work boundaries and a work budget shared across both sides.
- Accepted depth-128 and rejected depth-20,000 cases on a 64 KiB thread stack.
- Left-to-right host-call order, Unicode literal text, denied-effect skipping and per-example budget reset.

[Catalog tests](../crates/nitride-prepare/tests/catalog.rs) reject a deeply nested
source example and check that its diagnostic identifies a valid source span.
Ordinary Calendar and other interface suites remain integration coverage for
existing conformance behavior. See [incremental fuzzing](../fuzz/README.md) for
the separate parser/edit campaign and its limits.
