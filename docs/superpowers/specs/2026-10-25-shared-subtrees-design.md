# Shared subtrees during parsing and across edits

Status: approved by the user with "proceed" on 2026-10-25. The independent
builder check found no important unanswered questions. No implementation is
included in this document.

## Intent and agreed scope

The user confirmed both forms of in-place sharing: parser alternatives reuse
the existing storage of a successful subtree, and successive edited snapshots
retain unaffected subtrees in that same storage. Rebuild affected structure;
keep previous snapshots valid for existing readers.

Use one immutable syntax representation for both. Remain on ordinary stable
Rust/std targets with no third-party native-core dependency. Preserve the
admitted grammar boundaries and existing observable parsing behavior.

This develops milestone C of the approved CSharp4/recovery/incremental design.
LSP/editor integration and incremental semantic analysis remain separate work.

## Current costs and constraints

`syntax.rs` stores absolute byte spans in flat node/edge vectors. IDs are dense
postorder indices. Parser rollback truncates those vectors. Successful Pratt
memo entries copy node/edge suffixes into separate arenas; hits copy them back
and rebase indices. Alternative winner selection also copies suffixes.

Recovery's strict probes retain successful trees, compact selected suffixes,
and copy strict subtrees into the recovered tree. Its sidecar stores coverage,
repairs, diagnostics and damaged-node IDs. Binding, templates and lowering
index dense arrays and depend on postorder traversal. These are explicit
migration sites, not compatible assumptions to leave behind silently.

## Storage model

### Immutable structure and snapshot placement

A completed structural node describes its rule, consumed width, visible text
range relative to its consumed extent, and a child sequence. It contains no
absolute source offset, parent pointer, snapshot ID or mutable semantic result.
The distinction between consumed extent and visible span preserves existing
trivia ownership, token leaves, aliases and transparent rules exactly.

A snapshot owns a root occurrence, persistent source text and recovery data.
A lightweight occurrence/cursor combines structural identity with placement in
that snapshot. Absolute spans and text are resolved through the occurrence.
The same structure may occur in different snapshots or positions; structural
identity is never sufficient to identify a binding or diagnostic occurrence.

Child labels have grammar-stable meaning across chunks and snapshots. Use a
language-owned label table or immutable label context; never reinterpret a
chunk's label index through an unrelated snapshot-local table. Language and
parse options are part of the compatibility context.

### Wide nodes and source text

Child sequences are persistent and balanced/chunked. Their entries represent
child placement through widths and intervening gaps, with aggregate widths
for seeking. Editing an early child must not rewrite absolute offsets of all
later siblings or clone an entire class/file child array. Relative-to-parent
offsets may be derived by traversal; storing a flat array of such offsets is
insufficient for this requirement. Literal-only/trivia gaps remain represented
even when the compact grammar tree does not expose corresponding nodes.

Source text uses a persistent balanced piece/chunk representation. Snapshots
share unchanged buffers and edit only affected paths. Ranges crossing pieces
may require an owned string or iterator; internal readers must avoid flattening
the whole document to obtain text. UTF-8 byte positions remain the edit/span
contract, and invalid boundaries or overflow return structured errors.

### Arenas, lifetime and reclamation

Allocate nodes/edges in chunks with compact stable handles, not one allocation
or reference count per node. A mutable construction chunk is private to its
builder; published records are immutable. Growth may move backing allocations
without invalidating handles. Immutable sealed chunks may be shared by readers.

Snapshot and chunk ownership must retain every reachable dependency without
cycles. References within a chunk do not reference-count the chunk itself.
Cross-chunk ownership and construction/sealing rules must prevent chunk cycles
even though the node graph is acyclic. Bound traversal and destruction stack
use, including when a long snapshot history is released.

Snapshots share the storage directory too: publishing an edit must not clone
a directory proportional to the complete document or its edit history.

Memo eviction releases memo ownership, not nodes still reachable from a winner
or live snapshot. Bound speculative retention through explicit budgets and
safe-point reclamation. Whole-document compaction is allowed as an explicitly
reported maintenance operation, never hidden in each edit. Sparse-chunk
evacuation, if needed, creates a replacement representation for new ownership;
it cannot invalidate handles held by old snapshots. A tiny live subtree must
not silently retain an unbounded chain of abandoned speculative chunks.

## Sharing within a parse

Marks restore position and temporary construction state rather than truncate
published successful nodes. Memo entries and alternative winners hold a root
handle plus the scalar result/context data. A memo hit performs no descendant
copy, rebasing or traversal. Winners retain handles instead of copying suffixes.
Do not introduce global hash-consing of identical text; reuse already-known
parse results first.

Preserve ranking, ambiguity, precedence, failure/furthest information, aliases,
token options, lexical exclusions, predicates and parser limits. Audit the
existing memo key against all result-affecting modes rather than assuming its
current `(point, position, minimum)` tuple is universally sufficient.

Recovery probes and selected strict subtrees use the same handles. Temporary
search paths remain separate from immutable syntax. Reachability reclamation
replaces truncation/packing only where it preserves the existing search result.

## Reuse across edits

Provide a snapshot/edit entry point alongside existing strict and recovery
entry points. An edit is a replacement byte range and inserted UTF-8 text.
Initial implementation may accept one edit at a time; batches can be applied
sequentially with explicit snapshot-relative coordinates.

Reuse requires matching grammar identity, start rule, parser modes, template
context where applicable, and compatible parsing context such as precedence.
Record dependencies of successful and failed recognition: consumed text,
lookahead/predicates, trivia and lexical scanning, failed competing alternatives,
EOF observations, and continuation requirements. Track these transitively or
conservatively widen the invalidated region. `furthest` alone is not a proof.
Insertion at a boundary, including EOF, can invalidate a zero-width dependency.

Map old dependency ranges through the edit, invalidate affected results, then
reparse from a sound checkpoint until unchanged continuation/context permits
reuse. Identical subtree text does not establish that it is still selected by
the grammar. Changing a boundary can extend a token/comment, change precedence,
or change the winning alternative.

Fallback to a larger region or a complete parse whenever reuse cannot be
proved. Limits apply to work actually done; cache reuse must not bypass depth,
storage or other structural limits. Specify that resource exhaustion can differ
between warm and cold runs; it is reported explicitly and cannot be disguised
as equivalence of successful parse results.

Recovered snapshots are supported, with conservative invalidation of the repair
region and all search/continuation dependencies. Start by redoing recovery for
the enclosing uncertain region, up to the whole file. Reuse repaired structures
only with equivalent repair context. Coverage, repairs, diagnostics and damage
must resolve to the new snapshot, including unchanged regions shifted by edits.
Sidecar storage must eventually share unaffected ranges; rebuilding whole-file
coverage each edit would defeat the incremental goal. Full recovery fallback
is correct but is not evidence of practical incremental latency on broken text.

## Consumers and compatibility

Keep current observable syntax shape, field labels, text, absolute spans,
diagnostic ordering, repairs, coverage, template behavior and semantic results.
Physical node numbers/storage order may change; persisted observations should
compare normalized occurrences, not allocator identities.

Introduce a shared snapshot API for traversal/text/spans and explicitly owned
flat materialization for compatibility/export. Existing parse APIs can retain
the flat result during migration, backed by the new parser and one final export.
The incremental entry point must return the shared snapshot directly and never
materialize all nodes automatically.

Move semantic/template consumers behind traversal and occurrence accessors.
They may build dense occurrence tables when compilation is explicitly requested;
that cost is measured as semantic work, not hidden in parsing. Node identity
across snapshots does not imply reusable bindings, types or HIR: enclosing
scope, declarations and semantic catalog identity can change.

Flat export preserves the postorder contract for existing public-vector users.
Occurrence IDs used by consumer side tables are snapshot-local; structural
handles used to prove sharing are a distinct type. Any eventual removal of
public vectors is a separate API migration, not a silent compatibility change.

## Delivery order

1. Freeze current behavioral/performance baselines and add sharing/copy counters.
2. Introduce the representation, occurrence access and explicit flat export.
3. Replace memo, winner and strict-probe copies with shared handles; bound
   speculative ownership and verify cold-parse behavior.
4. Add persistent text/child editing, dependency tracking and strict edit reuse.
5. Add conservative recovery invalidation and persistent sidecars, then widen
   proven recovery reuse without changing repair decisions.
6. Migrate internal consumers, publish edit/memory evidence and integration
   results. Editor hookup and incremental semantics stay outside this scope.

These stages form one project; parser-only sharing does not complete the
confirmed request. The implementation plan must make each stage reviewable.

## Acceptance and measurement

- Preserve existing workspace tests, actual Nitrogen parity where applicable,
  preparation parity, grammar bootstrap fixed point and native deployment gates.
- Compare shared full parsing and edited snapshots to full reparsing of the new
  text: normalized syntax, spans, text/labels, diagnostics, coverage, repairs and
  damage; for typed fixtures compare binding/type/HIR results after compilation.
  Full parsing with reuse disabled and existing frozen observations are both
  needed, so shared implementation bugs cannot validate themselves.
- Exercise deterministic seeded edit sequences on valid and recovered inputs:
  token/comment boundaries, predicates, precedence, aliases, Unicode, EOF,
  missing tokens, nested repairs and edits to enclosing scope. Retain multiple
  old snapshots and validate them after later edits.
- Prove identity reuse and zero descendant copying on memo hits. For separated
  methods, edits to one method reuse the handles of unaffected methods whose
  parsing dependencies remain valid. Assert changed syntax is not reused.
- Show child-sequence and source edits avoid whole-file copying in wide files,
  and report work/copy counts separately from wall-clock latency.
- Measure Release cold strict/recovery time, memo copies, allocations, requested
  peak/live bytes, unique shared bytes, and chunk pinning. Measure 256 KiB,
  4 MiB and 16 MiB files, with early/middle/late edits and one/dense/junk/
  truncated error patterns. Publish p50/p95/max edit latency and fallback rate.
- For repeatable bounded-size documents, run at least 10,000 edits both keeping
  only the latest snapshot and keeping a bounded reader window. After readers
  and caches are released, retained syntax/source bytes must be bounded by live
  content plus documented chunk/budget overhead, not by edit count. Separately
  measure deliberately retained history and its expected unique storage.
- Include deep nesting, wide lists, cache eviction, sparse chunks, limit paths,
  source/piece fragmentation and old-snapshot destruction without stack failure.
- Publish before/after evidence for the complete workload matrix. Do not infer
  a speed or memory benefit from the representation alone. Numeric latency and
  regression thresholds are proposed in the implementation plan from the
  freshly captured baseline and approved before implementation starts.

## Builder choices and approval boundary

The representation, staged migration, persistent source/child sequences and
conservative recovery policy above are the author's proposed implementation
choices; the user's confirmed requirement is in-place sharing for both cases.
The builder chooses chunk sizing, compact handle layout, persistent sequence
algorithm, checkpoint indexing and exact API names, with evidence for their
memory and traversal costs. No unsafe pointer design is required.

Design approval leads to an implementation plan, including budgets and numeric
performance gates. Product code, dependencies, commits, pushes and integration
have not been performed by writing this proposal.
