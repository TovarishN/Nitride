# Packaged editor acceptance — 0.4.0 candidate

Validated November 9, 2026 on macOS arm64 with VS Code 1.145.2. The final
Portable-only and repository-language sessions passed in disposable user profiles,
extension directories and workspaces. No user extension installation or workspace
was changed. No engine regression was observed in these local acceptance runs.

## Candidate identity

The actual PGO VSIX and CLI archive were built from clean commit
`6c9066759540f358e6fc000ccf56e9c823e90c7c`, after merged PR #50 and the 0.4.0
version synchronization. Package metadata says `source_dirty: false`, native
Apple M1 tuning, PGO enabled and no component runtime. The later harness correction
changes only the external test driver; it does not ship in the extension or
alter the tested server. Both final reports record the same current harness
hashes and package/server hashes. A final-head three-platform rehearsal is tracked
in the release PR separately; publication requires its own exact commit gate.

VSIX SHA-256: `b60153410cab4eea1e67c1616edfdbd1529e7d79b848ec67d075d73179f3aaad`.

Bundled server SHA-256: `1e065ae81195333031d1d372370f1d8d96920b08af0fc04b57520f588dc7f8b3`.

## Workflows

Both sessions verify packaged routing and initial features; five unique unresolved
name edits; actual editor `type` commands; malformed typed-source diagnostics and
restoration; diagnostics in A while B keeps changing; rapid obsolete edits settling
to unique valid symbols; and unsaved grammar keyword changes followed by restoration.
The all-language run additionally opens a generated >=4 MiB C# source, removes and
restores a semicolon with ranged edits, makes eight rapid edits and then inserts
a unique leading comment fold absent from the previous revision. The final fold
is observed at lines 1–3 with current valid diagnostics. C# remains syntax-only.

| Observation | Portable only | All repository languages |
|---|---:|---:|
| Host launch → ready source features (runner included) | 1508.0 ms | 3254.0 ms |
| Source open → ready features | 110.7 ms | 1665.0 ms |
| Median edit → current diagnostic (five edits) | 47.7 ms | 43.7 ms |
| Editor type → current diagnostic | 55.0 ms | 101.2 ms |
| Grammar change → dependent diagnostic | 41.7 ms | 65.5 ms |
| Grammar restore → ready source | 171.5 ms | 206.0 ms |

| 4 MiB document observation | Time |
|---|---:|
| Open → current features | 1560.7 ms |
| Missing semicolon → recovery diagnostic | 465.1 ms |
| Restore → current features | 395.2 ms |
| Final rapid edit → new current fold | 235.2 ms |

These are single-session extension-host API observations with 20 ms diagnostic
polling granularity, not comparative benchmarks. Launch timing uses a shared host
wall clock and includes VS Code/extension-test startup. Activation had already
occurred before the test module in these runs. Provider queries can expedite
analysis before the normal debounce. Cross-document checkpoint duration is not
A's diagnostic latency. Folding requests include feature processing, transport
and VS Code's result handling, beyond the core analysis phase.

## Harness correction

The first all-language run timed out because it demanded an end-of-file fold.
VS Code returned exactly 5,000 folding ranges with no diagnostics for the large
source. Inspection of the installed 1.145.2 workbench confirmed that the provider
command applies `editor.foldingMaximumRegions`, whose default is 5,000. The fixed
harness leaves that setting alone and places the freshness probe near the start.
It asserts the probe is absent before editing, then requires the new leading fold.
The failing attempt and the earlier successful Portable run are retained alongside
the two final common-harness runs. No server behavior was changed to satisfy this.

## Validation and boundaries

- Clean native PGO build, training, final smoke validation and VSIX/archive verification passed.
- Extracted CLI archive reports 0.4.0 and passes Calendar process execution/conformance.
- Full workspace: 767 passed, three ignored; native-only LSP: 228 passed; native CLI/runtime: six passed.
- Required Python tooling: 42 passed; extension compilation and 18 unit tests passed; strict workspace Clippy, Node syntax, version/tag synchronization and patch checks passed.
- Independent whole-change review found no actionable issues; no builds/tests/desktop actions were delegated.

Manual visual appearance, IME, accessibility, painted-frame latency and interactive
editor acceptance on Windows/Linux remain unverified. The three-platform release
rehearsal covers native training and packaging, not those GUI claims. Large-file
feature observations are still hundreds of milliseconds, and initial multi-language
readiness takes around 1.7 seconds after source open here. These are explicit
follow-up areas if they affect actual workloads; no universal latency deadline
or complete elimination of LSP costs is claimed.

## Evidence and reproduction

Final raw reports: [Portable](benchmarks/2026-11-09/editor-host-040-portable.json),
[all languages and large source](benchmarks/2026-11-09/editor-host-040-repository.json),
[native archive Calendar](benchmarks/2026-11-09/release-040-native-calendar.json).
Retained [failed full attempt](benchmarks/2026-11-09/editor-host-040-repository-attempt1.json)
and [earlier Portable run](benchmarks/2026-11-09/editor-host-040-portable-attempt1.json)
identify their original harness hashes.

```sh
scripts/package-vscode.sh darwin-arm64
python3 scripts/editor_acceptance.py --code '/Applications/Visual Studio Code.app/Contents/MacOS/Code' --vsix dist/nitride-0.4.0-darwin-arm64.vsix --output /tmp/editor-040-portable.json
python3 scripts/editor_acceptance.py --code '/Applications/Visual Studio Code.app/Contents/MacOS/Code' --vsix dist/nitride-0.4.0-darwin-arm64.vsix --repository-languages --large-document --output /tmp/editor-040-full.json
```

Use a clean candidate commit for trustworthy native provenance and avoid concurrent
builds/tests when collecting timing observations. The harness removes its own
profiles/workspaces and reaps its own process tree on interruption/timeout.

## Publication follow-through

The exact rehearsal commit `4dcc669aa5ed0ed5985ce9221bbf09e3fd147898` was tagged
and [published as v0.4.0](https://github.com/TovarishN/Nitride/releases/tag/v0.4.0).
All three native PGO jobs passed (Linux also uses BOLT), and all published files
match the verified rehearsal files byte-for-byte.

The CI-built macOS VSIX was additionally exercised before promotion in
[Portable](benchmarks/2026-11-09/editor-host-040-rehearsal-portable.json) and
[full-language/large-file](benchmarks/2026-11-09/editor-host-040-rehearsal-repository.json)
sessions using the same final harness. Both pass, recording the exact rehearsal
commit and clean provenance. Diagnostic medians are 45.2/42.6 ms; large-file edit
checks are 246–476 ms. Its extracted CLI also passes [Calendar process checks](benchmarks/2026-11-09/release-040-rehearsal-calendar.json).
These reports identify the rehearsal artifact that was then promoted unchanged;
they are not a claim that GUI timing was rerun after publication.

The [published-file verification](benchmarks/2026-11-09/release-040-published.json)
records all 13 hashes. Manual visual/IME/accessibility and Windows/Linux GUI
acceptance boundaries above remain unchanged.
