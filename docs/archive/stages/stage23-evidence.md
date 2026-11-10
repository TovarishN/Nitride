# Stage23 label-cache experiment: rejected

A 32-slot cache for generated cold parses attempted to bypass repeated label interning HashMap lookups. Entries checked full static string pointer/length identity; the canonical map assigned IDs lazily. Empty label0, u16 overflow, logical work and read tracking were preserved on hits. The cache requested768bytes on this host, counted in the existing private budget, and dropped under tiny budgets. Default, edited-rope and recovery-probe parsers stayed uncached. There were no API/codegen/dependency changes.

| Valid C# target | Stage22 median | Cache median | Speedup |
|---|---:|---:|---:|
| 256 KiB | 0.122516 s | 0.119993 s | 1.0210x |
| 4096 KiB | 1.940727 s | 1.905334 s | 1.0186x |
| 16384 KiB | 8.001646 s | 7.957653 s | 1.0055x |

One three-round alternating fresh-process comparison per size, same generated valid C# and host, --native-syntax --snapshot, ordinary uninstrumented allocator. These are initial observations, not established repeatable gains.

At 16 MiB cache eviction events increased from 1,263,025 to 11,333,865. Logical work increased from 1,030,258,386 to 1,030,342,897. The additional cache charge changes memo-budget enforcement, rather than merely replacing label lookups. The marginal large-file timing gain does not justify this prototype; it was rejected without a repeat batch. This outcome does not rule out other generated label specialization designs.

Five focused cache/parser tests and five generated-syntax plus three profiler tests passed. Full candidate standard/adversarial/lifetime admission was intentionally not run after the performance gate failed. The archived prototype is not an admitted parser checkpoint.

Active source and rebuilt ordinary Release binary exactly match Stage22 (805c559269dcefc14ae704b1b6df6f1efdd4a8fe317fb7c432a3b7ac0f9f2aff / 5b384ec26ed6bc353258b77c0327f2c070d3a6e6b350a143b74e085a7a07676a). Fresh restored verification passed 310 workspace test executions, strict all-target/all-feature clippy, fmt, grammar bootstrap fixed point and nine Python driver tests. Stage22 full-corpus, recovery and reader-lifetime evidence remains applicable by exact source/binary identity; those complete captures were not rerun for this rejected candidate.

Only shared.rs and the new private labels.rs were owned by the experiment. The original was restored and the new module removed after archiving both. Source-overlay and archive hashes are recorded in the local checkpoint. Prior source files are unchanged by exact source identity. Experiment writes were confined to its own checkpoints, new evidence and README links; there was no pre-experiment manifest proving preservation of every historical artifact. Independent outcome/restoration review confirmed exact Stage22 source/binary and the raw outcome, with no active-source blocker. It found an archive-only budget edge: reclamation compares arena bytes alone while the final bound includes the surviving768B label cache. Repair that accounting and complete full admission before any reuse. Work remains local and uncommitted.

[Raw comparison and restoration audit](stage23-label-cache-experiment.json), [active Stage22 evidence](stage22-evidence.md).
