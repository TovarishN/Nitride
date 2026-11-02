# Parse throughput and memory against System.Text.Json

## Intent

The JSON comparison (`docs/json-comparison.md`) shows .NET's System.Text.Json
(STJ) 5–11× faster than Nitride on warm parses, with the ratio growing from
1 MiB to 16 MiB inputs, and Nitride's process peaking at 460–860 MiB where STJ
stays near 90 MiB. STJ's warm advantage shows the work is there to be saved.
The goal is to squeeze that performance out of Nitride's full parse: less time
and less memory per parse at every size, and repeated parses in one process
that are cheaper than the first.

## What the profile says (16 MiB numbers input, before any change)

- Time per node is nearly flat: 311 ns at 1 MiB, 325 ns at 16 MiB.
- About 16% of the time is the edit-reuse index: a hash map of recognition
  facts filled during the parse (`pending_index`, rehashing as it grows), then
  sorted (O(n log n)) and packed into `incremental::dependencies::Index`.
- Memory: 102 MB kept after the parse (syntax ~36 MB, reuse index ~38 MB,
  source 16 MB), heap peak 237 MB, process peak 460 MB.
- Nothing survives between parses: memo tables, child buffers, the arena
  builder and the hash maps are allocated anew each time, so warm ≈ cold.

## Goals

1. **Cheap facts during the parse, the index built only when needed.** Every
   parse still records its recognition facts, since an editor may edit any
   snapshot, but in a form that is cheap to write and quick to turn into the
   index:
   - an append-only log of compact fact records, which a counting sort by
     position (bounded by the text length) orders in linear time;
   - a slot replaced in place when a fact is re-recorded at the same position
     and rule, so the log never holds duplicates;
   - the in-parse lookups (`get`, `contains_key`, eviction order) served by a
     small position-and-rule-to-slot map, or by the memo, instead of today's
     map of full records.

   The snapshot keeps the log and packs it into
   `incremental::dependencies::Index` on its first edit, in linear time. A parse that is never edited never packs.
2. **A reusable parser.** A parser object that is kept across parses and holds
   on to its scratch memory (memo, buffers, arena builder capacity, maps) so
   that later parses in the process allocate little. The one-shot functions
   become wrappers over it.
3. **Less memory kept and at peak.** Shrink the published syntax per node and
   the transient peak during the parse (pre-size from input length, release
   scratch early, avoid doubling growth on the largest buffers).
4. **PGO numbers in the comparison.** Measure the bench with the existing
   release PGO harness, since it was worth 32–35% on C#.
5. **Updated comparison.** Re-run `scripts/json-bench` and rewrite
   `docs/json-comparison.md` with before and after, adding a "warm reused
   parser" column for Nitride.

## Success (my call; measured on the existing corpus, same host)

- The Nitride ÷ STJ-document warm ratio no longer grows from 1 MiB to 16 MiB,
  and is lower than today at every size.
- Process peak on 16 MiB records/numbers drops by at least half.
- Warm parses with a reused parser are measurably faster than cold.

These are directions, not hard gates: each change ships only if the bench
shows it helps, and the doc reports what was achieved.

## Non-goals

- Value decoding (unescaping, number conversion), `JsonSerializer` parity.
- Strict-JSON trivia (comments and Unicode whitespace stay accepted).
- Parallel parsing.

## Anti-goals

- Slower editor re-parsing after edits, or worse error recovery. Editable
  parses (the LSP path) keep building the index exactly as now.
- Wins that only exist in the bench (special-casing JSON in core).
- Unsafe code added without a test that would catch its misuse.

## Constraints

- The public parsing API may change freely; update in-repo callers
  (LSP, CLI, codegen output, tests, benches) in the same change.
- Every existing test stays green; `cargo clippy` stays at zero warnings.
- Generated parsers and the runtime loader keep producing identical trees.

## Decided

- Goal 1 keeps every snapshot editable (no opt-out flag) and moves the cost
  rather than removing it: a little help during the parse (the ordered,
  duplicate-free log) makes the index fast to build later. The index cannot
  be rebuilt from the tree alone, because it holds lookahead extents
  (`observed_offset`, `furthest_offset`) recorded during the parse.
- The first edit of a snapshot pays for packing the index. That cost is
  measured on the 16 MiB inputs and reported; if it is too high for editor
  use, the snapshot packs eagerly on a background thread instead.
- The JSON bench measures the default parse, log included, so the comparison
  needs no special mode.
- Order of work: fact log and deferred index, reusable parser, memory, PGO, then the doc —
  each measured before the next.

## Left to the builder

- The fact record's exact layout, and whether the in-parse lookups use a slot
  map or the memo.
- Whether an explicit "never edited" opt-out is still worth adding, decided by
  what the log costs on the 16 MiB inputs.
- Which scratch structures the reusable parser keeps, and how it caps what it
  retains after an unusually large input.
- Which syntax-layout changes to make for goal 3, guided by `storage_stats`
  and a heap counter.
- Adding an edit benchmark (a few single-character edits on the 1 MiB
  inputs) to show editable parses did not regress.
