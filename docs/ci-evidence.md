# Cross-platform checkout and deployment validation

The first GitHub run failed on all three runners because reference identity
hashes describe exact source/catalog text bytes, but checkout line endings were
not fixed. The local composition sources used CRLF while the committed blobs
used LF; Windows also converted other inputs to CRLF. The existing parity test
correctly detected both changes.

A fresh clone reproduced the failure before the repair. `.gitattributes` now
requires LF text checkouts on all platforms. The composition export was regenerated
with the live Nitrogen exporter. Provenance records retain the original upstream
hashes separately from the LF-normalized copies. Grammar content is unchanged;
hash validation was not weakened and source hashing still distinguishes bytes.

A fresh clone with `core.autocrlf=true` contains no CRLF tracked text files and
passes all 68 Rust tests. Local formatting and strict clippy pass. Live preparation
comparison passes for 70 valid profiles and 14 invalid grammars; 28 frozen stage-two
observations and 64 generated composition/XML programs still match live Nitrogen.

Repair commit: `2b8dc1a004e515be51a29708b05c50508f098073`.
[Initial failing run](https://github.com/TovarishN/Nitride/actions/runs/40068704001).
[Repair validation run](https://github.com/TovarishN/Nitride/actions/runs/40068966876).

## Confirmed runner results

The repair validation run completed successfully on 2026-10-24 for
`macos-latest`, `ubuntu-latest` and `windows-latest`. Each job ran formatting,
all 68 Rust tests, strict clippy, a release build, portable/Power/XML/namespace/
inference samples, and native preparation for all six configured profiles.
The workflow does not invoke the .NET reference tooling.

This establishes native build/test and sample execution on the three hosted
platforms. It does not establish native editor integration, full Nitrogen or
DateCalc compatibility, every CPU architecture, or comparative process-memory
measurements on Linux/Windows. Earlier stage reports retain their historical
platform and uncommitted-status statements; this report supersedes that status.

The independent repository is now private on GitHub at
[TovarishN/Nitride](https://github.com/TovarishN/Nitride), with `main` as its default
branch. Nitrogen product files remain untouched.
