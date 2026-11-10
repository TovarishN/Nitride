# Compact syntax trees

Nitride's CSharp4 tree was about 3× larger than Nitrogen's retained tree. This
stage measures Nitrogen's retained tree properly and slims Nitride's.

## Measuring Nitrogen

Nitrogen's `SyntaxTree` keeps preorder struct-of-arrays storage: a 2-byte kind
and 4-byte start, length and child-start per node, and a 4-byte positional slot
per child, with no labels. Small trees rent these arrays from `ArrayPool`, so a
warm parse reports almost no allocated bytes (656 bytes at 64 KiB);
`GC.GetAllocatedBytesForCurrentThread` is not a retained size.
`bench-syntax` now sums the arrays the tree holds (at their rented or exact
length) and cross-checks the sum with managed heap growth while the tree is
alive after full collections. The two agree within 0.1% from 1 MiB up.

## Changes

- **Alias rules build no node.** Codegen marks syntax-mode rules that Nitrogen
  treats as aliases (`EmitModel.IsAlias`: a reference, a labeled reference, a
  choice of them, or a plain rule of literals and references). The parser
  returns the alias's one child in its place, labeled by the reference to the
  alias, or no node when it matched only literals. An alias alternative that
  matched only literals keeps its node, since an extension point needs an
  operand (Nitrogen keeps an unreported literal leaf there). The grammar
  frontend opts back in with `ParseOptions::alias_nodes`, since it decodes by
  rule kind.
- **8-byte edges.** `Edge::label` is a `u16` index into the tree's own label
  table (`SyntaxTree::label(&edge)`), instead of a 16-byte `&'static str`.
  Labels are interned by address during the parse.
- **Exact-size vectors.** The node and edge vectors are shrunk to their length
  when the parse finishes, removing up to 2× growth slack.

## Results

`docs/stage13-scaling.json` (CSharp4, macOS arm64; Nitrogen warm, Release).
Nitride's tree includes its UTF-8 source copy; Nitrogen's adds its UTF-16 text.

| Input | Nitride nodes | Nitride tree (before → after) | Nitride parse peak (before → after) | Nitrogen nodes | Nitrogen tree + text |
|------:|--------------:|------------------------------:|------------------------------------:|---------------:|---------------------:|
| 64 KiB | 28,683 | 2.9 → 0.87 MB | 4.1 → 1.6 MB | 52,556 | 1.4 + 0.13 MB |
| 1 MiB | 443,463 | 47.2 → 13.5 MB | 64.6 → 26.6 MB | 812,540 | 15.2 + 2.1 MB |
| 4 MiB | 1,756,313 | 189 → 53.4 MB | 243 → 105 MB | 3,218,020 | 60.2 + 8.4 MB |
| 16 MiB | 6,902,313 | 755 → 210 MB | 858 → 327 MB | 12,646,820 | 237 + 33.6 MB |

Nitride's tree is now 3.6× smaller than before and about 22% smaller than
Nitrogen's tree with its text (210 MB against 270 MB at 16 MiB). Nitride keeps
about 30 bytes per node against Nitrogen's 19, but builds 45% fewer nodes
(tokens are leaves and literals are not nodes). Parse time is unchanged within
noise (27.0 s at 16 MiB against 25.4 s), still about 7× Nitrogen's.

Older profiles (`stage13-*-memory.json`, against `stage12-*-memory.json`):
the retained live memory entering HIR and execution drops by 25–60%, and parse
peaks drop by 25–40%. Each parse makes 7–9 more allocations (the label table
and the final shrink). The release CLI is 1,552,000 bytes (+16,512).

## Validation

- 148 Rust tests (new: `csharp_tree_holds_only_reported_nodes`, RED at 10,536
  tree nodes against 6,964 reported, then on capacity and edge size), strict
  clippy, formatting and the bootstrap fixed point pass.
- All live parity passes: 238 CSharp4 cases and 64 generated programs, every
  earlier frozen and generated comparison, 107 prepared profiles and 29
  invalid grammars, and the composition/export unit tests.

## Limits

`Edge::label` changed from `&'static str` to a `u16`; library code reads labels
through `SyntaxTree::label`. Typed profiles keep alias nodes (only syntax-mode
languages mark aliases). Node layout (20 bytes) is unchanged; Nitrogen-style
struct-of-arrays storage would save more but changes every tree consumer.
