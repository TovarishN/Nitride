# Historical stage evidence

This archive contains the original stage 2–45 reports, plans and ledgers. They
record their own revisions, workloads and acceptance limits; they do not describe
the current feature set. Start with [current status](../../project/status.md) or the
[documentation index](../../README.md) for active guidance.

## Preservation and path migration

The [manifest](manifest.json) maps 238 original paths to their archive locations
and records original/current SHA-256 hashes. All 180 raw JSON files are unchanged
byte-for-byte (108,344,469 bytes). Markdown retains its recorded text and results;
only relative link destinations were rebased. The source commit in the manifest
identifies the pre-move checkout, not the measured engine revision. Measured
revisions and provenance strings inside reports remain authoritative.

The filename is unchanged: `docs/stage15-baseline.json` is now
`docs/archive/stages/stage15-baseline.json`. Historical command lines and raw
provenance strings are preserved as recorded, including old output paths; use
the manifest when locating their inputs in the current checkout.

Git history was not rewritten. Existing links pinned to older commits/tags still
resolve at those revisions. Unversioned links to `docs/stage...` at the current
branch need the new archive path. Original bytes can also be recovered locally
with `git show SOURCE_COMMIT:ORIGINAL_PATH` using the manifest. No duplicate
redirect files remain at the old paths, and no evidence was deleted or moved to
an external service.

The stage migration reduced tracked top-level `docs/` files from 297 to 59.
The subsequent [topic layout](../README.md) leaves only the documentation index
at the root. These moves do **not** reduce repository, clone or evidence size:
raw files remain in-tree. The
[comparison harness](../../stage30-harness/) stays at its original path because
source and project files depend on that layout.

## Reproduction output

Current benchmark drivers write new default reports to ignored `target/reports/`,
not into this frozen archive or the top-level documentation directory. Explicit
`--report` paths remain available in drivers that already supported them. The
shared-subtree driver's default baseline reads this archive. The recovery
compatibility driver still produces separately named stage-14 memory reports,
now through the shared output helper rather than rewriting script source.

Only report routing changed. Historical workload assumptions, reference-tool
requirements and dated assertions remain as recorded; the migration does not
claim those measurements were rerun on the current engine.

Verify preservation without running historical benchmarks:

```sh
python3 scripts/check_evidence_archive.py
python3 -m unittest scripts/test_report_paths.py
```

The integrity checker runs in CI. New measurements belong in generated output
or a separately reviewed evidence checkpoint, not overwrites of these files.

## Stage navigation

| Stage | Records | Primary report |
|---|---:|---|
| 2 | 3 | [stage2-evidence.md](stage2-evidence.md) |
| 3 | 3 | [stage3-evidence.md](stage3-evidence.md) |
| 4 | 2 | [stage4-evidence.md](stage4-evidence.md) |
| 5 | 3 | [stage5-evidence.md](stage5-evidence.md) |
| 6 | 2 | [stage6-evidence.md](stage6-evidence.md) |
| 7 | 4 | [stage7-evidence.md](stage7-evidence.md) |
| 8 | 5 | [stage8-evidence.md](stage8-evidence.md) |
| 9 | 6 | [stage9-evidence.md](stage9-evidence.md) |
| 10 | 7 | [stage10-evidence.md](stage10-evidence.md) |
| 11 | 8 | [stage11-evidence.md](stage11-evidence.md) |
| 12 | 9 | [stage12-evidence.md](stage12-evidence.md) |
| 13 | 8 | [stage13-evidence.md](stage13-evidence.md) |
| 14 | 13 | [stage14-evidence.md](stage14-evidence.md) |
| 15 | 22 | [stage15-evidence.md](stage15-evidence.md) |
| 16 | 7 | [stage16-evidence.md](stage16-evidence.md) |
| 17 | 8 | [stage17-evidence.md](stage17-evidence.md) |
| 18 | 8 | [stage18-evidence.md](stage18-evidence.md) |
| 19 | 11 | [stage19-evidence.md](stage19-evidence.md) |
| 20 | 2 | [stage20-evidence.md](stage20-evidence.md) |
| 21 | 7 | [stage21-evidence.md](stage21-evidence.md) |
| 22 | 8 | [stage22-evidence.md](stage22-evidence.md) |
| 23 | 2 | [stage23-evidence.md](stage23-evidence.md) |
| 24 | 7 | [stage24-evidence.md](stage24-evidence.md) |
| 25 | 5 | [stage25-evidence.md](stage25-evidence.md) |
| 26 | 4 | [stage26-evidence.md](stage26-evidence.md) |
| 27 | 4 | [stage27-evidence.md](stage27-evidence.md) |
| 28 | 4 | [stage28-evidence.md](stage28-evidence.md) |
| 29 | 4 | [stage29-evidence.md](stage29-evidence.md) |
| 30 | 2 | [stage30-comparison.md](stage30-comparison.md) |
| 31 | 4 | [stage31-evidence.md](stage31-evidence.md) |
| 32 | 4 | [stage32-evidence.md](stage32-evidence.md) |
| 33 | 4 | [stage33-evidence.md](stage33-evidence.md) |
| 34 | 5 | [stage34-evidence.md](stage34-evidence.md) |
| 35 | 4 | [stage35-evidence.md](stage35-evidence.md) |
| 36 | 4 | [stage36-evidence.md](stage36-evidence.md) |
| 37 | 4 | [stage37-evidence.md](stage37-evidence.md) |
| 38 | 4 | [stage38-evidence.md](stage38-evidence.md) |
| 39 | 4 | [stage39-evidence.md](stage39-evidence.md) |
| 40 | 4 | [stage40-evidence.md](stage40-evidence.md) |
| 41 | 4 | [stage41-evidence.md](stage41-evidence.md) |
| 42 | 4 | [stage42-evidence.md](stage42-evidence.md) |
| 43 | 4 | [stage43-evidence.md](stage43-evidence.md) |
| 44 | 4 | [stage44-evidence.md](stage44-evidence.md) |
| 45 | 3 | [stage45-comparison.md](stage45-comparison.md) |
