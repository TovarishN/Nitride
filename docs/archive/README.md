# Historical records and path migration

These records retain the scope, revisions and limits of their original runs.
Use [current status](../project/status.md) and the [documentation index](../README.md)
for active guidance.

- [Initial milestone](initial/): original design, plan, progress and supporting evidence.
- [Experiments](experiments/): scanner planning and shared-subtree integration records.
- [Stages 2–45](stages/README.md): numbered plans, ledgers and measurement reports.

Dated benchmark reports live under [benchmarks](../benchmarks/), grouped into
compiler, JSON, scanners and editor/storage reports. They remain in the checkout;
this organization does not reduce clone size or rewrite Git history.

The [topic-layout manifest](layout-manifest.json) maps all 58 remaining legacy
root paths to their new locations. Its `files` entries freeze the original bytes
of the 19 moved JSON reports; current Markdown guides remain editable. The
[stage manifest](stages/manifest.json) separately tracks the earlier stage move.
Both manifests record the pre-move source commit, original path and original
SHA-256, so original files can be recovered with
`git show SOURCE_COMMIT:ORIGINAL_PATH`. Historical command lines and provenance
strings retain their original paths. Unversioned links should use the new paths;
links pinned to old commits or tags remain valid at those revisions.

New benchmark output defaults to ignored `target/reports/`. Verify preserved
reports without rerunning their workloads:

```sh
python3 scripts/check_evidence_archive.py
python3 scripts/check_evidence_archive.py --manifest docs/archive/layout-manifest.json
```
