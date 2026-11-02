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

1. **One-shot parses skip the reuse index.** A parse option says whether the
   snapshot will be edited. Without it, no recognition facts are recorded,
   hashed, sorted or packed, and the snapshot keeps no index. Editing such a
   snapshot still works and reparses fully (counted in `full_parse_fallbacks`).
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

- Goal 1 is an explicit option, not a lazily built index: the index holds
  lookahead extents (`observed_offset`, `furthest_offset`) recorded during the
  parse that cannot be recovered from the tree afterwards.
- The JSON bench uses the one-shot mode for the STJ comparison, which is the
  fair comparison since STJ cannot reuse work after edits; the doc says so.
- Order of work: one-shot mode, reusable parser, memory, PGO, then the doc —
  each measured before the next.

## Left to the builder

- The option's name and where it lives (`ParseOptions`, `SnapshotMode`, or the
  parser object).
- Which scratch structures the reusable parser keeps, and how it caps what it
  retains after an unusually large input.
- Which syntax-layout changes to make for goal 3, guided by `storage_stats`
  and a heap counter.
- Adding an edit benchmark (a few single-character edits on the 1 MiB
  inputs) to show editable parses did not regress.
