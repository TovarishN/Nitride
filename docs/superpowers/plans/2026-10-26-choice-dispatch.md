# Ordinary choice dispatch implementation plan

> **For agentic workers:** Execute inline using superpowers:executing-plans and TDD.

**Goal:** Continue reducing full-parse overhead toward Roslyn parity by pruning impossible ordinary choices, building on the verified Stage16 checkpoint.
**Architecture:** Prepare conservative first-primitive selectors lazily for choices with at least four alternatives. Keep a bounded parse-local cache; selectors disappear at publication instead of increasing snapshot retained memory. Preserve ordered-choice semantics and keep the original path for recovery probes. This is another execution specialization, not a new grammar policy.
**Spec:** User-approved lexical/predictive optimization direction and explicit Continue; docs/stage16-evidence.md is the verified starting checkpoint.
**Tech stack:** Existing Rust/std and profiler/Python tools, no dependencies.

## Constraints and review focus
- Preserve all previous dirty work, fixed binaries and reports; no staging/commits/integration.
- Only skip alternatives whose first primitive cannot match. Nullable, predicate-led, recursive/hazardous or deeply wrapped starts stay eligible.
- Keep all fatal grammar checks and the existing recovery work accounting; no early commitment among plausible alternatives.
- Cache at most256KiB of resident selectors; cap one choice at256 alternatives; record active temporary allocation costs honestly. No global caches or snapshot retention of plans.
- Confirm old-reader/source dependencies, Unicode, keyword boundaries, parse modes and first-edit behavior against the flat oracle.

## Task 1: Selector preparation and integration
- [x] Add a RED work-reduction/flat-tree regression for ordinary choices.
- [x] Implement conservative first-primitive selector construction with bounded traversal.
- [x] Integrate a bounded parse-local choice cache, skip only proven failures, and count skips/preparations.
- [x] Run core tests and clean exploratory timing; retain only changes with measured benefit.

## Task 2: Verification and evidence
- [x] Run the complete workspace suite, formatting, strict all-feature clippy and Release build.
- [x] Freeze a new ordinary-allocator profiler; measure three valid full parses per size at256KiB/4MiB/16MiB. Report remaining Roslyn gap without claiming parity.
- [x] Run complete standard/adversarial corpora, a validated first16MiB recovered edit and Release reader-window/lifetime checks.
- [x] Obtain one fresh whole-delta review, address Important findings with RED-to-GREEN tests, and leave reviewable local evidence.

## Execution result

Locally complete and verified, Roslyn parity false. See docs/stage17-evidence.md. The one review finding was fixed with a RED-to-GREEN regression and the full corrected verification/capture. No commits or integration.
