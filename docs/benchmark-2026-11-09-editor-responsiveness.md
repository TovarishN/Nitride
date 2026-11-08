# Editor responsiveness baseline — November 9, 2026

The packaged extension passes automated extension-host acceptance in both a
Portable-only workspace and a disposable copy of all repository languages.
The next **steady-state large-document optimization** should avoid repeated
flat-tree materialization. Cold multi-language preparation and typed indexing
are separate costs, rather than evidence that one change will improve every edit.
No production engine or extension behavior changes in this milestone.

## Packaged extension

[Portable raw run](benchmarks/2026-11-09/editor-host-portable.json) and
[full-language raw run](benchmarks/2026-11-09/editor-host-repository.json) use
VS Code 1.145.2 on this macOS arm64 host. The extracted darwin-arm64 VSIX contains
the native PGO server, with no configured external server and no installed user
extensions. The driver verifies the extension path and bundled executable,
records their hashes, and creates/disposes its own workspace and user profile.
Repository sources and the user's installed extension/profile are not changed.

The artifact is the successfully packaged November 8 development build of
version 0.3.0, not a new published release. Its metadata identifies `1ec1aa3`
and `source_dirty: true`: the release-training Python fix was then uncommitted.
Its production Rust/extension sources match merged `1f2e8c0`; this branch also
leaves those production sources unchanged. The recorded hashes identify the
actual supplied VSIX and server; metadata is a build declaration, not an
independent proof of the artifact's origin.

Both runs verify initial source symbols, five edits with unique unresolved-name
markers, diagnostics in A while B continues editing, rapid edits settling to a
valid latest revision, and an unsaved grammar keyword change invalidating the
source followed by restoration. They do not accept an old marker as a current
result. The multi-language fixture initially exposed a harness error: routing
closes/reopens a plaintext document. Querying that obsolete incarnation could
cache an empty provider result. Protocol traces and a direct packaged-server
probe established that the server returned correct symbols; the harness now
waits for routing and reacquires the current documents before querying.

| Extension-host observation | Portable only | All repository languages |
| --- | ---: | ---: |
| Source open → initial ready features | 75.8 ms | 1,646.5 ms |
| Median edit → current diagnostic (five edits) | 64.5 ms | 43.8 ms |
| Unsaved Portable grammar change → dependent diagnostic | 43.4 ms | 63.2 ms |
| Grammar restore → ready source features | 65.7 ms | 65.9 ms |

These are single sessions, not comparative performance results. VS Code's
normal feature traffic can expedite analysis before the 150 ms quiet period;
20 ms polling adds observation granularity. The A/B check asserts A's diagnostic
has arrived at the end of continuous B edits; its roughly 0.7-second checkpoint
is **not** A's diagnostic latency. The extension was already activated before
entry into the test module, so `activation_wait_ms` is not cold-start latency.
Source-open timing includes routing and feature-provider readiness, not process
startup. No painted-frame latency or manual visual acceptance is claimed.
Public provider commands do not expose the language client's cancellation token;
19 native-only controlled LSP async tests separately cover the protocol boundary.

## In-process analysis

[Raw phase samples](benchmarks/2026-11-09/editor-baseline.json) come from a
native-only release build of the existing `benchmark` example on merged
`1f2e8c0`, without PGO or inherited Rust flags. The workspace release profile
uses LTO and one codegen unit. No production Rust modifications, concurrent
builds or tests were present during measurements. The report includes source
identity at runner start (dirty because new tooling/docs were present), the
binary hash, input hashes, toolchain and host. Build provenance is declared.

Each case uses three fresh processes, each with one warm edit and five measured
updates. Every update's problems, values, tree shape and edges are compared
with fresh analysis outside timing. The example exits on disagreement. Tables
show medians of process medians; phase medians need not sum to total medians.
Update always precedes fresh analysis, so cache/order effects remain possible.
C# is syntax-only; generated Portable has unique typed Scalar declarations and
references. Recovery inputs remove one semicolon near the middle.

| Input | Cached total | Parse/update | Materialize | Semantics | Editor |
| --- | ---: | ---: | ---: | ---: | ---: |
| C# 256 KiB valid | 2.22 ms | 0.37 ms | 1.60 ms | <0.01 ms | 0.14 ms |
| C# 256 KiB recovery | 4.79 ms | 2.83 ms | 1.67 ms | <0.01 ms | 0.14 ms |
| C# 4 MiB valid | 38.26 ms | 4.10 ms | 28.11 ms | <0.01 ms | 4.29 ms |
| C# 4 MiB recovery | 44.33 ms | 9.34 ms | 29.52 ms | <0.01 ms | 3.98 ms |
| Portable 16 KiB valid | 0.63 ms | 0.09 ms | 0.09 ms | 0.20 ms | 0.26 ms |
| Portable 16 KiB recovery | 0.80 ms | 0.34 ms | 0.10 ms | 0.20 ms | 0.16 ms |
| Portable 64 KiB valid | 3.21 ms | 0.32 ms | 0.32 ms | 0.72 ms | 1.83 ms |
| Portable 64 KiB recovery | 3.77 ms | 1.15 ms | 0.36 ms | 0.73 ms | 1.49 ms |

Cold full-registry preparation medians are 1.62–1.71 seconds across cases,
measured separately. This agrees with full-workspace source readiness above;
Portable grammar changes reuse unchanged prepared languages and do not repeat
that entire cold cost in these sessions. Fresh 4 MiB C# analysis is about
1.48 seconds valid and 6.07 seconds with the missing semicolon, distinct from
cached update times. In-flight work still cannot be interrupted.

## Next engineering direction

Materialization is about 73% of the valid 4 MiB cached update and 67% of the
recovery update. Investigate snapshot-native access **first for syntax-only
workspace documents**, retaining the established flat path as an oracle and
fallback. Preserve source spans, node order, child labels, recovery repairs,
semantic tokens, navigation, selection and folding. Measure a matched candidate
against this source before retaining a change. Typed documents still require
semantic compilation and have a different dominant editor-index cost; do not
force them through a speculative representation rewrite in the same change.

Cold preparation responsiveness and cooperative interruption remain separate
follow-ups. The cold preparation delay is observed; the measurements do not yet
choose between prepared-state caching, asynchronous preparation or finer reload
invalidation. No baseline result here establishes a speedup over a prior build.

## Reproduction and checks

```sh
cargo build --locked --release -p nitride-lsp --example benchmark --no-default-features
python3 scripts/benchmark_editor.py --binary target/release/examples/benchmark \
  --build-description 'native-only portable release, no PGO; record source and flags' \
  --output /tmp/editor-baseline.json
scripts/package-vscode.sh darwin-arm64
python3 scripts/editor_acceptance.py \
  --code '/Applications/Visual Studio Code.app/Contents/MacOS/Code' \
  --vsix dist/nitride-0.3.0-darwin-arm64.vsix --repository-languages \
  --output /tmp/editor-host.json
```

Omit `--repository-languages` for Portable alone. Use the platform's VS Code
application executable, a matching native VSIX and Python 3.11+ for repository
release tests. `--trace` captures language-client traces on failure for diagnosis;
leave it off for timing. The GUI host requires a desktop session; these runs
are local, not hosted GUI evidence. The Python harness tests run in CI.

Focused verification: 35 Python release/benchmark/harness tests, 18 extension
unit tests plus compilation, 19 controlled native-only async LSP tests, Node
syntax and patch-whitespace checks passed. No production Rust changes require
a new engine throughput or correctness claim. Review findings and hosted checks
are recorded in the PR.

Independent review found two Important harness issues: process cleanup on interruption
and stale successful reports after early setup failure. Both were reproduced and
fixed; the focused suite also checks a real child timeout/reaping path. No Critical
findings remained. Deferred minor: the existing benchmark edits the first digit
after the midpoint, which can rename a Portable binding and produce unresolved
references on alternating updates; the typed rows are not uniformly semantically
valid after editing. The initial `valid` case names describe the starting input.
The reviewer did not run the GUI, builds or benchmarks or verify historical build
provenance beyond the recorded metadata/hashes.
