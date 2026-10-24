# Milestone B execution ledger

Plan: `superpowers/plans/2026-10-24-error-recovery.md` (approved 2026-10-24).
Base: aa82837. Work in place on the explicitly approved recovery branch.

Pre-flight: Tasks 1–3 share `RecoveredParse`, tree sidecar and recovery options;
Task 4 consumes that syntax and supplies a guarded Program; Task 5 serializes
both without executing; Task 6 measures all earlier interfaces. No conflicting
interface names found. The strict parser remains the zero-repair authority.

Ruling: Use this tracked ledger instead of ignored SDD scratch and commit-based
completion scripts: the approved plan explicitly leaves work uncommitted.

Task 1: in progress.

Task 1: complete — three API/coverage/barrier tests RED (missing APIs) → GREEN;
workspace baseline passed. Metadata fallback is scaffolding, not admission.
Task 2: initial repair tests RED → GREEN (missing delimiter, skipped Unicode,
five separate errors). Real CSharp4 and Inference tests added.
Ruling: The plan's Inference example uses bare `x`, but this fixture requires
`value(x)`. Correct the test source; treating bare x as valid would change the
language rather than test recovery. The valid CSharp4 corpus remains identical.

Task 2: exhaustive local search cost agrees with a brute-force edit-distance
oracle for all 156 strings of length 0–3 over a five-character alphabet.
Task 3: missing-semicolon C# preserves the next method; the full accepted
CSharp4 corpus returns identical strict trees. Random UTF-8 coverage passes.
Ruling: do not commit a repair at an optional (`max=1`) boundary; optional
prefixes must also retain their absent path when their full strict match fails.
Ruling: rollback trivia after unsuccessful postfix/repetition probes, matching
strict parser marks. A focused ReturnType-span regression failed then passed.
Task 4: partial HIR/error propagation and immutable execution barrier implemented;
missing declaration names create no symbols. Typed-profile, unused-template-body,
independent semantic error and host operation/value-provider tests pass.
Task 5: first measured gate failed. Keep failures in the report; do not admit
Milestone B on test counts. Full accepted frozen + generated baseline corpus
is frozen with explicit invalid-generator and inapplicable-operation records.
Recon: fetched Nitra recovery source and TokenChanges at commit
231a1ba97c23ef1ac89365cec003a59d92322336. It schedules parse records using a heap
ordered by cumulative insert/delete costs, favors insertion on equal total
cost, and falls back to deletion on timeout. Our bounded local search is not
a claim of porting Nitra or finding a globally minimum file repair.

Ruling: integrate merged tree-slim PR 7 (3784a91) before continuing recovery. Preserve compact aliases and indexed labels, and move recovery evidence to Stage 14; earlier recovery measurements are historical until rerun on this base.

Task 3: compact-tree adaptation preserves indexed edge labels and alias elision; core 11/11 and CLI 18/18 recovery tests pass (one release scaling test ignored in debug). A missing-condition-closer regression reproduced five bogus diagnostics before the fix and now yields one local missing `)`. A frozen truncated query regression lost earlier completed statements before the fix and now preserves them.
Ruling: zero-cost list boundaries may compact the selected continuation only when there are no competing queued states and no reopenable prefix. A repaired boundary must enclose the first repair location. Resynchronization starts at the furthest reached position, never before it; ties prefer the later repair origin. This preserves optional/alternative paths and prevents fallback from discarding already recognized prefixes.
Task 4: missing/unknown template parameter annotations now produce error types, suppress only derivative diagnostics, and preserve healthy later calls. Missing template/parameter names never enter declaration maps. Regression tests failed before the change and pass afterward.

Task 6: final independent read-only review by gpt-6-astra found no Critical findings and one diagnostic-cascade finding.
Final: Ruling: regrade unresolved-call overload cascade from Minor to Important for this recovery milestone: derivative type diagnostics obscure which failures require user action. Preserve the primary unresolved binding error and healthy later HIR; cost if wrong is suppressing a useful independent overload diagnostic, guarded by a focused recovery test and unchanged strict-mode checks.
Final: fixed unresolved calls entering empty overload selection — `unresolved_call_reports_only_the_primary_binding_error` RED (syntax + binding + derivative type) then GREEN (syntax + binding; healthy later call). Full suite verification follows.
Ruling: compact only newly selected event/probe suffixes at stable boundaries, retain failure cache facts at/after the continuation, and preserve relative parser memo arenas. This bounds speculative retention without repeated full-prefix copying.
Ruling: reopen nested partial strict successes through the original furthest failure; include this context in state dominance. The optional-else regression failed then passed, reducing the dense scaling case to the actual missing-semicolon repair.
Ruling: equal-cost ranking excludes skipped bytes that no authored terminal or outer lexical rule can recognize. Such scalars must be deleted, and prioritizing their deletion retains all zero-cost alternatives. Actual skipped spans and repair costs remain reported. Immediate commitment was rejected by the frozen gate; the original 4096-state budget remains. The nested-closer regression and exhaustive tiny repair oracle pass.

Task 5: complete — 757 accepted baselines, 4,334 frozen single edits, explicit 135 invalid generated programs and 208 inapplicable operations; broad gates pass overall and every C# operation family. Added 1,286 separately frozen every-token/multiple/reviewed cases; all coverage/determinism gates and eight one-primary/preserved-sibling checks pass. Seven metric tests include supplementary Roslyn offsets, semantic identity/origin mapping and a RED→GREEN check that limit diagnostics count in the cascade gate.
Task 4: shadowing/namespace/initializer-visibility tests pass: an error-valued redeclaration shadows only its namespace, while its initializer binds the previous declaration.
Ruling: keep search-contract tests together in the existing core recovery test target rather than a second `recovery_search.rs` harness; all named behaviors remain covered.
Ruling: count all C# recovery diagnostics, including limits, in the Roslyn cascade gate. Keep primary syntax counts separately for diagnosis. Excluding limit failures would understate cascades; the stronger gate still passes every family.
Task 6: 186 local workspace tests pass (two scaling tests excluded from the debug suite); formatting, strict clippy and bootstrap fixed point pass. All live parity/preparation scripts and clean offline source-only native deployment pass. Older typed phase retained allocations are unchanged in all repeated workloads. Final large scaling/work-bound results follow.

Task 6: complete — 12 final scaling cases (256 KiB, 4 MiB, 16 MiB targets × one/dense/junk/truncated) pass work-per-byte bounds; the independent fixed-density 256 KiB/4 MiB release regression passes. At 16 MiB: one error 41.67 s / 844.02 MB requested peak; dense errors 202.08 s / 1070.70 MB peak. These costs remain explicit.
Final: independent review fix verified by focused RED→GREEN and the green workspace suite; no deferred findings or declined behaviors. All approved local admission gates pass; 31 C# case-level preservation regressions and 95 case-level total-diagnostic regressions remain published despite aggregate/per-family success. Hosted recovery CI and incremental/editor integration remain unverified/unimplemented. Changes remain uncommitted as authorized.

Final storage ruling: retain the explicit coverage sidecar in this milestone rather than introduce a new compression design after the verified gate. At 16 MiB, syntax node count/layout matches strict Stage 13, but coverage grows retained storage from 210 MB to 411 MB. Report this cost explicitly; compression is future work.
Final integration: keep `codex/csharp-recovery` as-is in the existing checkout, per the approved no-commit/no-push plan. No worktree cleanup applies.

Integration authorization (2026-10-25): user explicitly requested “merge and push.” This supersedes the implementation plan’s earlier keep-uncommitted handoff. Integrate `codex/csharp-recovery` into the repository default `main`, including its existing CSharp4/compact-tree dependencies; preserve the sibling slim-tree checkout.
