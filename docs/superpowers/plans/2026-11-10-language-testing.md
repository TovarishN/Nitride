# Native Language Testing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans for inline implementation; one fresh whole-branch review at the end. Steps use checkbox syntax.

**Goal:** Implement approved native `.ntest` suites, a deterministic CLI runner, generated starter suites and VS Code Testing with unsaved inputs.
**Architecture:** A self-described test grammar feeds a shared Rust testing crate. A shared preparer overlay entry point preserves ordinary disk behavior. A CLI worker owns execution and structured output; the extension owns discovery, TestController state, snapshots and worker lifecycle.
**Tech Stack:** Existing Rust workspace, native prepare/codegen/core, serde_json; TypeScript VS Code Testing API, node child_process.
**Spec:** ../specs/2026-11-10-language-testing-design.md (approved with "yep").

## Global Constraints
- `.ntest` authoring syntax as approved, version1; native grammar frontend, retained original spans, raw `<[...]>` bytes and JSON-decoded quoted strings.
- Exactly one source/file and expectation per case; exactly one language project per suite; unique nonempty names; strict phase/diagnostic/value assertions.
- Case execution isolation; prepare once per project/snapshot; no leaked language owners.
- Unsupported/preparation/load/limit/preflight errors never satisfy a negative source case; no external bindings, grants or clock nondeterminism; Today=2000-01-01.
- JSON only for reports/events/snapshots. Exit0 pass,1 assertionfailure,2 infrastructure; incomplete streams never pass.
- Editor snapshots preserve unsaved input; no save side effects, no test execution in LSP loop. Trust/path/late-result/cancellation guards apply.
- Preserve main checkout, native no-default-features builds, existing disk preparation/CLI behavior and frozen stdlib1.0.0. No release/version bump.

## Review Focus
- Non-ASCII and escaped/raw text preserve correct source spans and diagnostic comparisons.
- Earlier-phase and infrastructure failures cannot masquerade as successful rejection cases.
- Integer versus float comparisons preserve large integer differences.
- Unsaved manifest changes alter dependency discovery; stale worker events cannot replace newer results.
- Cancelled/malformed workers and symlink/outside-root paths cannot yield passing tests or unintended writes/process launches.

### Task1: Native suite grammar and typed decoding
Files: new languages/testing/{Testing.ngr,NitrideTests.nproj}; new crates/nitride-testing/{Cargo.toml,build.rs,src/lib.rs,src/suite.rs,tests/suites.rs}; root Cargo.toml/Cargo.lock.
Interfaces: Suite {project:String,tests:Vec<Case>}; Case {name,span,name_span,input:Input,expect:Expectation}; Input::{Inline{text,raw_span},File(String)}; Phase::{Parse,Compile,Run}; ExpectedDiagnostic {category,code:Option<Option<String>>,span:Option<[u32;2]>}; SuiteError {span,message}; parse_suite(text:&str)->Result<Suite,SuiteError>. Types may be split into model.rs for focus.
- [x] Write suite parsing tests first: native syntax, raw versus quoted source/Unicode, fixture, phase/value/diagnostic forms and declaration spans; invalid repeated/unknown/missing declarations, names, version, number notation and source boundaries. Run expected RED missing API.
- [x] Add crate with generated frontend build from Testing.ngr through existing prepare/codegen and pinned lexical sources. Decode engine SyntaxTree fields, validate declarations and limits, retaining spans; no regex parser. Generate suite/case metadata JSON through explicit methods.
- [x] Run focused tests, strict crate Clippy and formatting; commit frontend/model.

### Task2: Snapshot preparation, runner and CLI protocol
Files: prepare/src/{project.rs,lib.rs}, tests/project_overlays.rs; testing/src/{snapshot,runner,compare,protocol}.rs and tests/runner.rs; cli/src/{test_command.rs,main.rs}, Cargo.toml and tests/language_tests.rs.
Interfaces: InputSnapshot owns normalized path/text overrides with read/exists/identity operations; prepare_project_with(path, reader) with disk wrapper preserves metadata/errors/physical inputs. testing::discover(path,snapshot)->suite+dependencies metadata; run(suites,selectors,snapshot,on_event)->Report; report/events are version1 JSON and include exact statuses/observations. Shared numerical comparison has exact integral handling.
- [x] RED overlay tests: disk parity, changed grammar/project/catalog/source fixture, newly unsaved file under existing parent, relocated roots, invalid/duplicate identities and immutable library source rejection. Refactor physical project I/O to reader injection, retaining existing disk APIs.
- [x] RED direct runner tests for phase successes/rejections, full diagnostics/code-none/span checks, unexpected earlier errors, bad project/preflight/limit as errored, fixed Today and isolated cases; integer precision and array/value mismatches. Implement shared observation/compare/report owners using direct core APIs and preflight.
- [x] RED real binary tests for explicit/default suites, multiple suites, selectors, list/dependencies without prepare, reports/events/exit codes and stdin overlays. Add CLI dispatch, argument validation/bounds and incremental event writer. Reject grants for this command.
- [x] Verify runtime observations against direct engine APIs and old prepare/project suites; run focused native tests/Clippy, commit shared runner/CLI.

### Task3: Starter suites and user documentation
Files: prepare/src/authoring.rs and tests/authoring.rs; testing/tests/starters.rs; cli/tests/authoring.rs; docs/language-testing.md and docs/authoring.md; release/train.py.
- [x] RED tests for both generated presets, exact expected file count/new tests.ntest, quoted/raw source correctness and genuine diagnostic codes/spans. Ensure all generated suites pass with native CLI. Keep no-overwrite/manifest-last/conservative rollback tests meaningful with the new file.
- [x] Add starter suites and README commands using shared planner only. Include parse accept/reject, typed42 and compile rejection; run against the actual engine before freezing expected diagnostics.
- [x] Update native training to run generated suites, retaining version/library/runtime checks. Write usage/phase/snapshot/protocol/capability boundaries documentation.
- [x] Run focused authoring/testing/CLI/tooling checks; commit starter workflow.

### Task4: VS Code Testing and final acceptance
Files: extension src/{testing,testingProtocol,testingWorker}.ts with helper tests; extension.ts/server.ts/package.json; syntax ntest.tmLanguage.json; acceptance.cjs and scripts/editor_acceptance.py as needed; docs/status.md and acceptance evidence.
Interfaces: native executable selection shared with LSP; TestController consumes list metadata/case spans and events; frozen input revision maps guard publication. Worker accepts argument array/stdin, bounded result buffering, cancellation and abnormal exit. Dedicated suite DiagnosticCollection owns only ntest syntax/validation errors.
- [ ] RED helper tests for protocol validation/truncation/exit mismatch, selected/excluded cases, byte→UTF16/raw source ranges, normalized/symlink workspace paths, dependency snapshot changes, cancellation and late completion. Implement focused helpers and run them in existing npm test harness.
- [ ] Add native ntest language/highlighting and TestController discovery/open-buffer refresh, invalid suite errors and run profiles. List against frozen suite/project overlays, capture dependency buffers, restart discovery if changed, launch one worker per suite at a time. No shell, silent saves or LSP test execution.
- [ ] Exercise actual packaged native VSIX: generated suite discovery/run, pass/fail/value/diagnostic inspection, unsaved grammar/fixture edits without disk mutation, selected/excluded case, syntax error recovery and cancellation with no late success. Report extension-host API scope separately from visual/new-window behavior.
- [ ] Run full workspace/native/all-feature Clippy/fmt/bootstrap/extension/tooling/optional scanner gates appropriate to modified owners. One fresh whole-branch review, one regression-backed material fix pass, final suites green; no re-review. Commit evidence, push feature branch and open PR; no merge/publish.

## Preflight and self-review
Task1 model feeds Task2 and Task4 via explicit JSON metadata; Task2 reader API feeds runner/editor overlays and retains physical inputs separate from libraries. Task3 planner remains independent of testing crate to avoid prepare↔testing cycles; testing integration tests consume generated plans. Task4 runs CLI workers, avoiding CLI↔LSP dependency cycles or server scheduling regressions. Build-generated Testing frontend consumes prepare/codegen without a test-runner bootstrap cycle. Ordinary .ntest files are tooling suites, not automatically claimed workspace programs; syntax diagnostics have one owner. No spec conflicts found; internal concrete structs may adapt without changing contracts, with rulings ledgered if behavior changes.
