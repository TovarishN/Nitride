# C# on Nitride, full error recovery, then incremental parsing

Status: draft for approval. Date: 2026-10-24.

## Intent

Nitride is heading toward powering the Nitrogen editor experience (the VS Code
and Rider extensions) without .NET. In an editor the document is broken most of
the time, so a parser that stops at the first error is not usable there.

The user wants "full blown" recovery: the recovery Nitra set out to deliver and
never fully reached. Nitrogen's current recovery is explicitly *not* the target;
the user calls it mediocre. Roslyn's recovery is the floor: on the same broken C#
input, Nitride must do at least as well. The proving ground is a real, large
grammar: the user's CSNitrogen C# grammar, CSharp4 profile.

The promise to an editor user: **an error damages only the code around it.**
The parser never stops. Everything outside a damaged region parses exactly as
in a valid file and, where the language has semantics, binds and type-checks as
if the error were not there.

## Milestones, in order

1. **A: CSharp4 on Nitride (valid files).** Admit everything CSNitrogen's
   CSharp4 composition needs and match Nitrogen's trees on valid,
   preprocessing-free C#.
2. **B: Error recovery.** Automatic, grammar-derived recovery producing a
   complete tree, measured against Roslyn on C# and with damaged corpora on
   every Nitride fixture.
3. **C: Incremental parsing.** Reparse after an edit by reusing unaffected
   work, built on the recovered trees.

Each milestone is reviewable and publishable on its own, like stages 1–11.

## Goals

### A. CSharp4 grammar admission

- Source: `~/work/CSNitrogen` (GitHub TovarishN/CSNitrogen),
  `src/CSNitrogen.Syntax/nitrogen.json`, language `CSharp4`: 30 modules, about
  610 lines, start rule `CSharp.CompilationUnits.CompilationUnit`.
- Gaps already found by probing Nitride's frontend:
  1. **Grammar functions**: `function Name(p: Grammar.Int | Grammar.Class |
     Grammar.Text, ...) : Grammar.Rule = ...`, evaluated when the grammar
     compiles. They use integer arithmetic, character classes and set algebra
     (`&`, `-`, ranges, computed `[(expr)]`), text, `let`, `if/then/else`,
     `choice x in a .. b { let ...; where cond : body }`,
     `seq c in word { ... }`, recursion, and built-ins such as `char`. Used by
     `Lexical.ngr` (identifier tables, `Contextual("word")` keywords).
  2. **UTF-16 surrogate pairs.** Generated tables (`GlobalTargets*.ngr` and the
     `Pairs` function) match supplementary characters as sequences like
     `['\ud834'] ['\udd73']`. Nitrogen sees UTF-16 code units; Nitride parses
     UTF-8, where those units never occur. Nitride must give these grammars the
     same meaning, so a high/low pair matches the supplementary code point.
  3. **`transparent` alternatives** (4 uses).
  4. **Infix/postfix forms whose operator is not a literal**: for example
     `Target:Expression Args:Arguments`, `Left:Expression MultiplyOp
     Right:Expression`. Nitride currently requires a literal operator.
  5. Unknown further gaps: the probe stopped at #4. Completing the inventory is
     the first task.
- Acceptance: on a corpus of valid, preprocessing-free C# (CSNitrogen's own test
  inputs plus generated programs), Nitride's syntax trees match Nitrogen's,
  normalized as earlier stages do, through a live .NET reference adapter.
  Nitride rejects what Nitrogen rejects. Native builds need no .NET.
- Scope: syntax only. CSNitrogen does not bind or type-check C#.

### B. Error recovery

- **Never stops.** Any input, however broken, yields one complete syntax tree
  covering every source byte, plus diagnostics.
- **Local damage.** Each error becomes a small, explicit error region in the
  tree: skipped source text and/or an assumed-missing element. Everything
  outside the regions has exactly the structure it would have in a valid file.
- **One diagnostic per local error**, at the real problem. No cascades.
- **Automatic.** Recovery is derived from the grammar alone. Optional grammar
  author hints (for example, resynchronization points) are allowed only where
  they measurably improve results, and a grammar with no hints must still
  recover well.
- **Semantics continue.** For typed Nitride profiles, binding, type checking
  and HIR run on recovered trees. Code outside damaged regions gets the same
  bindings, types and HIR as in the undamaged program. Anything depending on a
  damaged region gets an error/unknown type without further diagnostics. A
  declaration inside a damaged statement should stay visible later when its name
  can be recovered (builder's call on details).
- **Execution stays safe.** A program with diagnostics never runs host effects,
  as today.
- **Quality bar.**
  - Floor: on the same broken C#, Nitride's recovery is at least as good as
    Roslyn's under the metrics below.
  - Target: Nitra's design goal, the minimum-cost repair found automatically.
- **Measurement** (new, since no reference behavior exists to match):
  - Damaged corpora: take every valid program in the frozen and generated
    corpora (all Nitride fixtures and the C# corpus) and apply single automatic
    edits: delete a token, insert a stray token, duplicate or swap tokens,
    truncate mid-line, delete a closing bracket.
  - Metrics: the share of untouched top-level items (statements, declarations,
    members) whose syntax, and for typed profiles bindings/types/HIR, equal the
    original; diagnostics per edit (ideally 1); distance from the edit to the
    reported diagnostic; recovery time.
  - Roslyn comparison: run the same damaged C# through Roslyn
    (`CSharpSyntaxTree.ParseText`) and compute the same metrics from its tree
    (structure of untouched members, number of diagnostics). CSNitrogen already
    has Roslyn oracle tests that may be reused.
  - Results are recorded as evidence, like footprint and parity reports.

### C. Incremental parsing

- Given a previous tree (valid or recovered) and a text edit, produce the new
  tree by reusing unaffected work.
- **The result must be identical** to a full parse of the new text, including
  recovery decisions. This is checked by differential tests over random edit
  sequences.
- Must make keystroke-level reparsing of very large files practical.

## Non-goals

- C# preprocessing (`#if`, `#define`, regions, line mappings). C# input is
  preprocessing-free for now. CSNitrogen's preprocessing runs in C# code outside
  the grammar.
- C# semantics (binding, typing). CSNitrogen is a parser.
- C# profiles other than CSharp4 (later profiles may follow).
- LSP/editor integration itself (a later project that will consume this).
- Translating C# helper code in grammars.

## Anti-goals (failure even if it technically works)

- Any change to valid-file behavior: trees, diagnostics, HIR, values and
  Nitrogen parity for existing profiles must stay identical.
- Cascading diagnostics, or one edit producing many errors.
- Silently "fixing" invalid code: every repair must carry a diagnostic, and
  recovered programs never execute.
- Recovery whose result depends on anything but the input (non-determinism),
  or incremental results that differ from a full reparse.
- Superlinear blowups, stack overflows or hangs on adversarial input. Limits
  return structured errors, as today.
- C#-specific code in the engine. Grammar-specific behavior lives in grammars
  (or optional hints), never in Rust special cases.
- .NET or third-party dependencies in the native core.

## Constraints

- Scale: files as large as Roslyn handles, including multi-megabyte generated
  code. Parse and recovery time must stay close to linear in file size;
  Nitride's backtracking parser has no memoization today and will likely need
  it for this grammar.
- Latency: recovery should be "soon, not ASAP". Full recovery on large files may
  be somewhat slower than valid parsing; keystroke speed comes from milestone C.
- The user welcomes **every technique Nitrogen, Roslyn or Nitra use** to speed
  up parsing and everything else (memoization, token/node caching, red/green
  style trees, incremental blending, and so on).
- Existing project discipline: test-first, live parity against actual Nitrogen
  where a reference exists, recorded evidence and footprint, the bootstrap
  fixed point, CI on macOS/Linux/Windows, zero third-party core dependencies,
  ordinary Rust targets.
- Branching: stacked after the current unmerged branches (`codex/builtin-values`
  is PR #4; `codex/quote-rewrites` is committed locally, unpushed).

## Decided

- Order: C# admission, then recovery, then incremental.
- Profile: CSharp4. Preprocessing off.
- Recovery: automatic; hints only if they clearly help.
- Floor: Roslyn. Target: Nitra's goal. Not Nitrogen's current recovery.
- Valid-file behavior and Nitrogen parity are frozen.

## Calls made by the author of this document (not decided by the user)

- **APIs.** Today's strict `parse`/`compile` keep their exact behavior: they
  reject invalid input with the first diagnostic, so frozen observations for
  syntax-error cases are unchanged. Recovery is a new entry point that always
  returns a tree, all diagnostics and, for typed profiles, a program
  that may carry errors but is never executable.
- **Metrics are self-relative.** Each parser's damaged tree is compared with
  its own tree for the undamaged program: Nitride with Nitride, Roslyn with
  Roslyn. That keeps the comparison independent of tree-shape differences.
- **Grammar source.** CI cannot see `~/work`, so a pinned copy of the CSharp4
  grammar modules is vendored into Nitride with a provenance file recording the
  CSNitrogen commit and file hashes, like `grammar-provenance.json`. Updates are
  explicit.

## Left to the builder

- The recovery algorithm. Nitra's minimum-cost repair search over the parse
  (open source, rsdn/nitra) and Roslyn's techniques (skipped-token trivia,
  missing tokens, resynchronization) are the references to study. Error-node
  shapes, cost model and hint syntax, if any.
- Tree representation changes needed for recovery and incrementality (for
  example token/trivia retention, red/green split), as long as valid-file
  observations are unchanged.
- Memoization and other performance machinery.
- How the C# reference adapter hosts Nitrogen. CSNitrogen pins its own Nitrogen
  revision; which Nitrogen checkout the parity runs against is the builder's
  call, recorded in evidence.
- Exact damage operators, corpus sizes and how metrics are aggregated.
