# Host-binding crate tests and contract checks

**Goal:** Give nitride-process and nitride-wasm direct tests independent of CLI helpers, and repair concrete contract failures exposed by them.

User requested continuing and committing. Existing commits 29ea6ca and 81f098f are complete; keep unrelated local files untouched and commit this slice separately.

- [x] Add process public-API tests using a tiny std-only Rust fixture compiled by rustc into a temporary directory. Exercise request IDs, exact params, malformed/closed/mismatched replies, declared vs host failures, nested values and missing record fields. No Python or CLI dependency.
- [x] Add wasm crate-local value conversion tests plus direct binding tests using the checked-in Calendar component. Cover nested record/list/scalar values, invalid metadata, real calls, declared failures, fuel exhaustion and same-arity type mismatches.
- [x] Observe failing regressions, then reject malformed JSON-RPC envelopes and invalid/truncated outgoing record data. Ensure errors retain host.process/host.component identity. Validate component parameter/result types during loading, including result wrappers and record field names. Preserve valid current contracts.
- [x] Run both crates independently, existing process/component integrations, workspace tests, all-feature clippy, formatting, version and bootstrap checks. Request independent review, document coverage and limits, and commit.

Use existing dependencies and fixtures. Pure converter tests stay next to implementation; real process transport tests use a reusable temporary child fixture. No deadlines, asynchronous transport redesign, arbitrary metadata schema overhaul or Wasmtime cache redesign in this slice. Existing synchronous-call and host-code limits remain explicit.

Review ruling: strict record-name checking exposed an existing discrepancy between
codegen and adapter WIT names. Three regressions reproduced numeric-suffix and
underscore failures. Extracted the generator's existing unescaped algorithm into
core::host::binding_name and reused it in codegen/process/wasm; keyword escaping
remains in codegen. Generated WIT output is unchanged and no dependency is added.
Independent re-review reports no remaining actionable issues.

Final verification: 15 own-crate tests pass (5 process, 5 wasm unit/conversion and
5 direct component); existing CLI process/component integrations pass (7 tests),
and all 4 WIT-generation tests pass unchanged. Full workspace: 847 passed /
0 failed / 3 ignored. All-feature clippy, formatting, rustdoc with warnings
denied, documentation links, version consistency and all four bootstrap fixed points
pass. Missing-record/protocol/type/panic regressions and naming regressions were
observed failing before their fixes. Independent re-review has no actionable issues.
This slice is committed separately; no push or hosted acceptance is claimed.
