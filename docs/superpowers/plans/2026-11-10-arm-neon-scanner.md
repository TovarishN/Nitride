# ARM NEON ASCII scanner

Goal: accelerate generator-proven ASCII runs on aarch64 under the existing
opt-in simd-ascii-runs feature, keeping all parser-visible behavior and defaults.
Base: `5b5ae36`, stacked on improvement PR #55. Work in the current checkout on
codex/arm-neon-scanner; preserve unrelated untracked files. The user delegated
implementation choices and requested proceeding directly.

Design: reuse AsciiRun's low-nibble membership table. A 16-byte NEON table lookup
selects low/high nibble masks; non-ASCII high nibbles select zero. Reduce rejected
lanes to their first index, consume complete blocks only, and let the shared
scalar tail handle short slices. Check NEON availability before calling the
private target-feature function. AVX2 dispatch and default features stay intact.
No dependencies, generated-table changes, API additions or native CPU flags.

- [x] Establish the core/SIMD baseline. Add direct NEON differential tests before
  implementation: all byte values, singleton and mixed masks, rejection at every
  lane, unaligned slices, exact vector lengths/tails and mixed Unicode bytes.
  Test complete vector-prefix consumption independently from scalar-tail scan.
- [x] Implement AsciiRun::neon and aarch64 dispatch in lexical/runs.rs; use only
  bounded slice loads and explain each unsafe block. Run direct classifier and
  existing lexical work/depth/observation/repetition tests, then generated parsers.
- [x] Measure scalar-batching versus NEON with matched portable release builds,
  using the existing compiler-audit parser harness, rotating paired runs and
  checking tree/diagnostic fingerprints. Include long-string and mixed valid/
  damaged C# cases. Save new evidence under target/reports/ and retain a reviewed
  report without changing historical data. Confirm NEON instructions in assembly.
- [x] Run default workspace tests, feature-enabled core/generated suites, clippy,
  formatting and bootstrap checks. Update feature docs/tracker/changelog with
  actual measured limits; obtain independent review and commit separately.

Review focus: non-ASCII bytes must stop before UTF-8 decoding; lane ordering must
hold at both ends of each vector; no out-of-bounds loads on tails/unaligned slices;
work/depth/observation/cancellation behavior must match the existing path; absent
SIMD support must retain scalar behavior. The direct tests and existing lexical/
generated suites cover these boundaries. Other architectures need cross-target
compilation or hosted CI; local ARM measurements do not establish x86 performance.

## Recorded verification

- Baseline core with SIMD: 148 passed. New direct NEON tests failed to compile
  because the method was absent, then both passed after implementation.
- Feature-enabled core: 150 passed; generated lexical/parser suites: 14 passed.
  Nightly ASan classifier suite: three passed. Default workspace: 847 passed,
  zero failed, three ignored. All-feature/all-target clippy, formatting and four
  bootstrap fixed points pass; wasm32 scalar fallback compiles.
- Two matched benchmark runs: 336 timed processes plus 48 warmups; zero full
  fingerprint/diagnostic/node/work mismatches. String-1m improves 26–28% against
  scalar batching; mixed-code gains are inconsistent, so defaults stay off.
- Native macOS CI now runs the optional scanner suites after default CLI checks.
  Hosted NEON acceptance remains pending. The harness lockfile needed an offline
  refresh of path crates from 0.3.0 to 0.4.0; registry versions did not change.
- Read-only implementation review found no unsafe/correctness issues. Final
  evidence and documentation review precedes the commit.

Final independent review found no blocking issues. Corrected its one minor
wording finding: warmups are checked but their records are not retained; all
336 timed samples are retained. Verified raw medians and source/binary/input
hashes, 527 local documentation links, CI YAML and existing archive manifests.
PR #55 now passes all hosted checks; that result does not cover this stacked
NEON change. The NEON commit/PR are separate from the earlier improvements.
