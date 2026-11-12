# A fast strict path for the shared parser

Status: for approval; open decisions settled. Date: 2026-11-12.

## Intent

Nitride's shared parser is correct, incremental and recovers from errors, but a
cold parse of valid C# 4 is still about 4–5× slower than Roslyn's warm parse.
Two rounds of contract-preserving work ([per-rule cost](../../benchmarks/parser/kept-path-2026-11-12.md),
[fact levels](../../benchmarks/parser/fact-levels-2026-11-12.md), in PR #58) took 16–19% off
valid parses. They have reached the floor of what the current core can do.

Measurements on the 4 MiB generated CSharp4 input (Apple M4 Pro, portable
release builds) locate the remaining cost:

| Step | Time |
|---|---:|
| Plain hand-written tokenizer, one pass | 10–11 ms |
| The grammar's compiled lexical scanners, one pass | 71 ms |
| Building the final 1.76M-node tree into flat arrays | 7 ms |
| Today's strict parse, generated syntax | ~665 ms |
| Today's strict parse, interpreted syntax with compiled lexers | ~870 ms |
| Today's strict parse, fully interpreted (the editor's path) | ~1,255 ms |
| Roslyn 5.0 warm parse (October 30 assessment) | ~150 ms |

Reading tokens and building the tree together cost about 18 ms. A prototype that
removed all node ownership tracking and rollback bookkeeping (an append-only node
store with final compaction) kept trees identical and saved only about 3%.
**Nearly all the time goes to deciding which rule applies:**

- entering rules that fail at their first token;
- two-token lookahead walks over alternatives (57M lead steps per 4 MiB parse);
- trivia replayed about 11 times per token;
- per-step logical-work, depth, stack and observation tracking;
- fact and memo bookkeeping kept for edits and backtracking.

This design adds a second engine that makes those decisions cheaply on valid
input and hands every other case to the current engine.

## The idea in one paragraph

Before parsing, analyze the grammar once into **decision tables**: for each
ordered choice and each Pratt point, which alternative or candidate can begin
with each lookahead token. A **fast strict engine** reads tokens from a
**token layer**, consults the tables, and builds nodes directly into a **flat
builder**. Where the tables cannot decide, it backtracks cheaply by truncating
the builder. If it meets anything it cannot reproduce exactly (a syntax error, a
limit, an ambiguity, an unsupported construct, or a backtracking budget spent),
it **bails out**, and the current engine parses the source from scratch, as it
does today. A successful fast parse is converted into an ordinary snapshot. Trees
and diagnostics are therefore identical by construction: either the fast engine
produced exactly the engine's tree on valid input, or the engine itself ran.

## Goals

- **Speed.** Cold strict parses of valid CSharp4 at least 2× faster than `main`
  at 4 MiB, in both the generated and the runtime-loaded configurations. Stretch:
  within 2× of Roslyn's warm parse.
- **Exact equivalence.** On every input the fast engine accepts, the published
  snapshot has the same tree: nodes, rules, spans, labeled edges, alias and
  transparent collapse, and labels. On every other input, the result is exactly
  today's.
- **Bounded downside.** Inputs that bail out cost at most 10% more than today,
  measured on the damaged corpus (one error, junk, dense errors, truncated).
- **Editor benefit.** The language server parses runtime-loaded languages
  (`LoadedLanguage`, `RuntimeSnapshot`), not generated code. The fast engine
  must run from tables built at language load, not only from generated code.
- **Every bundled language.** The analysis is grammar-generic. CSharp4 is the
  driving workload; JSON, XML and the fixture languages must pass the same
  equivalence checks.

## Non-goals

- Error recovery. Damaged input goes to the current engine unchanged.
- Faster edits. Incremental reparsing keeps the current engine; see "Edits".
- Changing grammar semantics, tree shape or the `.ngr` language.
- Replacing the current engine. It remains the reference, the recovery engine,
  the edit engine and the fallback.

## Anti-goals

These would make the project a failure even if it is fast:

- **Any tree difference** from today's engine on input the fast engine accepts.
- **Accepting input the engine rejects**, or rejecting input it accepts, including
  through limits (frames, stack, work budget) and ambiguity errors.
- **A second grammar semantics** that must be maintained by hand. Every decision
  must be derived from the prepared grammar by the analysis.
- **Making the editor experience worse**: a visibly slower first edit, slower
  recovery, or higher memory, without an explicit decision to accept it.

## Constraints

- `nitride-core` stays free of Cargo dependencies. Generated code stays the
  output of `nitride-codegen`. The 0.x generated-code boundary holds: the
  existing `CompiledParser::new` signature keeps working, so any generated
  addition is a new constructor.
- The current engine's contracts are unchanged on every path it serves:
  logical work equality between generated and interpreted parsing, observation
  ranges, facts, recovery and the cancellation polling cadence.
- Cancellation is honored within a bounded number of tokens; an obsolete parse
  stops without publishing.
- Limits are honored: a parse the engine would reject for depth, stack or work
  must not be accepted. The simplest correct rule is to bail out early: below
  a frame margin, under non-default limits, or on any limit doubt.
- Equivalence is checked with the existing fingerprint harness
  (`scripts/compare_parser_builds.py`) plus differential fuzzing (below).

## Architecture

### 1. Decision analysis (built once per language)

Input: the prepared `Language` (the same tables codegen and the runtime use).
Output: a `FastTables` value held by `LanguageTables`, built lazily like the
recovery tables, and shareable through the existing language-tables cache.

For each **ordered choice** (`A / B / …`) and each **Pratt point**, compute, per
lookahead *token class* (see 2), the set of alternatives or candidates that can
begin there. The current `Dispatch` already computes first-character sets,
two-token leads and Pratt plans; the analysis should start from that and extend
it to token classes and a second token where needed.

Classify each decision:

- **Determined (LL(1)/LL(2)):** for every lookahead, at most one alternative can
  begin. Choosing it directly is exactly ordered choice: the excluded ones would
  fail at their first token. If the chosen alternative fails, the choice fails,
  as it would after trying every alternative.
- **Ordered with backtracking:** several alternatives remain for some
  lookahead. The fast engine tries them in grammar order and truncates the
  builder between attempts.
- **Unsupported:** a construct the fast engine does not reproduce; it bails out
  on reaching it. The first version may mark predicates, alias-node options,
  token-children mode, holes and context instances unsupported, depending on what
  the census (milestone 0) shows matters.

**Pratt points** keep their exact semantics: among the matching candidates the
one with the furthest end wins, then the higher precedence, and an equal top
rank is an ambiguity error. With one possible candidate it is parsed directly.
With several, each is parsed into the builder, the best is kept and the others
are truncated. A tie bails out, so the engine reports the ambiguity itself. Owner
and minimum-power semantics (direct operand references) carry over unchanged.

**Repetitions** stay greedy: an iteration is attempted only when the
repetition's next element can begin with the lookahead. An iteration that fails
after starting truncates back to its start, as today.

**Predicates** (`&X`, `!X`) are evaluated by running `X` in a scratch mode that
builds nothing, or are unsupported in the first version, according to the census.

### 2. Token layer

The grammar is scannerless: literals are matched character by character, and
contextual keywords (`ContextualGet` and similar) are separate lexical rules over
identifier spellings. The token layer accelerates matching; it is not a separate
lexical grammar:

- **Trivia once.** One forward pass computes, for each token start, the trivia
  end after it, so trivia is skipped exactly once per token.
- **Token classes.** For each token start, a class used by the decision tables:
  the identifier-like word with its keyword identity (when it equals a grammar
  keyword literal), each lexical token rule that matches there, or the first
  punctuation byte. Classes are computed lazily or in a forward pass, whichever
  the milestone 1 measurements favor.
- **Character-exact matching.** Literals are still compared at the character
  position, so `>` followed by `>` keeps matching inside `>>` exactly as today.
  The class only selects candidates.
- **Fast scanners.** The compiled lexical scanners are 7× slower than a plain
  lexer on this input. An ASCII fast path for identifier, number and string rules,
  derived from the lexical grammar, benefits both engines and is a separate,
  independently measurable milestone.

### 3. Fast strict engine

A recursive-descent interpreter over the decision tables, with:

- no facts, memo, prior-node hints, observation ranges or per-step limit
  checks;
- a depth counter checked against a conservative margin below the frame limit,
  and a stack check per rule entry, bailing out instead of erroring;
- a backtracking budget (for example, proportional to the source length) after
  which it bails out, so pathological backtracking never runs unbounded without
  memoization;
- cancellation polled every N tokens;
- the same alias collapse, transparent stand-in, label and `except` semantics as
  the engine, implemented once in shared helpers;
- lexical rules matched by the compiled scanners when the language has them,
  and otherwise by a character-level interpreter of the lexical rule without the
  engine's bookkeeping. Runtime-loaded languages have no compiled scanners, so
  this interpreter is on the editor's path and must be measured in milestone 1.

Generated code may later specialize the interpreter per rule, as generated
syntax bodies do today. That is a later milestone; the table-driven engine comes
first because the editor needs it.

### 4. Flat builder and publication

Nodes are appended in post-order to flat arrays (rule, width, first child,
child count; child edges with label and extent). Rollback truncates. The 7 ms
floor above shows the builder is not the bottleneck. On success the flat tree is
converted into published arena chunks, one node per node, with no compaction
needed because nothing unreachable remains. Snapshot statistics report the
fast engine's own counters and say which engine produced the snapshot.

### 5. Strict and recovering parses

A strict parse runs the fast engine first. A recovering parse does too: on valid
input it publishes the snapshot the engine would publish for that input in
recovering mode, with no repairs or damaged regions. On bail-out the engine
runs as today: a strict attempt that seeds recovery, then recovery.

### 6. Fallback

On bail-out, the fast engine's partial result is dropped and the current engine
parses the whole source from scratch. The first version falls back for the whole
file. Resuming from a partial fast parse is out of scope until measurements show
the cost of a wasted fast attempt matters.

### 7. Edits

An edit needs recognition facts from the snapshot it edits. A fast snapshot has
none. The design keeps edits correct with the simplest rule and leaves a choice
for later:

- **Version 1 (decided):** the first edit to a fast snapshot first reparses the
  original text with the current engine, which records facts, then applies the
  edit as today. Later edits are unaffected. The language server can hide this cost by
  backfilling facts in the background after a cold fast parse.
- **Later, if needed:** derive facts directly from the fast parse. Success facts
  need each element's exact observation end, failure reach and relative depth.
  The failure reach must match the engine's exactly, because it feeds error
  positions in later failing parses. This is hard to reproduce, which is why it
  is not version 1.

## Verification

- **Fingerprint parity** with `scripts/compare_parser_builds.py` on the generated
  corpus and the Mono corpus, extended with a "which engine ran" column.
- **Differential fuzzing:** a target that parses grammar-valid sources with both
  engines, with the fast engine forced, and asserts identical trees or a
  bail-out. Valid inputs come from the existing generators, the deep-nesting
  stress files and mutations that keep sources valid.
- **Bail-out census:** on each corpus, how often and why the fast engine bails
  out, by reason. A rising bail-out rate is a regression even when timings
  improve.
- **Limits:** the existing limit tests run with the fast engine enabled, plus
  tests at frame limits near the margin and with non-default limits.
- **Every bundled language** passes parity with the fast engine forced on.

## Milestones

0. **Decision census (go/no-go).** Implement the analysis only. Report, for
   CSharp4 and every bundled language: decisions classified as determined,
   backtracking or unsupported, weighted by how often each is reached on the
   corpus (instrumenting the current engine). Go if determined decisions cover
   most dynamic decisions on CSharp4 and unsupported constructs are rare on valid
   input. Otherwise rework the analysis or stop here.
1. **Fast engine, CSharp4, runtime tables.** Token layer with trivia once and
   classes, interpreter, flat builder, publication, whole-file fallback and
   version-1 edits. Parity and bail-out census green. Target: at least 1.5× on
   4 MiB valid, interpreted configuration.
2. **Fast lexical scanners** for both engines, measured separately.
3. **Generated specialization** of the fast engine for compiled parsers. Target:
   the 2× goal in the generated configuration.
4. **Every bundled language** enabled, the language server enabled, background
   fact backfill.
5. **Optional: derived facts**, if the first-edit cost matters in practice.

Each milestone is reviewable and mergeable on its own, with the fast engine
behind an explicit opt-in until milestone 4.

## Risks

- **Coverage.** If CSharp4's hot decisions need backtracking or unsupported
  constructs, the speedup shrinks. Milestone 0 measures this before building.
- **Backtracking without memoization** can blow up on some grammars. The budget
  and bail-out bound the cost but turn it into a slower fallback.
- **Semantic drift.** Two engines can diverge in subtle cases: keyword boundaries,
  `except` lists, alias collapse with owners, Pratt ties, depth near limits. The
  shared helpers, differential fuzzing and the rule that unsure means bail out
  are the mitigation.
- **First-edit cost.** Version 1 makes the first edit to a large file cost about
  one engine cold parse unless facts are backfilled.

## Decided

- **Editor first** (the user's decision, 2026-11-12). The table-driven engine
  serving runtime-loaded languages comes first; generated specialization follows.
- **A one-time slow first edit is acceptable** (the user's decision,
  2026-11-12). Version 1 reparses with the current engine on the first edit to
  a fast snapshot; the language server may backfill facts in the background.
  Derived facts stay an optional later milestone.

## Author's calls

Made while drafting; each can be revisited:

- Whole-file fallback, not resumption.
- The fast engine is opt-in until it passes milestone 4.
- Unsure, unsupported or over-limit always means bail out, never an error.

## Left to the builder

Token-class encoding, table layout and compression, the backtracking budget, the
frame margin, cancellation interval, whether token classes are computed lazily
or in a pass, how the builder converts to arena chunks, statistics naming, and
the exact census thresholds for the milestone 0 go/no-go.
