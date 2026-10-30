# Native releases

Cargo's `[workspace.package].version` is the source of truth. Every crate,
the VS Code extension and both lockfiles share stable SemVer
`MAJOR.MINOR.PATCH`. Until 1.0, minor releases may break compatibility;
patch releases contain compatible fixes. Version changes are explicit.

To prepare a release:

```sh
python3 scripts/version.py set 0.1.1
python3 scripts/version.py check
cargo test --workspace --locked
python3 -m unittest scripts/test_release.py
# Commit and push the reviewed changes, then tag that exact commit:
git tag -a v0.1.1 -m 'Nitride 0.1.1'
git push origin v0.1.1
```

Pushing a matching `vMAJOR.MINOR.PATCH` tag triggers Native release.
Use Actions → Native release → Run workflow on a branch for a complete
build rehearsal without publication. Pushing a `codex/release-*` branch also
runs the full matrix without publishing. A manual run on a tag publishes only
when its Publish input is selected. A version/tag/lockfile mismatch stops
the build before the matrix starts. The first release uses `v0.1.0`.

A tag can promote a successful rehearsal's artifacts when the commit matches
exactly and all three artifact bundles are still available. The workflow validates
the contents, checksums and clean source commit again before publication.
Otherwise it builds the complete matrix. Artifact promotion keeps the binaries
that passed verification and avoids repeating PGO training just to add a tag.

Each release includes three platform-specific VSIXs, three CLI archives
(`.zip` on Windows, `.tar.gz` elsewhere), per-platform optimization metadata,
per-platform SHA-256 manifests and a combined `SHA256SUMS.txt`. The workflow
checks the entire artifact set before creating a draft release, uploads all
assets and then publishes it. Existing published releases are never overwritten.
Failed uploads leave a draft for a rerun. Marketplace publication is separate;
install a downloaded VSIX using VS Code's “Install from VSIX” command.

| VSIX target | Rust target | Optimization |
| --- | --- | --- |
| win32-x64 | x86_64-pc-windows-msvc | PGO, full LTO, baseline + AVX2 |
| linux-x64 | x86_64-unknown-linux-gnu | PGO, full LTO, BOLT, baseline + AVX2 |
| darwin-arm64 | aarch64-apple-darwin | PGO, full LTO, Apple M1 CPU tuning |

All payloads use optimization level 3, one codegen unit and abort-on-panic.
The toolchain is pinned to Rust 1.99.1. Each target builds and trains on a
native runner. PGO generation and use keep the same CPU/LTO/target flags and use that compiler's `llvm-profdata`.
Empty/missing profiles fail the build. Every optimized payload is exercised
again before packaging.

The x64 `nitride` executable is a small baseline-compiled launcher.
It starts `nitride-avx2` only when AVX2, FMA, BMI1, BMI2, LZCNT and POPCNT
are detected, including OS support through Rust's feature detection;
otherwise it starts `nitride-baseline`. Keep these files beside the launcher.
This works for both the CLI and `nitride lsp` inside the extension.
Distributed code never uses `target-cpu=native`. ARM64 builds retain the
platform's baseline instruction set; macOS targets the first Apple Silicon
CPU generation so later Macs remain compatible.

PGO training covers preparation of the shipped language profiles, execution,
C# and XML parsing, warm parsing, malformed-input recovery and an actual LSP
session with initialization, edits, diagnostics, semantic tokens, symbols,
completion, navigation and formatting. Linux glibc builds additionally run
BOLT instrumentation, collect fresh profiles, optimize block/function layout
and validate the resulting binary. Symbols and relocations survive until
BOLT completes. BOLT output keeps its symbol table because subsequent stripping
can corrupt the ELF layout; other Unix payloads are stripped. macOS executables receive
an ad-hoc code signature after stripping. These are optimization settings,
not a claim of measured speedup; record benchmarks separately.

Linux x64 uses BOLT; Windows and macOS use LLVM PGO/LTO. Linux binaries are
built on Ubuntu 24.04 and require glibc 2.39 or newer. Windows and macOS
support is bounded by the pinned Rust toolchain and VS Code runtime requirements. The macOS
signature is ad-hoc, without Apple notarization or a Developer ID certificate.

## WASM exclusion

Release builds pass `--no-default-features -p nitride-cli --bin nitride`.
Neither `nitride-wasm` nor Wasmtime enters their dependency graph, and no
WebAssembly component or web-extension package is shipped. Component-backed
project execution/conformance and `binding draft` return an explicit
“built without component support” error. Process bindings and native language
features remain available. A project that mixes process and WASM bindings
also fails this preflight; use process-only bindings for native releases.
Ordinary development builds keep the default `components` feature (`wasm` remains a compatibility alias) and existing
component behavior. Release builds do not distribute `nitride-profile`.

## Local packaging

Install the pinned Rust toolchain with `llvm-tools-preview`, Node.js 22 and
Python 3.11+. On Linux glibc install LLVM's signed apt repository packages `bolt-23` and `libbolt-23-dev`. Run on the
actual target architecture and userspace:

```sh
scripts/package-vscode.sh darwin-arm64
```

The output is in `dist/`; profile data and intermediate binaries are in
`target/native-release/<platform>/`. `scripts/release/matrix.json` defines
the complete matrix. For a custom toolchain, `scripts/release/build.py`
accepts `--profdata PATH`, `--bolt PATH`, and `--merge-fdata PATH`.
The workflow installs its own tools and needs no publishing secret beyond
GitHub's built-in token with `contents: write` for the final job.
