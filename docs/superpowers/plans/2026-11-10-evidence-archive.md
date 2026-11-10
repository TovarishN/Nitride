# Historical evidence archive

**Goal:** Reduce top-level documentation clutter while preserving recorded evidence and usable reproduction routes.

The user delegated ordering and requested continuing. Move the 238 tracked stage2–45 Markdown/JSON files to `docs/archive/stages/`; leave `docs/stage30-harness/` at its existing path because code/project references depend on it. Do not rewrite Git history, delete measurements, publish assets or touch unrelated local files.

- [x] Capture original hashes and path mapping. Rebase Markdown links according to old/new document locations; preserve all non-link report text. Move all raw JSON byte-for-byte. Retain source commit and both original/current hashes in an archive manifest.
- [x] Add a checker for the manifest and a small report-output helper with regression tests proving new measurements cannot overwrite archived reports by default. Route benchmark defaults to ignored `target/reports/`; update the shared-subtree baseline reader and recovery-compatibility driver without changing measurement logic.
- [x] Add archive navigation/migration guidance, update active documentation links and add the archive checker/report-output tests to CI. Version-pinned external links remain valid; unversioned HEAD links to moved files need the archive path. Historical command/provenance strings remain as recorded.
- [x] Validate all raw hashes, all non-link Markdown text, repository-local link resolution against the pre-move baseline, Python driver tests/compilation and CLI help. Request independent review and commit.

Baseline: 297 tracked top-level documentation files, of which 238 are stage records
(180 JSON / 108,344,469 bytes and 58 Markdown / 311,712 bytes). One unrelated
untracked cleanup document is excluded. This is an in-tree navigation cleanup;
repository/clone size does not shrink, and all raw evidence stays available.

Ruling: the complete release-tool Python suite exposed a temporary version-test
fixture that copied only crates/, omitting the embedded example introduced earlier.
The version script itself is unchanged. The fixture now follows Cargo's actual
workspace members and explicitly checks facade/example lockfile versions; its four
tests and the 48-test CI Python command pass. Four new report/archive tests and
nine existing measurement-driver tests also pass.

Final verification: 238 archived records are accounted for; all 180 raw reports
(108,344,469 bytes) are byte-identical. All historical Markdown non-link content
is unchanged. Actual current links retain all 431 originally resolved local
destinations, and new archive navigation resolves. Tracked top-level docs drop
from 297 to 59 (physical count 60 includes the untouched user cleanup document).
The ten workload ASTs match before/after outside imports, output routing and
optional report names. All 48 CI Python tests, four version-fixture tests and
nine measurement-driver tests pass; Python compilation, CLI help, archive/hash
checks, version consistency and diff checks pass. Independent review verified
original Git blobs and found no actionable issues. Rust/.NET benchmarks were not
rerun because runtime code and workloads are unchanged. No evidence deletion,
history rewrite, external publication or hosted acceptance is claimed.
