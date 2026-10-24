# CSharp4 on Nitride (Milestone A)

Milestone A of the
[C# recovery and incremental design](superpowers/specs/2026-10-24-csharp-recovery-incremental-design.md):
Nitride prepares CSNitrogen's CSharp4 grammar and parses valid,
preprocessing-free C# into syntax trees identical to Nitrogen's. Plan:
[2026-10-24-csharp4-admission.md](superpowers/plans/2026-10-24-csharp4-admission.md).

## Vendored grammar

`languages/csharp4/Grammars` holds the 30 modules of CSNitrogen's `CSharp4`
language, byte-identical to TovarishN/CSNitrogen commit `c0413ce8`.
`provenance.json` records their SHA-256 hashes, and a test enforces them.
`languages/csharp4/corpus` vendors CSNitrogen's Mono test corpus with its
license files; `.gitattributes` keeps its original line endings.

The reference is the sibling Nitrogen checkout at `ed69ac8a`, which contains
CSNitrogen's pinned `a59ab220`. Its source generator builds the same 30 modules
into the reference tool, and the `oracle-syntax csharp4` profile reports
Nitrogen's trees.

## Frontend gaps closed

`scripts/csharp_inventory.py` stubbed and retried preparation until the full
composition prepared. It found six gaps; [stage12-inventory.md](stage12-inventory.md)
lists them with excerpts.

| Gap | Change |
|-----|--------|
| `transparent` alternatives | Grammar, decoder, exporter and codegen admit the marker (only when a name or element follows). The parser lets the single element node stand in; a literal-only transparent choice keeps a node that observers hide. |
| Infix/postfix operators that are rule references | Prepare, codegen and exporter no longer require a literal operator; the parser already handled them. |
| UTF-16 surrogate-pair classes | Class literals admit single surrogate code units. Prepared JSON keeps Nitrogen's UTF-16 ranges; codegen pairs a high-surrogate class followed by a low-only class into supplementary code points and removes unpairable units. |
| Grammar functions | `functions.rs` ports Nitrogen's registry, static checker (unused functions included), evaluator (memoization; depth 200, nodes 1,000,000, steps 10,000,000) and token-choice factoring. Calls expand before normalization; the exporter calls `GrammarFunctions.Expand` the same way. |
| Alternatives sharing an operator literal | Admitted; the parser ranks them by longest match, then precedence. |
| Prefix associativity | Accepted and ignored, as Nitrogen's generator ignores it. |

## Parser changes found by C# parity

- **Parser frame budget and stack guard.** `Limits::parser_frames` (default
  100,000) bounds parser recursion separately from `Limits::depth` (128, HIR
  and rewrite depth). The layered C# grammar needs several frames per nesting
  level; 128 frames rejected ordinary files such as `Boolean.cs`, and 512
  rejected a 1,000-branch `else if` chain. The parser also measures its own
  stack: past `parser_stack_bytes` (256 KiB) of the caller's stack it reruns on
  a scoped thread with `parser_spill_stack_bytes` (512 MiB, reserved, not
  committed), and past that budget it stops with a structured `limit` error.
  Deep input never overflows the caller's thread, even a 2 MiB one.
- **Size-relative parser work.** The parser may spend
  `max(work, parser_work_per_byte × input bytes)` (default 1024 per byte). A
  fixed 10M budget rejected C# files above about 40 KB, while the per-byte
  budget still stops superlinear backtracking.
- **Extension-point memoization.** Nested parentheses, casts, lambdas,
  conditionals, indexers and generic arguments were exponential: alternatives
  that begin alike (`(x)` and `(T)x`) each reparsed the nested operand. Results
  are now memoized by (point, position, minimum precedence) in two bounded
  arena generations (capacity `clamp(len/8, 4096, 2^20)`, subtrees up to 8,192
  nodes), so memory stays proportional to the tree. Codegen enables the memo
  only for points where two alternatives that parse nested expressions can
  begin with the same literal or character (8 of CSharp4's 28 points, and the
  `Calls` call point), so the other profiles keep stage eleven's exact parse
  metrics.
- **Leaf tokens.** Tokens are leaves by default, as in Nitrogen.
  `ParseOptions::token_children` keeps their insides for the grammar frontend,
  which validates literals inside opaque blocks.
- **Observer aliases.** The syntax observer hides plain syntax rules made of
  literals and references (Nitrogen's `EmitModel.IsAlias`), keeps extensible
  alternatives, and stops at token nodes.

## Validation

- RED first for each construct: transparent (missing field), reference
  operators and shared operators (codegen rejection), surrogates (unpaired
  error), grammar functions (7 tests failing on syntax), prefix associativity,
  C# parity (21 then 10 then 8 mismatches) and large-file limits. Two parser
  tests passed before any change: the core parser already handled reference
  and shared operators, and only the frontend rejected them.
- 147 Rust tests, formatting, strict clippy and `bootstrap -- --check` pass.
  Nesting tests parse parentheses, casts, lambdas, conditionals, indexers and
  generics 20 deep, a 1,000-branch `else if` chain, 100 nested blocks and 50
  nested classes under default limits, and deep input on a 2 MiB thread.
- C# parity: 238 frozen cases match actual Nitrogen trees. They comprise
  CSNitrogen's grammar, review, context and attribute cases (wrapped as its own
  oracle tests wrap them), the CSharp4 source-specific positive and negative
  sources, 29 Mono corpus files, and supplementary-plane identifiers (`𝑥` at
  UTF-8 span `[14,18]`, and `\U0001D465x`), plus nesting and postfix chains
  such as `f(x)(y)[z].w`. 191 are accepted and 47 rejected by both. Another 64 generated programs match live Nitrogen.
- `scripts/prepare_parity.py`: 107 prepared profiles match the live exporter,
  including the full CSharp4 composition, CSNitrogen's real `Lexical.ngr` and
  12 generated function grammars. 29 invalid grammars, covering function
  limits, type errors, unknown calls, `char` range and division by zero, are
  rejected by both.
- All earlier frozen and generated comparisons still pass (481 frozen, 576
  generated observations in total).
- Clean source-only copy (no tools/, language.json, `.cs` or `.Reference.ngr`)
  builds offline with no .NET on PATH. It verifies the bootstrap fixed point,
  prepares the 368-rule CSharp4 language and parses a Mono corpus file.

## Scaling and footprint

`docs/stage12-scaling.json` (best of 3 parse-phase times, macOS arm64):

| Input | Native parse | Nitrogen parse (warm, Release) | Native tree | Native parse peak |
|------:|-------------:|-------------------------------:|------------:|------------------:|
| 64 KiB | 0.10 s | 0.06 s | 2.9 MB | 4.1 MB |
| 1 MiB | 1.62 s | 0.22 s | 47 MB | 65 MB |
| 4 MiB | 6.42 s | 0.85 s | 189 MB | 243 MB |
| 16 MiB | 25.4 s | 3.62 s | 755 MB | 858 MB |

Native parsing is linear (time per byte at 4 MiB within 2× of 256 KiB; ignored
release test `csharp4_parse_scales_linearly`) but about 7× slower than Nitrogen's
generated parser (memoization added about 15%). The tree holds about 0.6 nodes
per source byte, and the parse peak stays within 1.4× of the tree. No speedup
or memory improvement over Nitrogen is claimed; Nitrogen's figure is allocated
bytes, not retained size. Likely optimizations (first-set dispatch, avoiding
winner subtree copies, narrower edges) are deferred.

The macOS arm64 release CLI is 1,535,488 bytes (+617,072 from stage eleven, almost
entirely the CSharp4 tables); the profiler is 987,872 bytes. Namespace,
inference, calls, functions, dates and rewrites phase metrics (allocations,
peak and retained bytes) are identical to stage eleven
(`stage12-*-memory.json`). The core still has no third-party dependencies.

## Limits

Preprocessing directives are out of scope. Only CSharp4 is admitted; later
profiles are untested. Diagnostics for rejected C# are generic syntax errors at
the furthest position; recovery is Milestone B. Nesting beyond the spill stack (about
tens of thousands of levels) stops with a structured `limit` error. Native parsing is about 7×
slower than Nitrogen. The Mono corpus is GPL/MIT-licensed test material,
vendored with its license files. Self-review only until the final branch
review.
