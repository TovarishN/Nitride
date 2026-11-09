# Reduce cooperative polling overhead

Scope approved by “go on”: reduce the cost recorded in PR #50 while preserving
its cancellation semantics, existing logical budgets and snapshot outputs. Work
in the existing isolated branch and update PR #50; no merge or release.

Hypothesis: the poll hot path first branches on an optional context, then checks
a mutable threshold. Encode disabled polling as an unreachable threshold and
move the context consultation entirely behind the work threshold. This removes
the per-step context load/branch while preserving immediate scoped polling,
large work charges and speculative rewind credit. Keep traversal structure and
poll locations unchanged initially; specialize traversal only if evidence warrants.

- [x] Retain ec674f6 native executables and the original benchmark artifacts.
- [x] Refactor the poller using the existing deterministic batching, jump, rewind,
  scope, spill and transactional cancellation regressions as preservation gates.
- [x] Run correctness/lint checks and rebuild; run paired default/scoped overhead
  against ec674f6 plus gated cancellation latency, without concurrent workloads.
- [x] Keep the change only if measured overhead improves; record every trial.
- [x] Independent review, resolve material findings, commit and update PR #50.

Primitive/bulk/semantic/host preemption limits remain unchanged. Report any
remaining regression against pre-cancellation e7eb003; improvement against PR #50
alone does not establish recovery of all original performance.

## Investigation update

The threshold-only candidate was discarded: five pairs showed +0.9% cached /
+0.1% fresh unscoped time and -1.6% cached / -0.3% fresh scoped time. All three
trial reports remain under `polling-threshold-trial-*.json`; their source hashes
identify the discarded code.

The retained candidate specializes postorder walks once per scope presence.
Active walks check fixed 1,024-unit boundaries and visit every logical counter
value, including scheduled children and elided leaf exits. A final boundary
check catches cancellation from the last callback. Parser/recovery pollers retain
the original threshold implementation for jumps and rewinds. Existing structural,
logical-limit and cancellation gates remain; verify the final callback regression
against the original traversal before final measurements.

Final outcome: keep traversal specialization. Five of five unscoped cached
pairs improve (median -10.9%, projection -13.8%). Active-scope cached -0.8% is
treated as effectively unchanged; fresh time is unchanged. Direct e7eb003
comparison retains +8.6% cached / +6.9% fresh active-scope overhead. All 18 final
cancellation trials pass, 0.08–0.74 ms. Full workspace767/3ignored, native LSP228,
native CLI6, strict Clippy, format and patch gates pass. Review entry-span issue
fixed RED→GREEN; original final-callback failure reproduced then fixed GREEN.
Report and raw data: docs/benchmark-2026-11-09-polling.md.
