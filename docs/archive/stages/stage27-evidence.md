# Stage27 parser hot-path checkpoint

Three changes, each with logical work unchanged on every measured input:

- The shared parser's per-step stack guard (`tick`, and the generated `guard`) reads the stack position inline. Before, it called the never-inlined `stack_position`. The probe is still a local's address and differs by at most one small frame. Legacy parsing and lexical scanners keep the original function.
- Parser-private maps (memo generations, pending facts, label ids, choice plans, prior paths, probe cache) hash with a seeded multiply-fold hasher instead of SipHash. Their keys are fixed-width positions, ids and pointers. Each map draws a random seed, so bucket placement stays unpredictable. No dependency is added.
- The cache-budget check that runs after every backtrack is an inlined comparison. Eviction moved to a cold, out-of-line path.

An early exit from `find_prior` on cold parses was also tried. It showed no measurable change and was dropped.

## Timings

Seeded-recovery build (stage26, `f4b0473`) against this change, `--native-syntax`, uninstrumented Release. Each value is the median of six fresh processes, in alternating order, same host.

| Input | Mode | Before s | After s | Speedup |
|---|---|---:|---:|---:|
| 256 KiB valid | strict | 0.113 | 0.096 | 1.18x |
| 4 MiB valid | strict | 1.796 | 1.518 | 1.18x |
| 16 MiB valid | strict | 7.341 | 6.291 | 1.17x |
| 256 KiB one error | recovering | 0.538 | 0.501 | 1.08x |
| 4 MiB one error | recovering | 8.480 | 7.911 | 1.07x |
| 4 MiB truncated | recovering | 1.781 | 1.575 | 1.13x |

Profiling had suggested 30-35% from these costs. The measured gain is about half that. The remaining large costs are structural: interpreted Pratt and rule dispatch, and arena reference counting.

## Validation

436 workspace test executions, including three new hasher tests, plus strict all-target/all-feature clippy and fmt. One run hit an LSP test-harness temp-directory flake (`create_dir_all`); the rerun and the full suite passed. All 4,334 standard and 1,286 adversarial traces pass with `--native-syntax`, including the cold-snapshot check, with zero mismatches. Profiler SHA-256: `e020d747ff05b780516d6f6d58fe84ca6076d0082e5f571c05f5c4fde561b927`.

[Paired timings](stage27-paired.json), [standard traces](stage27-differential.json), [adversarial traces](stage27-adversarial.json).
