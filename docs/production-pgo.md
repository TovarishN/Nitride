# Production profile-guided builds

Run from the repository root with Node 22 and a working Rust release toolchain:

```sh
rustup component add llvm-tools-preview
node scripts/build-pgo.mjs
```

The script instruments the actual shipping `nitride` executable, explicitly
targets the host triple (keeping build scripts uninstrumented), trains its
bundled generated grammar parsers, merges profiles using the matching LLVM
tools, and rebuilds that same executable with `-Cprofile-use`. Full LTO and
the existing release settings remain in effect. It does not enable native CPU
instructions or optional ASCII/SIMD features.

The validated binary and provenance manifest are written to `target/pgo/dist/`.
The **Production PGO** workflow runs on relevant PRs, manual dispatch and `v*`
tags, and uploads these platform-specific PGO artifacts for Linux, macOS and
Windows. Ordinary `cargo build --release` remains an untrained portable build;
use this workflow's artifacts when deploying the PGO build. No external release
is automatically published.

Training uses all bundled sample languages, 32 frozen C# fixtures and larger
valid/malformed C# sources. `--pgo-parse <profile> <source>` performs generated
recovering parsing and an incremental trailing-space edit without executing
source programs or host bindings. Full node/edge trees, diagnostics and recovery
records must match between instrumented and optimized executables; logical
call counts are excluded because physical stack layout can change. The C#
cases must exercise generated syntax in both builds.

Set `NITRIDE_PGO_MANIFEST` to a JSON array of additional `{ "profile": "csharp4",
"file": "path/to/input.cs" }` entries to include consuming-app workloads.
Add representative real documents before deployment. The bundled corpus is a
reproducible starting mix, not a claim of optimal training for every language.
Training includes materialization, JSON reporting and incremental edits.

Profiles are regenerated for each invocation and never reused across compiler
or source changes. The manifest records compiler/target, Git revision, binary
and profile hashes, and every training input hash. `--offline` uses cached Cargo
dependencies. Downstream applications linking `nitride-core` must run their own
PGO process: profiles and flags do not propagate from a dependency's manifest.
The audit's 25–39% result is not a benchmark of these production artifacts.

Local Windows x86-64 validation on 2026-10-30 completed all 46 instrumented/PGO
parser and edit comparisons, followed by shipping CLI version, portable-language
execution and XML parsing smoke checks. The focused training test, strict CLI
all-target/all-feature Clippy and formatting checks passed. Binary/profile and
compiled-source hashes are recorded in [the validation record](production-pgo-validation.json).
