# Parser structure and documentation cleanup

**Goal:** Make shared parsing code and current documentation easier to navigate without changing parser behavior or losing evidence.

**Scope:** The user chose structural cleanup first and delegated subsequent ordering. Work on `codex/parser-structure-and-docs`; preserve unrelated untracked files. No publication or evidence deletion.

- [x] Establish core unit-test baseline (101 passing).
- [x] Extract shared-parser tests, snapshot lifecycle, recognition, prediction, Pratt parsing, recovery probes and token caches into responsibility-based modules. Preserve public paths, algorithm bodies, attributes, budgets and storage layouts; use only private module visibility where needed.
- [x] Add `docs/README.md` grouping current guides and historical evidence; link it prominently from the root README and mark the initial plan historical. Preserve evidence paths because scripts and reports depend on them.
- [x] Verify moved bodies mechanically against HEAD, format, run workspace tests and clippy, and review the final diff.

Existing tests cover strict/generated parsing, incremental reuse, recovery ownership, cancellation, logical limits and allocation bounds. This is a code-movement refactor; no new behavior or mirror tests are needed. Run existing tests before and after. Follow up with fuzz/property testing after this cleanup is verified.

## Follow-up: incremental fuzzing

The existing suite already has seeded strict-edit checks; extend it with a reusable
coverage-guided harness rather than duplicating those tests. Four bundled profiles
(Portable, strict JSON, XML, CSharp4), strict/recovering modes and interpreted/generated
initial snapshots feed bounded UTF-8 edits. Compare observable syntax and recovery
metadata against full parsing, and verify retained readers. Work-limit outcomes can
differ with reuse and are excluded from equality checks. Core gains no dependencies.

- [x] Add a separate cargo-fuzz package and committed seed corpus, sharing its oracle with an ordinary CLI integration smoke test.
- [x] Build the target, run a bounded campaign and retain any discovered regression.
- [x] Document replay, instrumentation and tested limits; run final verification and request review.

Verification so far: 101 core unit tests pass before and after extraction; all 141
original function definitions are mechanically preserved modulo formatting, internal
visibility and relocated paths. Workspace tests pass; all-feature clippy passes after
narrowing cache visibility. A fresh read-only reviewer found no actionable issues in
the parser/documentation cleanup. Final checks will include the fuzz additions.

Final verification: workspace 816 passed / 0 failed / 3 ignored; optional SIMD core
141 passed; generated syntax/lexical suites 14 passed; all-feature workspace clippy
and standalone fuzz clippy clean; formatting and bootstrap fixed points pass.
Final ASan campaign: 4,738 inputs in 61 seconds, no failure. Reviewer identified a
failed-edit oracle using shared cold parsing; corrected to independent flat parsing
and re-reviewed with no remaining actionable findings. Work remains uncommitted.
