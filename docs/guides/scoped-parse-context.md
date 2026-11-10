# Scoped parse context

Nitride admits Nitrogen's scoped parse context (Nitrogen PR 94), which
CSNitrogen's C# 5 grammars use to recognize `await` only in async bodies.

```ngr
syntax module ScopedWords
{
  context Loud;
  syntax Document = Blocks:Block*;
  syntax Block = LoudBlock / QuietBlock;
  syntax LoudBlock = "loud" "{" Words:with(Loud = true) Word* "}";
  syntax QuietBlock = "quiet" "{" Words:with(Loud = false) Word* "}";
  extensible syntax Word
  {
    | LoudWord = context(Loud) "bang"
    | QuietWord = !context(Loud) "bang"
  }
}
```

- `context Name;` declares a Boolean flag owned by the module. A language has
  at most 64 flags; names resolve like rules (qualified, local, or a single
  imported module's).
- Every parse starts with all flags false. `with(Flag = true|false) e` parses
  `e` with that flag replaced; callers' flags pass on to everything they call.
- `context(Flag)` succeeds without consuming input when the flag is true;
  `!context(Flag)` is the ordinary negative predicate over it.
- Scopes and tests build no node, label or field. A scope around a Pratt
  operand keeps the operand's precedence. A scope around an infix's left
  operand and context in tokens are rejected at preparation.

## Implementation

Context is specialized away when a prepared language is generated
(`nitride-codegen/src/context.rs`); the parsers have no context state.

- Each rule's dependencies are the flags it reads, transitively, before a scope
  of its own replaces them. An extensible alternative shares its point's.
- A rule is instantiated once per environment it is reached in, restricted to
  its dependencies. The all-false instance keeps the rule's own index, so start
  rules, quote categories, checks, helpers and patterns are unchanged. Other
  instances are appended clones with `origin` set to the rule they stand for.
- In each instance, tests become constants and fold: a failing alternative is
  dropped, a choice of one becomes that alternative, and a point offers only
  the alternatives whose tests can hold. Aliases are decided before folding, as
  Nitrogen does (a scope is seen through, a test is not a reference).
- Nodes record their origin: trees, snapshot views, recovery output and quote
  matching see only original rules. Memoization, facts and incremental reuse
  key on the rule actually called, so a result computed under one environment
  never answers a lookup under another, including across snapshot edits.

Grammars without context flags generate exactly as before.

## Cost

Instances multiply only rules whose dependencies are nonempty. CSNitrogen's
CSharp5 profile (two flags) prepares to 376 rules before specialization; see
`nitride prepare` and the generated table for the instance count of a profile.
