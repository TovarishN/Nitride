# CSharp4 Admission (Milestone A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Nitride prepares CSNitrogen's CSharp4 grammar and parses valid, preprocessing-free C# with syntax trees identical to Nitrogen's, at close to linear time on large files.

**Architecture:** Vendor the 30 CSharp4 modules with provenance. Close each frontend gap in the existing pipeline: `Grammar.ngr` and the decoder, `normalize.rs`, the live exporter, codegen, and the core parser. Then prove parity with a syntax oracle over the C# corpus and generated programs. Grammar functions are expanded in `nitride-prepare`, exactly as Nitrogen's `GrammarFunctions.Expand` does before validation, so later stages never see a call.

**Tech Stack:** Rust (stable, edition 2024, `rust-version = "1.85"`), development-only .NET 10 reference tool over the sibling `../Nitrogen` checkout, Python 3 parity scripts.

**Spec:** `docs/superpowers/specs/2026-10-24-csharp-recovery-incremental-design.md` (Milestone A only; B and C get their own plans).

## Global Constraints

- Valid-file behavior and Nitrogen parity of every existing profile stay identical: all frozen and generated comparisons and the bootstrap fixed point must keep passing.
- No .NET or third-party dependency in the native core; preparation keeps only `serde_json` and `sha2`.
- C# input is preprocessing-free; preprocessing is out of scope.
- No C#-specific code in the engine; grammar-specific behavior lives in grammars.
- Limits return structured errors; no stack overflow, hang or superlinear blowup on adversarial input.
- Reference: the sibling `../Nitrogen` checkout (HEAD `ed69ac8a`, which contains CSNitrogen's pinned `a59ab220`). Record the revision in evidence.
- Grammar source: CSNitrogen commit `c0413ce`, `src/CSNitrogen.Syntax/nitrogen.json` language `CSharp4`, start `CSharp.CompilationUnits.CompilationUnit`, vendored with SHA-256 provenance.
- Existing discipline: test-first; evidence and footprint recorded in `docs/stage12-*`; CI on macOS/Linux/Windows; commit per task on branch `codex/csharp-recovery`.

## Review Focus

1. Supplementary-plane identifiers (for example `𝑥`, U+1D465) and `\U0001D465` escapes must parse like Nitrogen's UTF-16 parse, with UTF-8 byte spans. Owned by Task 4.
2. Grammar-function expansion must be deterministic and bounded: deep recursion, huge `choice` ranges and class blowups return errors, not overflows. Owned by Task 5.
3. A `transparent` alternative leaves no node, yet the child keeps its own span and the enclosing structure is unchanged. Owned by Task 2.
4. A call-style postfix (`f(x)(y)[z].w`) composes with precedence exactly as in Nitrogen. Owned by Task 3.
5. A 16 MiB generated C# file parses in time and memory roughly proportional to its size, and deep nesting fails with a structured limit error. Owned by Task 7.

---

### Task 1: Vendor CSharp4, wire the reference, finish the inventory

**Files:**
- Create: `languages/csharp4/Grammars/*.ngr` (the 30 modules of CSNitrogen's `CSharp4` list, byte-identical), `languages/csharp4/provenance.json`, `languages/csharp4/project.json`, `languages/csharp4/catalog.json`
- Create: `scripts/csharp_inventory.py`, `docs/stage12-inventory.md`
- Modify: `tools/Nitride.Reference/Nitride.Reference.csproj`, `tools/Nitride.Reference/SyntaxOracle.cs`

**Interfaces:**
- Produces: `project.json` = `{"mode":"syntax","start":"CSharp.CompilationUnits.CompilationUnit","grammars":[<30 paths in CSNitrogen order>],"catalog":"catalog.json"}`; catalog `{"types":[],"operations":[],"statements":{}}`.
- Produces: `provenance.json` = `{"source":"TovarishN/CSNitrogen","commit":"c0413ce…(full sha)","language":"CSharp4","files":{"<path>":"<sha256>"}}`.
- Produces: reference command `oracle-syntax csharp4 <source> <prepared.json>` with the same output schema as the existing `xml`/`composition` profiles.

- [ ] **Step 1:** Copy the modules and write `provenance.json`; add a test in `crates/nitride-prepare/tests/frontend.rs`, `vendored_csharp4_matches_provenance`, that hashes every listed file and compares it with `provenance.json`.
- [ ] **Step 2:** Run `cargo test -p nitride-prepare vendored_csharp4_matches_provenance`. Expected: PASS.
- [ ] **Step 3:** Add the 30 modules as `AdditionalFiles` with namespace `Nitride.Reference.CSharp4` and the `csharp4` profile in `SyntaxOracle`. Run `dotnet build tools/Nitride.Reference`; Nitrogen's generator must build them, since it supports all four constructs.
- [ ] **Step 4:** Write `scripts/csharp_inventory.py`. It copies the grammar to a temp dir, applies stubs for the four known gaps (the method used in the recon: functions → literal placeholders, surrogate pairs → `"\u0001"`, `transparent` removed, non-literal infix heads reported rather than stubbed), and repeatedly runs `nitride prepare`. It records each new error location, stubs it generically, and stops when preparation succeeds.
- [ ] **Step 5:** Run it. Record every gap beyond the known four in `docs/stage12-inventory.md` with a grammar excerpt. **If new gaps exist, add one task per gap after Task 5, in the style of Tasks 2–4, before continuing.**
- [ ] **Step 6:** Commit: `git commit -m "Vendor CSharp4 grammar, reference oracle and admission inventory"`.

### Task 2: `transparent` alternatives

**Files:**
- Modify: `languages/grammar/Grammar.ngr`, `crates/nitride-prepare/src/parser.rs`, `crates/nitride-prepare/src/normalize.rs`, `crates/nitride-prepare/src/grammar_seed.rs` (regenerated), `crates/nitride-codegen/src/lib.rs`, `crates/nitride-core/src/grammar.rs`, `crates/nitride-core/src/parser.rs`, `tools/Nitride.Reference/Exporter.cs` (emit `"transparent": true` instead of rejecting), test literals in `crates/nitride-core/tests/*.rs` and `crates/nitride-cli/tests/pipeline.rs`
- Test: `crates/nitride-core/tests/transparent.rs`, `crates/nitride-prepare/tests/frontend.rs`

**Interfaces:**
- Produces: normalized alternative rules carry `"transparent": true` only when marked; the key is absent otherwise, so older exports are unchanged.
- Produces: `Rule.transparent: bool` in core. A transparent prefix alternative pushes no node of its own; its single element's node stands in. Semantics follow Nitrogen's language guide §transparent: `transparent` is the marker only when `Name =` or an element follows it; a postfix or nullable transparent alternative is rejected.

- [ ] **Step 1: Write failing tests.** `transparent_alternative_yields_its_element_node` builds a hand-written language with `| transparent Plain = Body:(Var / ";")` and asserts the parsed tree has a `Var` node directly under the point's parent, with the `Var` span. `transparent_marker_requires_name_or_element` prepares `| transparent` (a reference to a rule named `transparent`) and the marked form, and asserts both normalize as Nitrogen does.
- [ ] **Step 2:** Run `cargo test -p nitride-core --test transparent`. Expected: FAIL (no `transparent` field).
- [ ] **Step 3:** Add `TransparentMarker` to `Alternative` in `Grammar.ngr`, decode it, emit `"transparent":true`, validate (rejected when postfix, multi-element or a predicate), and add `transparent` to `Rule`. Codegen emits it. In `Parser::pratt`, a winning transparent prefix contributes its child edge instead of its own node.
- [ ] **Step 4:** Regenerate the seed (`cargo run -p nitride-prepare --example bootstrap`, then `-- --check`) and run `cargo test --workspace`. Expected: all pass.
- [ ] **Step 5:** Commit: `git commit -m "Admit transparent extensible alternatives"`.

### Task 3: Non-literal infix and postfix operators

**Files:**
- Modify: `crates/nitride-prepare/src/normalize.rs:300-320`, `crates/nitride-codegen/src/lib.rs:720-740`, `tools/Nitride.Reference/Exporter.cs:185-200`
- Test: `crates/nitride-codegen/tests/profiles.rs`, `crates/nitride-core/tests/precedence.rs`

**Interfaces:**
- Produces: an infix/postfix alternative's second element may be a rule reference (labeled or not). The conflicting-operator check applies only to literal operators. Precedence is still required (1..255).

- [ ] **Step 1: Write failing tests.** `call_postfix_with_reference_operator` hand-builds `Invoke = Target:E Args:Arguments precedence 180 left` and `Member = Target:E "." Name:Id precedence 180 left`, parses `f(x)(y).w`, and asserts the tree nests `Member(Invoke(Invoke(f,(x)),(y)),w)`. `reference_operator_infix_is_generated` asserts codegen accepts `Left:E MultiplyOp Right:E precedence 160 left`.
- [ ] **Step 2:** Run both. Expected: FAIL with "infix must start with a literal operator".
- [ ] **Step 3:** Relax the three checks to literal-only conflict detection. The core parser already parses any element sequence after the left operand; adjust only if the Step 1 test shows otherwise.
- [ ] **Step 4:** Run `cargo test --workspace` and `python3 scripts/prepare_parity.py`. Expected: all pass, with existing counts unchanged.
- [ ] **Step 5:** Commit: `git commit -m "Admit infix and postfix operators that are rule references"`.

### Task 4: UTF-16 surrogate-pair grammars over UTF-8 input

**Files:**
- Modify: `crates/nitride-prepare/src/literal.rs` (accept surrogate code units inside character classes only), `crates/nitride-codegen/src/lib.rs` (pairing transform)
- Test: `crates/nitride-codegen/tests/generation.rs`, `crates/nitride-core/tests/public_grammar.rs`

**Interfaces:**
- Normalized JSON keeps UTF-16 ranges exactly as the exporter already emits them, so the exporter needs no change and parity is unchanged.
- Produces: `fn pair_surrogates(expr: &Value) -> Result<Value, String>` in codegen, applied before emission. Within a lexical sequence, a class containing only high surrogates followed by a class containing only low surrogates becomes one class of the supplementary code points `0x10000 + (h-0xD800)*0x400 + (l-0xDC00)` for every pair. A class mixing BMP and surrogate units keeps its BMP part, and its surrogate part is paired with the next element when that is a low-surrogate class. A surrogate unit that cannot be paired matches nothing (UTF-8 text cannot contain one) and is removed; codegen errors only if that empties a required class.

- [ ] **Step 1: Write failing tests.** `surrogate_pair_classes_match_supplementary_characters` generates a token `T = ['\ud835'] ['\udc65'..'\udc66']` and asserts `T` matches "𝑥" and "𝑦" (U+1D465, U+1D466) with byte span `[0,4]` and rejects "x". `unpaired_surrogate_class_is_inert` asserts `T = ['\ud835'] / "a"` matches "a" only.
- [ ] **Step 2:** Run them. Expected: FAIL ("unpaired UTF-16 surrogate is not admitted").
- [ ] **Step 3:** Implement `pair_surrogates` and the literal change.
- [ ] **Step 4:** Run `cargo test --workspace`. Expected: all pass.
- [ ] **Step 5:** Commit: `git commit -m "Give UTF-16 surrogate-pair grammars their meaning over UTF-8 text"`.

### Task 5: Grammar functions

**Files:**
- Create: `crates/nitride-prepare/src/functions.rs`
- Modify: `languages/grammar/Grammar.ngr`, `crates/nitride-prepare/src/parser.rs`, `crates/nitride-prepare/src/normalize.rs`, `crates/nitride-prepare/src/grammar_seed.rs` (regenerated), `scripts/prepare_parity.py`, `tools/Nitride.Reference/Exporter.cs` (call `GrammarFunctions.Expand` on the parsed modules before export and stop rejecting `module.Functions`)
- Test: `crates/nitride-prepare/tests/functions.rs`

**Interfaces:**
- Produces: `pub fn expand(modules: &mut [parser::Module], sources: &[Source]) -> Result<(), PrepareError>` in `functions.rs`, called by `normalize::prepare` before rule normalization. Every `{"kind":"call",...}` expression becomes the expression it evaluates to; afterwards no call, `if`, comprehension or value class remains.
- Syntax: mirror Nitrogen `GrammarParser.cs` exactly: `ParseFunction` (763–786), `ParsePrimary`/`ParseCall`/`ParseIf`/`ParseComprehension` (420–586), `ParseLets`/`ParseValue`/`ParseBinary` and its precedence table, `ParseUnaryValue`/`ParseNameOrCall` (587–735), and the class bounds `ParseCharClass`/`ParseBound` (453–522). The `CallHead` touching-call rejection now applies only when the name is not a declared function.
- Semantics: mirror `Nitrogen.Grammar/GrammarFunctions*.cs`, including the types `Grammar.Int` (long), `Grammar.Char`, `Grammar.Text`, `Grammar.Class` and `Grammar.Rule`; built-ins `char code upper lower length`; class algebra `& | -` and `+`/`-` shifting by an integer; `choice`/`seq` comprehensions with `let` and `where`; and limits `MaxDepth = 200`, `MaxNodes = 1_000_000`, `MaxSteps = 10_000_000`, reported as preparation errors at the call.

- [ ] **Step 1: Write failing tests** in `functions.rs`:
  - `hex_digit_function_expands_like_nitrogen`: `HexDigit(d)` from CSNitrogen's `Lexical.ngr` with `token T = HexDigit(11);` normalizes to the class `[['B','B'],['b','b']]`.
  - `comprehension_with_let_and_where`: `HexIn(1, ['\u0003'..'\u0005'])` expands to the choice of digit classes for 3, 4 and 5.
  - `contextual_word_seq`: `Contextual("get")` expands to the same sequence the exporter emits.
  - `recursion_and_step_limits_are_errors`: an unbounded recursive function and a 2^40-element `choice` both return `PrepareError` with no panic.
  - `undeclared_call_is_rejected`.
- [ ] **Step 2:** Run `cargo test -p nitride-prepare --test functions`. Expected: FAIL (syntax not admitted).
- [ ] **Step 3:** Extend `Grammar.ngr` and the decoder, implement `functions.rs`, and regenerate the seed (bootstrap, then `--check`).
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Add to `scripts/prepare_parity.py`: CSNitrogen's `Lexical.ngr` alone (syntax mode), the full CSharp4 project, and 12 generated function grammars covering every operator, built-in, comprehension form and error class. Expected: native and live export equal; the error cases are rejected by both.
- [ ] **Step 6:** Commit: `git commit -m "Admit grammar functions with Nitrogen's compile-time semantics"`.

### Task 6: CSharp4 syntax parity

**Files:**
- Modify: `crates/nitride-cli/build.rs` (add `("csharp4","csharp4")`), `crates/nitride-cli/src/lib.rs` (`pub mod csharp4`), `crates/nitride-cli/src/main.rs` (`--parse-csharp4 <source>`), `.github/workflows/ci.yml`, `nitrogen.json`
- Create: `fixtures/csharp4-cases.json`, `fixtures/csharp4-reference.json`, `scripts/csharp4_reference.py`, `crates/nitride-cli/tests/csharp4.rs`, `languages/csharp4/corpus/` (vendored Mono2004/2006/2008 `.cs` files with their `COPYING`, `LICENSE` and `LICENSE.GPL`)

**Interfaces:**
- Consumes: Tasks 1–5 (plus any inventory tasks).
- Produces: `nitride --parse-csharp4 <file>` printing the existing `syntax_observe` schema; `scripts/csharp4_reference.py [--freeze]` comparing native against `oracle-syntax csharp4` on every case, every corpus file and 64 generated programs.

- [ ] **Step 1:** Build cases from CSNitrogen's `tests/Corpus/source-grammar-cases.json`, `source-review-cases.json`, `context-conformance-cases.json`, `attribute-conformance-cases.json` and `CSharp4/source-specific-cases.json`, keeping those valid for CSharp4 per their `expected` and `mode` fields (wrap expression/type modes in a compilation unit as CSNitrogen's tests do), plus invalid ones as rejection cases. Write `crates/nitride-cli/tests/csharp4.rs::csharp4_syntax_matches_real_nitrogen` against the frozen reference.
- [ ] **Step 2:** Freeze with `python3 scripts/csharp4_reference.py --freeze`, then run the Rust test. Expected: FAIL wherever behavior diverges.
- [ ] **Step 3:** Fix each divergence in the engine (never with C#-specific code). Record each one and its fix in `docs/stage12-evidence.md`.
- [ ] **Step 4:** Run `python3 scripts/csharp4_reference.py` and `cargo test --workspace`. Expected: every case, corpus file and generated program matches, and all earlier gates pass.
- [ ] **Step 5:** Commit: `git commit -m "Match Nitrogen's CSharp4 syntax trees on valid C#"`.

### Task 7: Large-file performance

**Files:**
- Modify: `crates/nitride-core/src/parser.rs` (memoization only if measurements require it), `crates/nitride-cli/src/bin/profile.rs` (`csharp4` mode)
- Create: `scripts/csharp4_scaling.py`, `docs/stage12-scaling.json`

**Interfaces:**
- Produces: `scripts/csharp4_scaling.py` generating deterministic C# of 64 KiB, 1, 4 and 16 MiB (classes, methods, expressions and nesting drawn from the corpus shapes). It records native parse time, allocations and peak memory, plus Nitrogen's parse time and allocation through the reference, in `docs/stage12-scaling.json`.

- [ ] **Step 1: Write a failing test** `csharp4_parse_scales_linearly`, `#[ignore]` by default and run in CI release, asserting that the time per byte at 4 MiB is at most 2× that at 256 KiB, and that 300-deep nested parentheses return a `limit` diagnostic.
- [ ] **Step 2:** Run `cargo test --release -p nitride-cli csharp4_parse_scales_linearly -- --ignored`. Expected: FAIL if backtracking is superlinear on this grammar; otherwise record the pass and skip Step 3.
- [ ] **Step 3:** Add packrat memoization keyed by `(rule, position)` for non-lexical rules, with Pratt operands keyed by `(point, position, minimum)`. Memo entries hold the produced node range and end position, using the existing `Winner` buffer copy mechanism, bounded by `Limits::work`. Older profiles' allocation counts may change only through this mechanism, and the evidence must report that.
- [ ] **Step 4:** Run the test, `scripts/csharp4_scaling.py` and every parity script. Expected: PASS, with all comparisons unchanged.
- [ ] **Step 5:** Commit: `git commit -m "Keep CSharp4 parsing linear on large files"`.

### Task 8: Evidence and publication readiness

**Files:**
- Create: `docs/stage12-evidence.md`, `docs/stage12-*-memory.json`
- Modify: `README.md`, `docs/progress.md`, `.github/workflows/ci.yml` (prepare `csharp4`, parse one corpus file)

- [ ] **Step 1:** Run all gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, `cargo run -p nitride-prepare --example bootstrap -- --check`, every `scripts/*_reference.py`, `scripts/differential.py`, `scripts/stage2_differential.py`, `scripts/prepare_parity.py`, `scripts/test_reference.py`, and the memory scripts compared with stage 11. Expected: all pass; record counts.
- [ ] **Step 2:** Do a clean source-only offline build without .NET on PATH (as in stages 9–11) that prepares CSharp4 and parses a corpus file.
- [ ] **Step 3:** Write the evidence: gaps closed, divergences fixed, parity counts, scaling results compared with Nitrogen, footprint, and limits.
- [ ] **Step 4:** Commit: `git commit -m "Record CSharp4 admission evidence"`.
