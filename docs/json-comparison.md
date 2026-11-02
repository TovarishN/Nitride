# JSON parsing: Nitride vs System.Text.Json, 2026-11-02

A JSON grammar ([`languages/json/Json.ngr`](../languages/json/Json.ngr)) was
written for Nitride and its generated parser compared with .NET's
`System.Text.Json` on an **Apple M4 Pro**. Raw samples are in
[`json-comparison.json`](json-comparison.json); the harness is
[`scripts/json-bench`](../scripts/json-bench).

## What is compared

| Engine | Work done |
|---|---|
| Nitride | Generated parser (`parse_snapshot_generated`, strict mode) building a shared syntax snapshot: one node per object, array, member, string and number with spans and labeled edges. No string unescaping or number conversion. |
| STJ `JsonDocument.Parse` | Validates and builds JsonDocument's token index (the closest analogue of a syntax tree). No string unescaping or number conversion until accessed. |
| STJ `Utf8JsonReader` | Validates and tokenizes only; retains nothing. Lower bound. |

Nitride: LTO, one codegen unit, no PGO, rustc 1.99.1. STJ: `net10.0`, default
tiered JIT, workstation GC. Cold is the first parse in a fresh process (STJ
includes JIT); warm is the median of 10 more parses in that process. Each cell
is the median of 5 fresh processes. Process start and file reading are
excluded. Inputs are synthetic (`corpus.py`): pretty-printed API-style
records, compact number arrays, and long strings with escapes.

## Results

Milliseconds, cold / warm (lower is better):

| Input | Nitride | STJ document | STJ reader | Nitride ÷ STJ document, warm |
|---|---:|---:|---:|---:|
| records 64 KiB | 1.52 / 1.33 | 4.18 / 0.24 | 3.39 / 0.17 | 5.4× |
| records 1 MiB | 20.6 / 19.5 | 7.69 / 4.06 | 6.02 / 2.57 | 4.8× |
| records 16 MiB | 343 / 330 | 72.2 / 33.1 | 44.3 / 36.1 | 10.0× |
| numbers 64 KiB | 1.59 / 1.47 | 1.22 / 0.23 | 0.60 / 0.18 | 6.4× |
| numbers 1 MiB | 23.6 / 23.1 | 4.60 / 3.64 | 3.66 / 2.97 | 6.4× |
| numbers 16 MiB | 427 / 405 | 60.9 / 46.5 | 44.2 / 41.2 | 8.7× |
| strings 64 KiB | 0.21 / 0.18 | 3.59 / 0.02 | 3.25 / 0.02 | 11× |
| strings 1 MiB | 2.75 / 2.77 | 4.18 / 0.30 | 3.66 / 0.32 | 9.2× |
| strings 16 MiB | 43.9 / 43.9 | 9.61 / 5.16 | 8.57 / 4.92 | 8.5× |

Peak resident memory of the whole process, MiB:

| Input | Nitride | STJ document | STJ reader |
|---|---:|---:|---:|
| records 16 MiB | 532 | 93 | 64 |
| numbers 16 MiB | 856 | 90 | 64 |
| strings 16 MiB | 60 | 64 | 63 |
| any 64 KiB | 3–6 | 47 | 47 |

## Assessment

- **Throughput: STJ is 5–11× faster warm.** Nitride sustains about 45–55 MB/s
  on structure-heavy JSON and about 380 MB/s on long strings; STJ document
  reaches 260–510 MB/s and 3.2–3.9 GB/s respectively. Nitride's cost scales
  with node count (about 200–350 ns per node), not bytes: long strings are
  cheap, dense numbers are the worst case.
- **Cold start: Nitride wins on small inputs.** With no JIT, Nitride finishes
  a 64 KiB records or strings file before STJ's first parse completes
  (0.2–1.5 ms vs 3.6–4.2 ms). For a one-shot CLI parsing small configs, that
  is the number that matters. From 1 MiB up, STJ is faster even cold
  (2.7–7× for structured input).
- **Nitride does not warm up.** Its cold and warm times are within ~10%,
  while STJ's tiered JIT gains 2–15×.
- **Memory: a large gap on structured input.** Nitride's peak is 33–53× the
  source size for records/numbers at 16 MiB, versus about 5× for the whole
  STJ process. Part of Nitride's peak is the harness's `materialize()` call
  (a flat copy, done outside the timed region to count nodes), so the
  snapshot alone is smaller; the gap remains large. Small inputs favour
  Nitride because .NET's runtime baseline is ~47 MiB.
- **Grammar shape matters little.** Collapsing the wrapper rules for strings,
  numbers and literals into `Value` improved warm time by about 10%; the
  results above use that flat grammar.
- **Not measured:** PGO (32–35% on C# in the
  [Oct 30 assessment](compiler-assessment-2026-10-30.md)), incremental
  re-parsing after edits (Nitride's strength; STJ has none), error recovery,
  and decoding values into objects (STJ's main use, via `JsonSerializer`).

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

The cases are a smoke test, not JSONTestSuite. Nitride's leniency about
whitespace and comments makes it unsuitable as a strict JSON validator
without a change to trivia handling.

## Reproduce

```sh
cd scripts/json-bench
cargo build --release
dotnet build -c Release dotnet
python3 -I corpus.py target/json-bench
./target/release/nitride-json-bench check target/json-bench/cases/*.json
./dotnet/bin/Release/net10.0/JsonBench check target/json-bench/cases/*.json
python3 -I bench.py > ../../docs/json-comparison.json
```

One synthetic corpus on one host; real payloads and other machines will differ.
