# Standalone embedding API implementation plan

**Goal:** A Rust application can own a typed `.ngr` language and parse, check and execute it without importing the CLI or preparing grammars at runtime.

**Scope:** User approved proceeding with the embedding API and previously delegated implementation choices. Continue on the existing feature branch, preserving all prior cleanup/fuzzing and unrelated files. Initially leave changes reviewable and uncommitted; subsequent user instructions authorized committing.

**Design:** Add the `nitride` facade, depending on core only by default. Re-export core contracts so existing generated Rust can use a single facade dependency (document the root `extern crate nitride as nitride_core` alias). `Engine<'g>` borrows language tables, owns limits and strict parse options, and provides `parse`, `compile`, `run`, and `run_with_values`. `Error` retains phase, all diagnostics and the invoked-operation trace (including a failing call). Run preflights the entire program before host effects; executor revalidation remains intact. Core APIs and grammar admission stay unchanged.

The facade's optional `build` feature enables `nitride::build::compile_project(project, output)` for build scripts. Relative projects resolve from Cargo's manifest directory; output is a single filename below `OUT_DIR`. It prepares once, emits both tables and interface bindings, and registers every prepared input with Cargo. No environment mutation in tests. Build-only dependencies stay out of the runtime dependency graph under resolver 2.

Compatibility: documented public source API compatibility within a `0.x` minor series; breaking changes require a new minor plus migration notes. Generated Rust is build output tied to matching facade/build-helper versions, not a persistent serialization or ABI contract. No crates.io publication or package-name availability claim.

- [x] Add workspace facade and standalone calculator package with owned grammar/catalog; add runtime acceptance tests for successful host execution, parse/check/preflight errors with zero effects, complete preflight diagnostics, host failure trace, syntax-only rejection, limits and explicit host values. Observe the absent API failure before implementation.
- [x] Implement Engine and phase-aware errors, preserving borrowed language lifetimes and strict comment checking.
- [x] Implement the build helper; test generation from a real project, tracked imports, invalid projects and output traversal rejection through an injected internal Cargo context. Verify generated interfaces using a real interface fixture.
- [x] Add standalone build/run recipe, semver boundaries and documentation links; replace the README's CLI embedding recommendation.
- [x] Run example, workspace tests, all-feature clippy, formatting, version checks and standalone consumer checks. Verify the normal dependency graph excludes CLI/LSP/prepare/codegen/Wasmtime. Request fresh review and fix substantive findings.

Progress: the absent build/Engine API failed compilation as expected before implementation.
Eight standalone acceptance tests and three build-helper tests pass. An external copy
of the application printed 42 with no CLI dependency; mutating its catalog changed the
result to 13, changing its grammar triggered a Parse error, and restoring inputs restored
42, verifying Cargo input tracking. Default normal dependency graphs contain only
nitride and nitride-core. Rustdoc builds with warnings denied. Fresh independent reviews
found no actionable implementation or documentation issues.

Ruling: broader all-feature verification exposed an existing recovery-profiler test
assuming numeric allocator metrics with uninstrumented-profiler enabled. Corrected the
assertion to check null in that mode while retaining the measured bound otherwise;
both targeted configurations pass. Production profiler behavior is unchanged.

Verification ruling: the first parallel default/all-feature runs shared a target
binary path, so the uninstrumented profiler replaced the default profiler during
its tests. Re-ran the complete all-feature suite successfully (817 passed, 3 ignored),
then launched default workspace verification sequentially. External copied-consumer
tests pass 8/8. No production change was made for that test-run collision.

Final verification: default workspace 824 passed / 0 failed / 3 ignored; all-feature
workspace 817 passed / 0 failed / 3 ignored. All-feature all-target clippy, default
facade rustdoc with warnings denied, formatting, version consistency, and all four
bootstrap fixed points pass. External consumer acceptance passes 8 tests and confirms
catalog/grammar rebuild tracking. Default runtime dependency graph is core-only.
Independent review has no remaining actionable findings. Completed in commit `29ea6ca`;
no publication or hosted cross-platform acceptance is claimed.
