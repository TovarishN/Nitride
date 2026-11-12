# Recognition facts for the outer list levels, November 12

`claude/fact-levels-pratt-memo` against `main` (a675281, which includes the
[per-rule cost changes](kept-path-2026-11-12.md)). One Apple M4 Pro (macOS,
rustc 1.99.1), portable release builds: full LTO, one codegen unit, no PGO and
no SIMD features.

## Changes

- **Facts for the outer four list levels.** Recognition facts are recorded at
  repeated-element checkpoints: they let an edit reuse unchanged elements, seed
  recovery from a failed strict attempt and memoize backtracking. They are now
  kept for the outermost four levels of nested repeated elements. Deeper elements
  skip the checkpoint bookkeeping, except where a recovery probe waits for a
  failure fact; an edit there reparses its nearest recorded ancestor.
- **Memoized Pratt results record their own depth and reach.** A memoized
  precedence-expression entry stored the parser-wide running maxima of depth,
  observation and failure reach rather than its own, unlike lexical memo entries.
  Replayed deeper than it was recorded, it could inherit an earlier sibling's
  depth and fail the frame limit on valid input. List-item checkpoints reset those
  maxima, which hid the defect; with one recorded level, a valid 4 MiB file was
  rejected. A regression test checks that memoizing a point changes no
  depth-limit outcome.

## Cold parses

Compiler-audit harness, medians of nine rounds in alternating build order.

| Input | Mode | `main` ms | Branch ms | Change |
|---|---|---:|---:|---:|
| 256 KiB valid | strict | 45.17 | 44.06 | −2.5% |
| 4 MiB valid | strict | 691.99 | 679.28 | −1.8% |
| 16 MiB valid | strict | 2,856.13 | 2,721.23 | −4.7% |
| 256 KiB one error | recovering | 57.98 | 56.90 | −1.9% |
| 4 MiB one error | recovering | 890.15 | 862.82 | −3.1% |
| 4 MiB junk | recovering | 882.88 | 859.75 | −2.6% |
| 256 KiB dense errors | recovering | 348.04 | 348.74 | +0.2% |

## Storage and edits

`nitride-profile` builds of both revisions. Storage is a cold strict snapshot of
the 4 MiB valid input; edits alternate one digit near the middle of the file 60
times, with the median edit phase shown.

| Measure | `main` | Branch | Change |
|---|---:|---:|---:|
| Fact index | 9.2 MiB | 2.6 MiB | −71.2% |
| Retained snapshot | 68.2 MiB | 60.8 MiB | −10.8% |
| Parse peak | 136.3 MiB | 117.8 MiB | −13.6% |
| Edit, 256 KiB valid | 0.19 ms | 0.23 ms | +17.7% |
| Edit, 4 MiB valid | 2.59 ms | 1.50 ms | −41.9% |
| Edit, 4 MiB one error (recovering) | 8.47 ms | 7.69 ms | −9.2% |

## Choosing four levels

A sweep with the Pratt memo fix compared one to four recorded levels with no limit.
Two to four levels keep every case healthy; one level does not:

| Levels | Edit, 256 KiB valid | Edit, 4 MiB valid | 4 MiB one-error parse | Parse peak |
|---|---:|---:|---:|---:|
| No limit | 0.19 ms | 2.51 ms | 895 ms | 136 MiB |
| 1 | 240 ms (whole file) | over 2 minutes | 1,202 ms | 107 MiB |
| 2 | 0.38 ms | 1.54 ms | 850 ms | 108 MiB |
| 3 | 0.38 ms | 1.64 ms | 853 ms | 110 MiB |
| 4 | 0.23 ms | 1.46 ms | 869 ms | 118 MiB |

With one level an edit reparses the whole file, because the outermost elements
are the only reusable ones. Four levels keep small-file edits close to the
unlimited setting while retaining most of the memory saving.

## Parity

The generated inputs (64 KiB to 4 MiB; valid, one error, junk, dense errors,
truncated) and the 19 Mono corpus files, in strict and recovering mode: 69 runs.
Tree fingerprints (nodes, spans, labeled edges, diagnostics and recovery records)
and diagnostic counts are identical in every run. With every fact kept, the Pratt
memo fix alone also leaves logical work unchanged.

Logical work changes where fewer facts are recorded: 0.7% more on strict valid
generated inputs, from fewer memo hits. Generated parses of four valid deeply
nested inputs (40-deep blocks, 24-deep nested classes, 30-deep array initializers
and lambdas) keep identical trees at two to six levels, and the nested classes and
lambdas also at one, with at most 5% more work and time, at 44–75 units per byte
against a budget of 1,024.

## Limits

- One host, one synthetic corpus shape and synthetic deep-nesting inputs. What a
  list level corresponds to depends on the grammar; other languages were covered
  only by the workspace tests.
- PGO builds and the .NET comparisons were not remeasured.

## Reproduce

```sh
cargo build --release --manifest-path scripts/compiler-audit/Cargo.toml --target-dir target/a
cargo build --release -p nitride-cli --bin nitride-profile --target-dir target/a
# check out the other revision and build the same into target/b, then
python3 -I scripts/compare_parser_builds.py target/a/release/nitride-compiler-audit \
    target/b/release/nitride-compiler-audit --rounds 9 --work-may-differ
python3 -I scripts/compare_parser_edits.py target/a/release/nitride-profile \
    target/b/release/nitride-profile
```

[Cold samples and per-run parity](fact-levels-2026-11-12.json), [storage and edit samples](fact-levels-2026-11-12-edits.json).
