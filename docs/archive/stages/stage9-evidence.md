# Self-hosted admitted grammar parser

The .ngr frontend now uses nitride-core's generated parser, driven by
`languages/grammar/Grammar.ngr`. The previous handwritten lexer and recursive
descent parser are removed. Rust decodes the resulting syntax tree into the
existing preparation model, including UTF-16-compatible literal contents, then
performs existing module resolution and profile admission.

## Bootstrap chain

A checked-in `crates/nitride-prepare/src/grammar_seed.rs` is compiled directly.
Cargo does not run preparation to build preparation. No pre-exported JSON,
Nitrogen executable or .NET component is required. The first seed was generated
with the preceding native frontend; its normalized grammar was independently
compared with the actual Nitrogen exporter before the switch.

Run:

```
cargo run -p nitride-prepare --example bootstrap
cargo run -p nitride-prepare --example bootstrap -- --check
```

The first command prepares Grammar.ngr using the current generated frontend and
emits Rust tables. The second rebuilds with those tables, prepares it again and
requires identical bytes. A workspace test performs the same equality check;
source/catalog identity is part of the emitted seed, so even stale comment-only
source changes are detected. CI runs the check on macOS, Linux and Windows.
The seed is generated code intentionally excluded from rustfmt traversal.

This is self-hosting of the admitted grammar syntax, not a complete Rust rewrite
of Nitrogen's full Nitrogen.ngr. Literal/model conversion, admission and Rust code
emission remain Rust code. The Rust compiler is still the bootstrap toolchain.
Unsupported language and semantic constructs remain explicit errors.

## Boundaries and regressions

Generated recognition retains touching-call rejection (`Name(X)`), grouped
references (`Name (X)`), reserved expression words, literal range punctuation,
separated literal-delimiter lists, templates and opaque syntax-mode blocks.
Quoted/commented braces are balanced; raw/verbatim C# strings are still rejected.
Opaque character regions are scanned without asserting C# validity. Grammar
character-class literals still require one BMP UTF-16 unit. Surrogate pairs in
strings are combined; unpaired surrogates fail, including opaque regions.

The core's existing default accepts trailing unterminated block comments to match
frozen Nitrogen application-language behavior. Grammar-source parsing must reject
them, as its old frontend did. `parse_with_options` adds an explicit strict-comment
policy; ordinary `parse` keeps its previous default. No global trivia behavior was
changed. Grammar errors retain UTF-8 file/line/column/spans; generated syntax
recognition uses a generic error rather than the old expected-token messages.

Recognition uses core Limits (512 parser-frame depth, 10,000,000 work). These are
parser frames, not 128 author-written parentheses: generated wrappers consume
frames too. Twelve nested groups are tested as valid. Deep groups, opaque nesting and postfix chains return structured
errors. A postorder preflight bounds the combined syntax/model nesting at 128 before
recursive JSON construction, separated-list cloning or normalization. Each
postfix contributes to that combined budget. The preflight scratch is dropped
before model construction. Preparation temporarily owns a syntax tree in addition
to its normalized model; no reduction in preparation memory is claimed.

## Validation

- 99 Rust tests, formatting and strict clippy pass; locked release builds pass.
- All 189 frozen observations and 384 generated live programs still match actual
  Nitrogen, including bindings, nominal types, HIR, origins, values and effects.
- 15 development-exporter tests pass. 77 valid preparation profiles, including
  Grammar.ngr itself, match live Nitrogen export; both reject 16 invalid grammars.
- Twelve new tests cover bootstrap equality, strict trailing comments, range
  punctuation, lowering-keyword fallback, name adjacency/reserved words, opaque
  blocks and character scanning, UTF-16 decoding ordinary nested groups and deep malformed input.

The macOS arm64 release CLI is 818,400 bytes (799.2 KiB), +17,296 bytes from
stage eight. The embedding/profiling executable remains 519,600 bytes. The core
still has no third-party dependencies. Four `stage9-*-memory.json` reports retain
raw 10/100/1000-pair measurements; all allocation counts, retained bytes and peaks
match stage eight. Elapsed times vary and are not a throughput improvement claim.
Metrics measure requested System bytes per phase, excluding caller input, static
tables, allocator overhead and RSS. Static parser tables enter the CLI size.

A clean source-only copy excludes tools/, every language.json, C# helper and
.Reference.ngr. With .NET environment entries removed and PATH restricted to
system tools, absolute Cargo/Rust executables build it offline. The rebuilt
bootstrap example verifies its fixed point; grammar and function preparation
succeed, and the function sample still yields [-6,6]. The host retains .NET
elsewhere; it was not physically uninstalled.

Root nitrogen.json registers the grammar-language fixture and its real start
rule. Nitrogen's workspace path loads it: the valid .nitridegrammar sample passes;
a temporary malformed grammar reports a source-located syntax error and is
removed. Live VS Code/Rider interaction remains unverified.


## Independent review and fixes

One fresh reviewer compared 92 additional expression forms and found two issues.
A reserved-word label midway through a sequence bypassed the former continuation
boundary. A regression reproduced the acceptance; the generated grammar now guards
sequence tails with !Reserved while preserving valid first-position labels.

Composing twelve groups each containing128 postfix operators overflowed a normal
2 MiB stack during model construction/normalization, while the preceding frontend
returned a nesting error. A subprocess regression reproduced the abort. Bottom-up
total-depth preflight now rejects the input before building recursive JSON;
the same regression passes on an explicitly sized 2 MiB worker stack. Full regression
and bootstrap equality checks pass after both fixes. No minor findings are deferred.
