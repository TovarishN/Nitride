# Stage-two composition and isolated engine memory

Historical baseline before scratch reuse; see [stage-three evidence](stage3-evidence.md)
for current parser allocation measurements.

Validated locally on macOS arm64, 2026-10-24, with Rust 1.99.1. The release CLI
is 452,512 bytes (441.9 KiB). Cargo builds and native execution use generated
language data and require no .NET runtime. Development export and live reference
comparison still use the sibling Nitrogen checkout and .NET.

## Compatibility evidence

- Ordered multi-module export resolves local, qualified, imported and implicit
  extension-target references. Portable.Power adds typed, right-associative
  exponentiation with real Nitrogen binding/type/HIR comparisons.
- Base/Uses and XML grammars are copied unchanged from Nitrogen; original paths
  and SHA-256 hashes are in [grammar-provenance.json](grammar-provenance.json).
- XML and Base/Uses are explicitly syntax-only. Tree comparisons retain named
  rules, UTF-8 spans and parent relationships, omitting pure aliases on both sides.
  XML C# closing-tag checks and pattern warnings are listed as omitted semantics.
- 40 Rust tests and 11 development exporter tests pass. All 51 frozen observations
  match live Nitrogen; 128 generated portable/composed/XML programs also match.
- Formatter, strict clippy and release builds pass. A fresh copy containing only
  Git-visible source passes all Rust tests offline, including profiler tests.
- Independent review findings were reproduced and repaired: profiler source was
  hidden by a bin ignore rule; precedence leaked into enclosed expressions.
  Three precedence regressions were observed failing before the repair.

Equal-ranked ambiguous trees, conflicting infix spellings, multiple symbol
namespaces and general C# semantic helpers are not admitted. This is an expanding
compatibility profile, not full Nitrogen equivalence.

## Engine allocation measurements

The separate `nitride-profile` executable wraps the standard System allocator.
The ordinary executable has no allocation instrumentation. Each workload runs
three times; raw phase records and workload sizes are in
[stage2-phase-memory.json](stage2-phase-memory.json).

| Workload | Source bytes | Parse retained | Parse peak | HIR retained | HIR peak | Execution retained / peak |
|---|---:|---:|---:|---:|---:|---:|
| 1,000 portable statements | 12,889 | 733,785 B | 745,832 B | 356,352 B | 473,224 B | 40,960 / 80,992 B |
| 1,000 composed statements | 12,889 | 733,785 B | 745,832 B | 356,352 B | 473,224 B | 40,960 / 80,992 B |
| 1,000 XML elements | 47,812 | 768,708 B | 768,708 B | — | — | — |
| 5,000 XML elements | 247,812 | 6,014,980 B | 6,014,980 B | — | — | — |

These are requested allocation bytes, not RSS or total process memory. Each
phase's peak is incremental above its own baseline; independent peaks must not
be summed as a simultaneous process peak. Caller source and pre-created host
bindings are in the baseline. Parse storage includes its owned source copy.
Static tables, executable mappings, allocator overhead and OS-internal realloc
copies are excluded. JSON formatting happens after all phase snapshots.
Instrumentation affects timing, so recorded times are not uninstrumented
throughput benchmarks. There is no equivalent .NET phase-allocation comparison.

At 1,000 statements parsing makes 29,036 allocations (34,036 with Power), so
temporary parser buffers are a concrete future optimization target. Vector
capacity growth also affects retained memory. These measurements establish a
baseline; they do not establish a memory ceiling for arbitrary grammars.

```sh
cargo build --release --workspace
python3 scripts/phase_memory.py
python3 scripts/check_reference.py
python3 scripts/check_stage2_reference.py
python3 scripts/differential.py
python3 scripts/stage2_differential.py
```

macOS arm64 was executed. The GitHub Actions matrix covers macOS/Linux/Windows,
but no remote workflow has run and Linux/Windows remain unverified. The existing
Nitrogen workspace checker accepts the registered samples; live VS Code/Rider
interactions remain unverified. Work is local and uncommitted, with no publication
or existing Nitrogen product replacement.
