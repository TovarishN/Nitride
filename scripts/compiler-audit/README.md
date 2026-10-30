# Compiler optimization audit

Run from the repository root:

```powershell
rustup component add llvm-tools-preview
node scripts/compiler-audit/run.mjs
node scripts/compiler-audit/summarize.mjs # Windows x86-64 report and flag verification.
```

This standalone Cargo workspace generates the real CSharp4 parser from its
project manifest, links only the parser and JSON reporting dependencies, and
uses the ordinary allocator. It does not load the CLI's WebAssembly runtime.
Its release profile matches the parser-relevant workspace settings: level 3
(Cargo's release default), full LTO, one codegen unit, and aborting panics.
Symbols are retained to help inspection.

The runner clears inherited Rust flag variables, records verbose build logs,
and compares portable, native CPU, portable PGO and native CPU plus PGO builds.
PGO requires the LLVM tools matching the active Rust toolchain. It trains on
separate 64 KiB valid/one-error/junk/dense inputs from the same generator family.
It measures five fresh processes for every case/build, rotating and reversing
build order after one unrecorded run. Timings exclude source reading, tree
export, fingerprinting, process startup and destruction. All emitted samples,
input and executable hashes, compiler identity and CPU are saved in
`docs/compiler-optimization-audit.json`. Build logs, generated inputs, profiles
and binaries stay under ignored `target/compiler-audit/`.

Every run fingerprints the full materialized tree, labeled edges, diagnostics
and recovery records outside the timed region. Any cross-build or repeat-run
mismatch makes the runner fail. This is a compiler-configuration equivalence
check, not an independent parser correctness oracle or exhaustive corpus test.
Logical work is reported but not used as a fingerprint: dense recovery already
has documented variable work counts.

The portable training profile is applied to both PGO variants. Training also
executes the harness's export and fingerprint validation. These measurements
are evidence for this harness and generated parser; production PGO should train
the actual deployed executable on its own representative workloads.

To inspect generated assembly after measuring (avoid compiling during timings):

```powershell
$env:RUSTFLAGS = '' # Use '-Ctarget-cpu=native' for the native build.
Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
cargo rustc --offline --release --manifest-path scripts/compiler-audit/Cargo.toml --target x86_64-pc-windows-msvc --target-dir target/compiler-audit/portable -- --emit=asm,link '-Cremark=loop-vectorize slp-vectorizer'
```

Pass the resulting `.s` file in the target's `release/deps` directory to
`node scripts/compiler-audit/inspect.mjs <file.s>`. The inspection script counts
static instruction sites within Windows `.seh_proc` blocks, including vector
register use, moves and selected packed comparison instructions. These are not
runtime samples or proof of vectorized token recognition. Inlined operations
are attributed to their enclosing function. Optimization remarks without debug
information may omit source locations.

Native CPU builds may require instruction sets absent from other machines.
The audit does not change production compilation defaults. PGO data is specific
to the source, compiler and training workloads; regenerate it after changes.

## ASCII run experiment

`scanner-runs.mjs` compares the frozen portable audit executable with scalar
and SIMD candidates built into separate directories:

```powershell
cargo build --offline --release --manifest-path scripts/compiler-audit/Cargo.toml --target x86_64-pc-windows-msvc --target-dir target/scanner-default
cargo build --offline --release --manifest-path scripts/compiler-audit/Cargo.toml --target x86_64-pc-windows-msvc --features nitride-core/ascii-runs --target-dir target/scanner-scalar
cargo build --offline --release --manifest-path scripts/compiler-audit/Cargo.toml --target x86_64-pc-windows-msvc --features nitride-core/simd-ascii-runs --target-dir target/scanner-simd
python3 scripts/compiler-audit/pin-windows.py
node scripts/compiler-audit/summarize-scanner-runs.mjs
```

The Python launcher pins only itself and its child benchmark processes to CPU 0.
Run after compilation/tests finish. The baseline executable must still match
the original audit's recorded hash. `NITRIDE_SCANNER_REPORT` overrides the
output path, which otherwise is `docs/ascii-scanner-runs.json`. Preserve each
completed report before rerunning.

The optional `nitride-core/ascii-runs` feature enables scalar batching of proven
repeated one-byte ASCII traces in generated lexical scanners. The optional
`nitride-core/simd-ascii-runs` Cargo feature includes scalar batching and adds
runtime AVX2 detection on x86-64; unsupported CPUs and other
architectures keep scalar scanning. It does not require a native CPU build or
change interpreted/incremental parsing into generated parsing.

## Apple Silicon reassessment

The [October 30 assessment](../../docs/compiler-assessment-2026-10-30.md)
compares fresh M4 Pro PGO/ASCII-batching runs with Nitrogen and Roslyn. The
explicit SIMD classifier is AVX2-only: on ARM64 the feature measures scalar
batching. A NEON classifier is a proposed follow-up, with no measured speedup.

Use a new output directory when changing source, compiler or training. The
runner accepts output/report paths so the Windows evidence stays intact:

```sh
NITRIDE_AUDIT_OUT=target/compiler-audit-macos \
NITRIDE_AUDIT_REPORT=docs/compiler-optimization-macos.json \
NITRIDE_LLVM_BIN=/opt/homebrew/opt/llvm/bin \
NITRIDE_AUDIT_ASCII=1 node scripts/compiler-audit/run.mjs
```

`NITRIDE_LLVM_BIN` is optional when Rust's bundled LLVM tools are installed.
Use tools compatible with rustc's LLVM profile format; this run used rustc
LLVM 23.1.3 and Homebrew `llvm-profdata` 23.1.4. `NITRIDE_AUDIT_ASCII=1` adds
feature-enabled and feature-enabled PGO builds, a separate feature-enabled
training profile, and the 1 MiB string case. It does not add NEON code.
`NITRIDE_AUDIT_REUSE_PROFILE=1` reuses the existing base training profile only;
use it solely when the source, toolchain and training inputs are unchanged.

Build the .NET harnesses before timing. The binary-reference harness reuses
`tools/Nitride.Reference/bin/Release/net10.0/` and records the runtime/reference
identities and DLL hashes. If those files are absent, first build
`tools/Nitride.Reference` against the intended Nitrogen checkout. Its generated
CSharp4 grammar must match the grammar being compared. The assessment records
the exact identities used here; checkout HEAD alone does not identify a DLL.

```sh
dotnet build -c Release docs/stage30-harness/rcmp \
  -p:UseArtifactsOutput=true -p:ArtifactsPath="$PWD/target/reassessment/dotnet" \
  -p:UseSharedCompilation=false -p:NuGetAudit=false
dotnet build -c Release scripts/compiler-audit/ncmp-binaries.csproj \
  -p:UseArtifactsOutput=true -p:ArtifactsPath="$PWD/target/reassessment/dotnet" \
  -p:UseSharedCompilation=false -p:NuGetAudit=false
python3 scripts/compiler-audit/compare-dotnet.py
```

Run the comparison after the native audit and all builds finish. It verifies
source hashes, measures five fresh .NET first parses and one warm process
(two warmups/five timed parses), and writes `docs/compiler-comparison-macos.json`.
Nitrogen language construction and process launch remain outside timing.
Damaged Nitrogen parses stop early and must not be ranked against whole-file
recovery. This run does not update memory or incremental-edit measurements.
