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
