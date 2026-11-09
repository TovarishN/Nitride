# Active cancellation polling cost

User approved continuing reduction of active-scope cost. Update PR #50 in the
existing isolated worktree; no merge, release, new dependencies or API changes.
Base: 4c13a29. Retain its release examples before changing source.

Hypothesis: inlining context/predicate traversal and diagnostic construction
into batched poll sites expands hot loops. Mark the shared Context check cold
and out of line, preserving all logical decisions and polling locations. This
is a measured compiler-layout refactor, not a cancellation feature redesign.

- [x] Retain exact baseline and validate core preservation regressions.
- [x] Build candidate and measure active-scope five-pair comparison first.
- [x] Retain only a useful measured improvement; preserve discarded evidence.
- [x] If retained, run full/native/quality gates and measure unscoped cost,
  direct pre-cancellation residual overhead and gated latency without competing
  builds/tests. Review independently and fix material findings.
- [x] Record results, commit, push and update PR #50; keep worktree clean.

No universal interruption deadline or GUI latency claim. Primitive/bulk/setup/
semantic/host boundaries remain unchanged. Use source/executable hashes rather
than branch labels as evidence. Do not multiply ratios across rounds to infer
residual overhead.

Initial active-scope five-pair result: cached -2.5% (all five pairs improve),
fresh -5.9%, projection -3.2%. Keep the two compiler hints provisionally and
run the final gates and direct residual/unscoped checks. Core101 passed before
timing; poll locations and conditions remain byte-for-byte unchanged.

Final outcome: retain hints for active cached -2.5% / fresh -5.9% (five of five
pairs improve both). Unscoped fresh -4.2%, cached +5.2% (four of five worsen),
kept explicit as a tradeoff. Direct e7eb003 residual: cached +5.1%, fresh +0.3%.
All18 latency trials pass, 0.07–0.64 ms. Workspace767/3ignored, native LSP228,
native CLI6, strict Clippy, format and patch checks pass. Independent review
clean, no deferred findings. All reports match current source/executable hashes.
Results: docs/benchmark-2026-11-09-active-polling.md.
