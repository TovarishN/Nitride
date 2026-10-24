# Parser scratch reuse

The parser reuses child-edge and Pratt-winner snapshot vectors within one parse.
Recursive frames own their buffers exclusively; rollback and tree indices are
unchanged. Scratch storage is dropped before the returned tree copies its source.
The core uses safe Rust, has no third-party runtime dependencies and keeps the
standard allocator. There are no global caches or changes to grammar admission.

## Measured effect

macOS arm64, Rust 1.99.1, release profile; three runs per workload. Counts,
retained bytes and peak bytes agree across all three runs. Baseline:
[stage2-phase-memory.json](stage2-phase-memory.json). Current raw results:
[stage3-phase-memory.json](stage3-phase-memory.json).

| Workload | Parse allocations before → after | Parse peak before → after | Retained syntax bytes |
|---|---:|---:|---:|
| 1,000 portable statements | 29,036 → 49 | 745,832 → 746,688 B | 733,785 |
| 1,000 composed statements | 34,036 → 49 | 745,832 → 746,688 B | 733,785 |
| 1,000 XML elements | 11,039 → 46 | 768,708 → 768,708 B | 768,708 |
| 5,000 portable statements | 145,043 → 56 | 3,080,552 → 3,081,408 B | 2,952,473 |
| 5,000 XML elements | 55,048 → 55 | 6,014,980 → 6,014,980 B | 6,014,980 |

Allocation count falls by more than 99 percent on these large fixtures. Returned
tree capacity and HIR/execution allocation records remain unchanged. Portable
parse peak increases by 856 bytes from scratch-pool bookkeeping. Tiny XML (10
elements) adds 864 bytes peak; larger measured XML peaks are unchanged. Fewer
allocations do not imply lower retained storage or prove faster execution.
The release CLI remains 452,512 bytes (441.9 KiB).

Metrics count requested bytes through the profiling executable's System wrapper,
not RSS, allocator overhead, stack memory or executable mappings. Caller input is
in the baseline, the tree's source copy is included, and JSON is formatted after
all snapshots. Phase peaks use independent baselines. Instrumented timings are
not uninstrumented throughput measurements. Parser recursion and large nested
winner snapshots remain possible costs; this pass does not promise a general
memory ceiling. Refer to [stage-two evidence](stage2-evidence.md) for provenance
and compatibility boundaries.

## Verification

- Two resource regression tests first failed against the previous parser, then
  passed after scratch reuse. They exercise actual profiler subprocesses and
  verify scratch is absent from returned syntax storage.
- All 42 Rust tests pass, including rollback, lookahead, ambiguity, precedence,
  Unicode spans, typing, preflight and reference parity.
- All 11 exporter tests pass. 51 frozen observations and 128 generated cases
  match real Nitrogen; the generated comparisons use the optimized release CLI.
- Formatting, strict clippy and locked release builds pass.
- A fresh reviewer inspected recursive ownership, winner snapshots, rollback,
  errors and tests, and reported no actionable correctness findings.

```sh
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
python3 scripts/phase_memory.py
python3 scripts/check_reference.py
python3 scripts/check_stage2_reference.py
python3 scripts/differential.py
python3 scripts/stage2_differential.py
```

macOS is verified locally. Linux/Windows CI remains configured but unrun.
All changes remain local and uncommitted; no Nitrogen product was replaced.
