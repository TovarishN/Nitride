# Memoizing every Pratt point, November 12

A follow-up to the [decision census](decision-census-2026-11-12.md), which found
most backtracking waste in C# declaration decisions whose alternatives share
long prefixes. Measured on one Apple M4 Pro (macOS, rustc 1.99.1), portable
release builds of the [compiler-audit harness](../../../scripts/compiler-audit/README.md).

## Change

Codegen memoized a Pratt point only when its candidates could begin alike. The
C# declaration prefixes are made of points that fail that test: `Modifier` and
`TypeReference` have candidates that begin differently, so they were not
memoized. Every type declaration alternative (`class`, `struct`, `interface`,
`enum`, `delegate`) re-parsed `Attributes* Modifier*`, and every member body
alternative re-parsed its type reference. Now every Pratt point is memoized, and
the overlap analysis that chose points is removed. Runtime-loaded languages read
the same generated tables, so the editor's path changes too.

A variant that memoized only `Modifier` and `TypeReference` performed the same
within noise and was dropped: it named two C# rules in codegen.

## Results

Medians of nine rounds in alternating build order.

| Input | Mode | Before ms | After ms | Change |
|---|---|---:|---:|---:|
| 256 KiB valid | strict | 44.46 | 44.15 | −0.7% |
| 4 MiB valid | strict | 682.37 | 669.21 | −1.9% |
| 16 MiB valid | strict | 2,710.40 | 2,652.13 | −2.1% |
| 256 KiB one error | recovering | 57.89 | 57.40 | −0.9% |
| 4 MiB one error | recovering | 863.65 | 857.24 | −0.7% |
| 4 MiB junk | recovering | 861.67 | 858.87 | −0.3% |
| 256 KiB dense errors | recovering | 353.46 | 342.22 | −3.2% |
| 256 KiB truncated | recovering | 394.67 | 324.65 | **−17.7%** |

Two earlier rounds on the same builds put valid input at −1.7% to −3.3%, so
valid parses gain about 1–3%. Truncated input, the shape of a file being typed,
gains consistently: −16.9% and −17.7% in two separate runs. Its logical work
falls 23%.

## Parity

The generated inputs and the 29 Mono corpus files, strict and recovering:
69 runs. Tree fingerprints (nodes, spans, labeled edges, diagnostics and
recovery records) and diagnostic counts are identical in every run. Logical work
falls 2.3% on valid generated input and 23% on truncated input; one-error and
junk inputs are within 0.1%. All four frontend seeds were already memoized
everywhere and reproduce unchanged.

## Wasted work

The census now also measures the logical work spent in failed attempts, with
a failure nested in another failed attempt counted once:

| Source | Before | After |
|---|---:|---:|
| Generated 4 MiB file | 17.1% of work | 15.5% |
| 29 Mono files | 30.1% | 28.3% |

Memoization removes little of the waste. The three costliest decisions after the
change are `MemberDeclaration` (5.1% of all work), `TypeDeclaration` (4.8%) and
`ClassMember`'s choice of a type or member declaration (3.9%). A memo hit still
pays for a rule entry, a trivia replay and its work ticks. And `TypeDeclaration`
still runs all five candidates for every declaration, because a Pratt point ranks
every matching candidate. Removing that waste needs a decision made after the
shared prefix: recognize the attributes and modifiers once, then choose by the
keyword that follows.

## Limits

- One host and one synthetic corpus shape plus the Mono files; cold parses only.
- Memo storage stays within the existing per-parse budget; peak memory and edits
  were not remeasured.
- Other bundled languages are covered by the workspace tests, not timed.

## Reproduce

```sh
cargo build --release --manifest-path scripts/compiler-audit/Cargo.toml --target-dir target/a
# check out the other revision and build into target/b, then
python3 -I scripts/compare_parser_builds.py target/a/release/nitride-compiler-audit \
    target/b/release/nitride-compiler-audit --rounds 9 --work-may-differ
cargo build --release -p nitride-cli --bin nitride-profile --features census
python3 -I scripts/parser_census.py --profile target/release/nitride-profile
```

[Timing samples, per-run parity and both census summaries](pratt-memo-2026-11-12.json).
