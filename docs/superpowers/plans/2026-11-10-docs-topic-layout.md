# Organize remaining documentation by topic

**Goal:** Make docs/README.md the only tracked file at the documentation root.

User explicitly requested structuring the remaining root documents. Keep the
untracked personal cleanup note untouched and uncommitted. Use project/, guides/,
editor/, releases/, testing/, benchmarks/ and archive/ for current guides, release
notes, validation, measurements and initial history. Preserve all raw data bytes.

- [x] Map and move all 58 remaining tracked root files except README.md. Rebase local links throughout tracked Markdown, including frozen stage reports; update only their current hashes, never original hashes or raw data. Retain a migration map for legacy paths.
- [x] Update active code/script references and compiler-report defaults: read frozen inputs at the new paths and write new outputs under ignored target/reports/. Keep measured workloads and historical provenance strings unchanged.
- [x] Extend the integrity checker to accept a second manifest for the newly moved raw reports; leave active guides editable. Add a regression for a non-stage evidence manifest.
- [x] Validate raw bytes, all previously resolved links, layout, Python/Node syntax and tooling tests; request independent review and commit.

No history rewrite, artifact deletion, publication, benchmark rerun or runtime
engine change is part of this task. Existing legacy command/provenance strings
inside historical records remain as recorded; current guides and active scripts
use the new paths.

## Validation

- All 19 newly moved raw JSON reports match their pre-move Git blobs. Existing
  stage evidence verifies separately: 238 files, including 180 unchanged JSON
  reports. Only link destinations changed in frozen stage Markdown.
- All 482 previously resolved local link destinations survive the move; no
  broken local Markdown links were found. The tracked root contains README.md
  only. The unrelated untracked personal cleanup note stays untouched.
- Tooling tests: 49 passed. Shared-subtree driver tests: 9 passed. Python syntax,
  compiler-audit Node syntax, version consistency and diff whitespace checks pass.
- Runtime Rust tests and historical benchmarks were not rerun: this change moves
  documentation and routes report files without changing engine behavior or
  measurement workloads.

Independent read-only review found no critical or important issues. Fixed its
one minor finding: an old documentation pointer in the context-language example.
Historical performance, platform benchmark execution and manual editor acceptance
were outside this layout review. Commit follows the completed checks.
