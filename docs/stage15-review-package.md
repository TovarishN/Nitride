# Shared subtree whole-change review

Base and HEAD are both 88cbc93: authorized scope prohibits commits. Review all local tracked changes AND untracked implementation, not an empty commit range. Complete extended-context patch: /tmp/nitride-shared-review.diff. Live files are authoritative if needed.

Plan: docs/superpowers/plans/2026-10-25-shared-subtrees.md
Spec: docs/superpowers/specs/2026-10-25-shared-subtrees-design.md
Ledger and Ruling decisions: docs/stage15-progress.md

The original flat parser is deliberately preserved as an independent oracle and compatibility API. New parse_snapshot/edit/compile_snapshot APIs opt into shared storage. No dependency, commit, push or integration is implied.

Current implementation includes immutable packed syntax chunks, weighted persistent child/source sequences, memo/winner/probe root ownership, dependency-indexed strict edits, recovery VM reuse with bounded outer probe caching, persistent relative coverage/repairs/diagnostics/damage, explicit dense exports and occurrence-based semantics. Limit recovery uses explicit legacy fallback to preserve exact prior diagnostics.

Validation: final workspace suite has 269 top-level passes plus the nested small-stack subprocess check, zero failures; strict all-feature clippy, formatting, bootstrap, source-only offline native build, both complete frozen manifests (4,334 standard /1,286 adversarial), and 100,000-edit zero/eight-reader windows pass. All 48 instrumented edit cases meet latency/copy gates. The complete 15-workload instrumented matrix misses all cold-time gates and two peak-memory gates; quantitative admission is not met. Uninstrumented controls are captured separately.
Review correctness, ownership, dependency soundness, limitations and plan fulfillment. Final measurements will be judged by the executor. Current numeric runs are being restarted after an actual outer-cache pressure regression; this is an opt-in local implementation, not an automatic default-backend cutover.

## Final source and test manifest

- crates/nitride-cli/Cargo.toml
- crates/nitride-cli/src/bin/profile.rs
- crates/nitride-cli/tests/incremental.rs
- crates/nitride-cli/tests/incremental_recovery.rs
- crates/nitride-cli/tests/profiling.rs
- crates/nitride-cli/tests/profiling_mode.rs
- crates/nitride-cli/tests/shared_syntax.rs
- crates/nitride-core/src/incremental/dependencies.rs
- crates/nitride-core/src/incremental/mod.rs
- crates/nitride-core/src/lib.rs
- crates/nitride-core/src/parser.rs
- crates/nitride-core/src/parser/shared.rs
- crates/nitride-core/src/parser/shared/children.rs
- crates/nitride-core/src/recovery/incremental.rs
- crates/nitride-core/src/recovery/mod.rs
- crates/nitride-core/src/recovery/search.rs
- crates/nitride-core/src/recovery/search/shared.rs
- crates/nitride-core/src/recovery/search/source.rs
- crates/nitride-core/src/recovery/tree.rs
- crates/nitride-core/src/semantic.rs
- crates/nitride-core/src/syntax.rs
- crates/nitride-core/src/syntax/access.rs
- crates/nitride-core/src/syntax/arena.rs
- crates/nitride-core/src/syntax/memory.rs
- crates/nitride-core/src/syntax/sequence.rs
- crates/nitride-core/src/syntax/shared.rs
- crates/nitride-core/src/syntax/sidecars.rs
- crates/nitride-core/src/syntax/text.rs
- crates/nitride-core/src/templates.rs
- crates/nitride-core/tests/incremental.rs
- crates/nitride-core/tests/wide_allocations.rs
- scripts/shared_subtrees.py
- scripts/test_shared_subtrees.py

## Post-review author verification

The original patch above records the review input. The final source patch contains the four accepted Important fixes, bounded failed facts and construction hints, persistent child runs, coverage-gap correction, directory lookup/charging fixes, sound prefix reuse, selected-fact retention, maintenance allowance and warmup gate correction. The validation above applies to this frozen implementation. The reviewer reached its usage limit before a final verdict; no independent final post-fix sign-off is inferred.

Final reviewable source patch: `/tmp/nitride-shared-final-source.diff`; file manifest: `/tmp/nitride-shared-final-source-files.txt` (33 changed/new source, test and build files). This supplements the preserved original reviewer input. README, plan/spec, execution ledger and numerical JSON reports remain separate in this checkout. See `docs/stage15-evidence.md` for numerical results and limitations.
