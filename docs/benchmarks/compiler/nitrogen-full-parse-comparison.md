# Current Nitride versus Nitrogen: full C# parse speed

Measured on the same host using byte-identical generated valid C# inputs from the Roslyn comparison:256KiB,4MiB and16MiB targets. Current Nitride is the admitted Stage22 generated shared parser; the label-cache experiment was rejected and fully removed. Nitrogen Runtime/Generator is freshly built Release from current source, version0.19.0, commit`ed69ac8aaa3bbc7c8d44c97cf969d1b6ddd89ea2`, running.NET10.0.0 Arm64. Both consume the same thirty checked-in CSharp4 grammar files.

## Fresh-process first full parse

Median of three fresh processes per backend and size. Run order rotates through the three backends, so each occupies every position once. JIT/empty-pool costs can appear in Nitrogen first-parse timing. Process startup, input I/O and .NET language construction are outside the timer.

| Input target | Nitride shared/generated | Nitrogen one-shot | Nitrogen incremental initial |
|---|---:|---:|---:|
| 256 KiB | 0.122816 s | 0.341835 s | 0.373142 s |
| 4096 KiB | 1.978496 s | 1.368329 s | 1.592254 s |
| 16384 KiB | 8.057632 s | 4.345059 s | 5.069167 s |

At16MiB Nitrogen one-shot is 1.85x faster than Nitride; Nitrogen initial incremental parsing is 1.59x faster. At256KiB Nitride wins the first-parse comparison; the JIT/initial-pool effect matters at that size.

## Warmed Nitrogen full-parse control

Separate process per size and mode: three complete warmup parses, then five measured complete parses. A new full parse is performed every time; these are not incremental edits. Results are disposed outside the timing region, allowing normal buffer-pool reuse. Nitride has only the fresh-process measurement here, so this is not a warmed-versus-warmed comparison.

| Input target | Nitrogen one-shot warm median | Nitrogen incremental initial warm median |
|---|---:|---:|
| 256 KiB | 0.074613 s | 0.092333 s |
| 4096 KiB | 0.802901 s | 0.892688 s |
| 16384 KiB | 3.357566 s | 3.720324 s |

## What is included

- Nitride: `--native-syntax --snapshot`, ordinary allocator without allocation instrumentation. The phase includes its SourceText copy, dispatch preparation, shared syntax construction and retained dependency index. It performs no implicit flat-tree export or semantic indexing.
- Nitrogen one-shot: `Language.Parse(source, start)` through the completed ParseResult and flat syntax tree; no incremental reuse table.
- Nitrogen initial incremental: `Language.ParseIncremental(source, start)`, including its reuse table; this still constructs a fresh full tree.
- Nitrogen language/module construction is measured separately in each raw result but excluded from parse time, as language preparation normally precedes repeated file parsing.
- All engines exclude file I/O, process launch, post-parse validation, serialization and final result disposal from the reported parse phase. Neither performs semantic compilation. No forced GC or concurrent builds/tests occurred during captures.
- All measured inputs were accepted without diagnostics. Node counts agree across Nitrogen one-shot/incremental and cold/warm runs. Both modes reject a negative control with an invalid suffix, guarding against prefix-only success.

Syntax representations and retained metadata differ. These are API-level full-parse costs on this generated corpus, not a claim of identical tree layouts, equivalent incremental-edit behavior or whole-compiler performance. Roslyn was not rerun here; its earlier timings should remain labeled historical.

## Reproducibility

The raw report embeds the controller and harness sources, grammar/source/executable hashes, runtime identity, individual timings and validation counts:27 cold measurements,30 warm measurements and18 warmups. Current Nitride source/binary exactly match Stage22 throughout. Nitrogen Runtime/Generator/Grammar source fingerprints, Git revision and Git status match their pre-build snapshots. Build outputs were redirected to`/tmp/nitride-nitrogen-full-parse/artifacts`; frozen benchmark binaries live in this worktree checkpoint. No Nitrogen source changes or performance optimizations were made.

[Raw measurements and exact protocol](nitrogen-full-parse-comparison.json).
