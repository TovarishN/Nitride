# Fast strict path decision census, November 12

Milestone 0 of the [fast strict path design](../../superpowers/specs/2026-11-12-fast-strict-path-design.md):
how many grammar decisions could a table-driven engine settle from lookahead
alone? Measured on one Apple M4 Pro with `nitride-profile --census`
(the `census` feature) and [`scripts/parser_census.py`](../../../scripts/parser_census.py).

**Verdict: no-go under the planned thresholds.** Choices and Pratt phases are
mostly decided by today's two-token lookahead (G1 and G2 pass), but failed
attempts that consume input are about four times the planned limit (G3 fails),
and syntax predicates are frequent. Two declaration-level decisions cause most
of the waste. Milestone 1 does not start; the findings go back to the design.

## What was counted

Every strict parse is fully interpreted, the language server's path. At each
ordered choice and each Pratt prefix phase or infix round, the census counts how
many alternatives or candidates the current engine actually runs after its
existing first-character routes and two-token leads. The fast engine's tables
would start from the same lookahead. An entry is **decided** when at most one
alternative ran. A failed attempt **consumed input** when some match inside it
(a literal, character class, whole lexical token, or replayed rule or memo
result) passed the attempt's start; trivia, lookahead scans and syntax
predicates do not count. Choices and repetitions inside lexical rules are
token-level and not counted. A census build with counting active produces
identical trees, diagnostics and logical work on the 69-run comparison corpus.

Thresholds, fixed by the [plan](../../superpowers/plans/2026-11-12-fast-strict-path-census.md)
before measuring:

- **G1:** at least 90% of choice decisions decided.
- **G2:** at least 90% of Pratt phases decided.
- **G3:** at most 20 consumed failures per 1,000 source bytes, across choices,
  Pratt candidates and repetition exits.
- **Predicate support** is needed if syntax predicates run more than 5 times per
  1,000 bytes.

## CSharp4

The 4 MiB generated valid input and the 29 Mono corpus files, 4.23 MB in all.

| Measure | Value | Threshold | |
|---|---:|---:|---|
| Choice decisions | 1,035,277 | | |
| Choices decided | 90.1% | ≥ 90% | G1 passes |
| Pratt phases | 2,958,339 | | |
| Pratt phases decided | 97.3% | ≥ 90% | G2 passes |
| Consumed failures per KB | 78.3 | ≤ 20 | **G3 fails** |
| of which choices / Pratt / repetition exits | 35.0 / 32.5 / 10.8 | | |
| Syntax predicates per KB | 37.8 | > 5 needs support | **support needed** |
| Context-instance entries per KB | 0.0 | | |

### Where the waste is

The costliest decisions by consumed failures per 1,000 bytes:

| Decision | Kind | Per KB | Entries | Attempts | Decided |
|---|---|---:|---:|---:|---:|
| `Declarations.TypeDeclaration` | Pratt prefix (5 candidates) | 24.3 | 22,833 | 114,101 | 16 |
| `Declarations.MemberDeclaration` | choice (11) | 21.5 | 45,466 | 136,360 | 11,364 |
| `Statements.StandardStatement` | choice (4) | 5.4 | 102,337 | 125,080 | 79,594 |
| `Expressions.Expression` | Pratt prefix (33) | 2.7 | 318,501 | 352,774 | 284,323 |
| `Declarations.ClassMember` | Pratt prefix (2) | 2.7 | 56,857 | 56,931 | 34,019 |
| `Declarations.StandardClassMember` | choice (2) | 2.7 | 45,475 | 56,923 | 34,027 |
| `NamesTypes.TypeBase` | Pratt prefix (6) | 2.7 | 147,856 | 170,631 | 136,468 |
| `Attributes.Parameters` | choice (2) | 2.7 | 22,753 | 45,506 | 0 |
| `Queries.Binding` | choice (2) | 2.7 | 11,369 | 22,730 | 8 |

`TypeDeclaration` and `MemberDeclaration` alone are 46 of the 78 consumed
failures per KB. Their alternatives share long prefixes: attributes, modifiers,
then a type and a name. Two tokens cannot see past the shared prefix, so the
engine parses it once per alternative until one fits. `TypeDeclaration` runs all
five candidates on almost every type, because a Pratt point ranks every matching
candidate. These are low-frequency decisions (one per type or member) with
expensive attempts, which is why they dominate the waste while barely moving
the decided shares.

## Other bundled languages

One sample file each, reported only; samples are small (31–854 bytes).

| Language | Bytes | Choices | Decided | Pratt phases | Decided | Consumed per KB | Predicates per KB |
|---|---:|---:|---:|---:|---:|---:|---:|
| portable | 58 | 4 | 1.000 | 12 | 1.000 | 0.0 | 0.0 |
| power | 31 | 2 | 1.000 | 10 | 1.000 | 0.0 | 0.0 |
| composition | 31 | 2 | 1.000 | 4 | 1.000 | 0.0 | 0.0 |
| xml | 854 | 58 | 0.741 | 0 | — | 17.6 | 0.0 |
| namespaces | 85 | 6 | 1.000 | 12 | 1.000 | 0.0 | 0.0 |
| inference | 118 | 6 | 0.500 | 24 | 1.000 | 25.4 | 0.0 |
| calls | 74 | 5 | 1.000 | 14 | 0.500 | 189.2 | 0.0 |
| functions | 176 | 21 | 1.000 | 0 | — | 0.0 | 0.0 |
| dates | 141 | 7 | 1.000 | 36 | 0.944 | 0.0 | 0.0 |
| rewrites | 149 | 7 | 1.000 | 40 | 0.950 | 0.0 | 0.0 |
| context | 60 | 11 | 1.000 | 30 | 1.000 | 0.0 | 0.0 |
| grammar | 92 | 13 | 1.000 | 0 | — | 0.0 | 43.5 |

JSON has no profiler language and is not covered.

## What this means for the design

- **Most decisions are table-friendly.** Nine in ten choices and almost every
  Pratt phase are already decided by two tokens of lookahead.
- **The waste is concentrated, not spread.** A handful of declaration-level
  decisions with shared prefixes cause most consumed failures. A fast engine
  that only backtracks would repeat today's waste there.
- **Predicates are on the hot path** at 38 per KB on C#, so bailing out on them
  is not an option; milestone 1 would have to evaluate them.

Options for the design discussion, not decided here:

1. **Left-factor shared prefixes in the analysis:** recognize the common
   attributes-modifiers-type-name prefix once, then decide on what follows. This
   must still reproduce today's trees exactly.
2. **Memoize within those few decisions:** keep backtracking but cache the shared
   prefix rules, as the engine's facts already do.
3. **Raise G3 or measure cost instead of count:** a consumed failure here may be
   cheap or expensive; weighting by work spent would show whether the 78 per KB
   matter in time.

## Limits

- One host; one synthetic generated corpus plus 29 Mono files. The generated
  corpus repeats one class shape, which over-weights its particular declarations.
- The census measures today's routes and leads. A deeper analysis (three or
  more tokens, left factoring) could decide more; this census does not estimate
  that.
- "Consumed" is a count of failed attempts, not their cost.

## Reproduce

```sh
cargo build --release -p nitride-cli --bin nitride-profile --features census
python3 -I scripts/parser_census.py --profile target/release/nitride-profile \
    --report target/reports/decision-census.json
```

[Per-language summaries, the verdict and per-file raw census reports](decision-census-2026-11-12.json).
