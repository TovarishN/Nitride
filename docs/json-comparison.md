# JSON parsing: Nitride vs System.Text.Json, 2026-11-02

A JSON grammar ([`languages/json/Json.ngr`](../languages/json/Json.ngr)) was
written for Nitride and its generated parser compared with .NET's
`System.Text.Json` on an **Apple M4 Pro**. Raw samples are in
[`json-comparison.json`](json-comparison.json), and the numbers from before the
throughput work in [`json-comparison-baseline.json`](json-comparison-baseline.json).
The harness is [`scripts/json-bench`](../scripts/json-bench).

## What is compared

| Engine | Work done |
|---|---|
| Nitride | Generated parser (`parse_snapshot_generated`, strict mode) building a shared syntax snapshot: one node per object, array, member, string and number with spans and labeled edges, plus the index that lets an edit reuse unchanged syntax. No string unescaping or number conversion. |
| Nitride PGO | The same code built with profile-guided optimization (`pgo.py`, trained on the 1 MiB inputs and an edit run). |
| STJ `JsonDocument.Parse` | Validates and builds JsonDocument's token index (the closest analogue of a syntax tree). No string unescaping or number conversion until accessed. |
| STJ `Utf8JsonReader` | Validates and tokenizes only; retains nothing. Lower bound. |

Nitride: LTO, one codegen unit, rustc 1.99.1. STJ: `net10.0`, default tiered
JIT, workstation GC. Cold is the first parse in a fresh process (STJ includes
JIT); warm is the median of 10 more parses in that process. Each cell is the
median of 5 fresh processes. Process start and file reading are excluded.
Inputs are synthetic (`corpus.py`): pretty-printed API-style records, compact
number arrays, and long strings with escapes.

## Results

Milliseconds, cold / warm (lower is better):

| Input | Nitride | Nitride PGO | STJ document | STJ reader | Nitride ÷ STJ document, warm | PGO ÷ STJ document, warm |
|---|---:|---:|---:|---:|---:|---:|
| records 64 KiB | 1.50 / 1.24 | 0.89 / 0.77 | 3.81 / 0.26 | 3.31 / 0.18 | 4.8× | 3.0× |
| records 1 MiB | 19.2 / 18.7 | 11.9 / 11.4 | 8.21 / 4.07 | 6.14 / 2.58 | 4.6× | 2.8× |
| records 16 MiB | 315 / 303 | 197 / 189 | 72.0 / 33.1 | 44.3 / 40.2 | 9.1× | 5.7× |
| numbers 64 KiB | 1.42 / 1.34 | 0.97 / 0.83 | 1.11 / 0.23 | 0.63 / 0.19 | 5.8× | 3.6× |
| numbers 1 MiB | 21.5 / 20.6 | 13.2 / 12.4 | 4.60 / 3.67 | 3.60 / 2.95 | 5.6× | 3.4× |
| numbers 16 MiB | 360 / 343 | 230 / 214 | 61.3 / 46.6 | 48.5 / 41.6 | 7.4× | 4.6× |
| strings 64 KiB | 0.23 / 0.18 | 0.19 / 0.14 | 3.63 / 0.02 | 3.20 / 0.02 | 11× | 9.1× |
| strings 1 MiB | 2.74 / 2.69 | 2.38 / 1.94 | 3.92 / 0.31 | 3.50 / 0.30 | 8.8× | 6.3× |
| strings 16 MiB | 44.5 / 43.5 | 38.0 / 32.1 | 9.14 / 4.89 | 8.72 / 4.89 | 8.9× | 6.6× |

Peak resident memory of the whole process (11 parses), MiB:

| Input | Nitride | STJ document | STJ reader |
|---|---:|---:|---:|
| records 16 MiB | 407 | 93 | 64 |
| numbers 16 MiB | 366 | 90 | 64 |
| strings 16 MiB | 64 | 63 | 63 |
| any 64 KiB | 3–5 | 47 | 47 |

## Before and after the throughput work

The changes (see the [design](superpowers/specs/2026-11-02-parse-throughput-design.md)):

1. During a parse, edit-reuse facts now go into a flat list, plus a small map
   from position and rule to each fact. Before, they went into a hash map of
   full records. Publishing sorts the list by position in linear time; before,
   it was an O(n log n) sort.
2. The fact list and its eviction order grow to the size the text so far
   predicts, instead of doubling.
3. The sort counts facts per 16-byte block of text, not per byte.
4. Profile-guided optimization, measured as its own engine.

| Measure | Before | After | After, PGO |
|---|---:|---:|---:|
| records 16 MiB, warm | 306 ms | 303 ms | 189 ms |
| numbers 16 MiB, warm | 375 ms | 343 ms | 214 ms |
| records 1 MiB, warm | 18.1 ms | 18.7 ms | 11.4 ms |
| numbers 1 MiB, warm | 21.8 ms | 20.6 ms | 12.4 ms |
| strings 16 MiB, warm | 41.2 ms | 43.5 ms | 32.1 ms |
| records 16 MiB, heap peak / kept | 159 / 78 MiB | 135 / 78 MiB | same |
| numbers 16 MiB, heap peak / kept | 221 / 87 MiB | 170 / 87 MiB | same |
| records 16 MiB, process peak | 531 MiB | 407 MiB | 407 MiB |
| numbers 16 MiB, process peak | 855 MiB | 366 MiB | 366 MiB |
| numbers 1 MiB, edit (first / median) | 0.37 / 0.35 ms | 0.46 / 0.38 ms | — |
| records 16 MiB, edit (first / median) | 9.3 / 4.3 ms | 9.8 / 6.1 ms | — |

Heap figures come from the bench's `heap` mode, which counts allocations
during one parse. Edit times vary by about ±50% between runs: rebuilding the
pre-change code gave 8–9 ms for the records 16 MiB median, so the edit rows
show no change beyond that noise.

Tried and not kept:

- **Building the reuse index on the first edit instead of at publish.** This
  was 3–4% faster to parse, but kept 60 MB more on numbers 16 MiB.
- **A parser object that keeps its scratch memory between parses.** Warm
  parses were 0–5% faster, which is within noise. Nitride's cold and warm
  times differ little because the system allocator already returns warm
  memory. STJ's large warm gain comes from its JIT recompiling hot code;
  native code starts out compiled, so there is no such gain to take.

## Assessment

- **PGO is the largest lever.** It makes structured input 37–40% faster and
  long strings 26% faster. STJ document parsing leads by 2.8–3.6× on
  structured input up to 1 MiB and by 4.6–5.7× at 16 MiB, down from
  4.8–6.5× and 8.4–10.1×. Without PGO, the code changes alone are worth
  about 8% on numbers and are within noise elsewhere.
- **The gap still grows with size.** From 1 MiB to 16 MiB the warm ratio goes
  from 4.6× to 9.1× on records (2.8× to 5.7× with PGO) and from 5.6× to 7.4×
  on numbers. Nitride's time per node stays nearly flat, while STJ gets
  faster per byte as its JIT tiers up on long runs. Closing that needs a
  cheaper tree or snapshot per node, which this work did not change.
- **Memory improved most at peak.** The process peak on numbers 16 MiB fell
  from 855 to 366 MiB (−57%) and on records from 531 to 407 MiB (−23%).
  What a snapshot keeps is unchanged, at 4.7–5.2× the source; STJ's whole
  process stays near 90 MiB. The spec's "process peak at least halved" is
  met for numbers, not for records.
- **Cold start still favours Nitride on small inputs** (0.2–1.5 ms, or
  0.14–0.97 ms with PGO, against STJ's 1.1–3.8 ms including JIT).
- **Not done:** making the fact list's records smaller (an estimated 10% off
  peak) and narrower per-node syntax storage. These are the next places to
  look for memory.

## Conformance

`corpus.py` writes 53 hand-written RFC 8259 cases (19 valid, 34 invalid);
`check` mode reports a verdict per file. Nitride and STJ agree with the RFC on
49. The differences:

| Case | RFC | Nitride | STJ default |
|---|---|---|---|
| `//` and `/* */` comments | reject | accepts | rejects (accepts with `CommentHandling.Skip`) |
| U+00A0 between tokens | reject | accepts | rejects |
| 50,000 nested arrays | accept | `limit` error | accepts with `MaxDepth = 100000` |

The first two come from Nitride's fixed trivia (Unicode whitespace and C-style
comments between tokens), which a grammar cannot currently override, so the
Nitride grammar is a strict superset of JSON. Default limits accept 10,000
levels of nesting and reject 20,000; STJ's default `MaxDepth` is 64.

Verdicts are unchanged by the throughput work, with and without PGO.

The cases are a smoke test, not JSONTestSuite. Nitride's leniency about
whitespace and comments makes it unsuitable as a strict JSON validator
without a change to trivia handling.

## Reproduce

```sh
cd scripts/json-bench
cargo build --release
cargo build --release --features heap --target-dir target/heap
python3 -I pgo.py
dotnet build -c Release dotnet
python3 -I corpus.py target/json-bench
./target/release/nitride-json-bench check target/json-bench/cases/*.json
./dotnet/bin/Release/net10.0/JsonBench check target/json-bench/cases/*.json
python3 -I bench.py --extras > ../../docs/json-comparison.json
```

One synthetic corpus on one host; real payloads and other machines will differ.
