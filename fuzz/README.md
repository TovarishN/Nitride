# Incremental parser fuzzing

The `incremental_edits` target compares parse → edit → reparse with a full parse
through the independent flat parser. It checks source text, root, rules, spans,
labelled children, diagnostics, recovery repairs, coverage and damaged nodes.
Two retained snapshot readers are rechecked after subsequent successful and failed
edits. Strict failures must agree with the flat parser's diagnostic. There are no
host calls and no fuzzing dependencies in `nitride-core`.

The sixteen committed seeds cover Portable, strict JSON, XML and CSharp4, each
with strict/recovering modes and interpreted/generated initial snapshots. Source
and insertions can contain Unicode or malformed UTF-8 (decoded lossily); edit
endpoints are valid character boundaries. This target does not exercise invalid
UTF-8 edit ranges, arbitrary grammar generation, semantic execution or cancellation.
Existing focused suites cover several of those contracts separately.

## Ordinary CI replay

From the repository root, using stable Rust:

```sh
cargo test --locked -p nitride-cli --no-default-features --test fuzz_smoke
```

This reuses the exact target oracle, verifies that all sixteen configurations
are present, and requires each seed to attempt multiple edits. Workspace tests
also run this integration test. It needs neither nightly nor cargo-fuzz.

## Coverage-guided campaign

Follow the [Rust Fuzz Book setup](https://rust-fuzz.github.io/book/cargo-fuzz/setup.html)
to install nightly and cargo-fuzz. Keep the ordinary default toolchain unchanged:

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
mkdir -p /tmp/nitride-fuzz-corpus
cp fuzz/corpus/incremental_edits/* /tmp/nitride-fuzz-corpus/
cargo +nightly fuzz run incremental_edits /tmp/nitride-fuzz-corpus -- \
  -max_total_time=60 -max_len=1024 -timeout=10 -rss_limit_mb=2048 -seed=20261110
```

The temporary corpus lets the campaign grow without adding every new coverage
input to git. Cargo-fuzz enables AddressSanitizer and coverage instrumentation;
`cargo check --manifest-path fuzz/Cargo.toml --locked` checks compilation alone.
The standalone fuzz workspace has its own lockfile and disables CLI component
features so Wasmtime is not part of this target.

Crashes are written under `fuzz/artifacts/incremental_edits/`. Replay and minimize:

```sh
cargo +nightly fuzz run incremental_edits fuzz/artifacts/incremental_edits/crash-HASH
cargo +nightly fuzz tmin incremental_edits fuzz/artifacts/incremental_edits/crash-HASH
```

Keep a minimized reproducer and add a focused regression before fixing a defect.
A bounded successful campaign is a smoke check, not proof that parsing is panic-free.

## Input format and bounds

- Byte 0: bits 0–1 choose the profile (Portable, JSON, XML, CSharp4); bit 2
  chooses recovery; bit 3 chooses generated initial parsing.
- Byte 1: initial source byte count, clipped to the remaining input.
- Remaining bytes: up to sixteen edits, each with a little-endian `u16` position,
  one removal-count byte, one insertion-length byte, then insertion bytes.
- Position selects a character boundary modulo the current boundary count;
  removal counts characters and clamps at EOF. Insertion length is reduced
  modulo 25 and clipped to the remaining input.

The decoded document stays below 2 KiB even with lossy UTF-8 expansion. Recognition
has a one-million-work cap, 4,096 logical frames and an 8 MiB spill-stack cap;
recovery uses at most 128 local search states and a 512-byte local window.
Resource-limit outcomes are checked for safe return but excluded from equivalence:
reuse spends less work, so warm and full parses can hit different limits. Recovery
repair choices, diagnostics and syntax must match whenever neither path is limited.

## Local verification, November 10, 2026

On macOS arm64, the final harness passed a nightly AddressSanitizer campaign:
4,738 executions in 61 seconds, seed `20261110`, using the grown temporary corpus
from an earlier 5,698-execution smoke run. No crash or oracle mismatch was reported.
This is bounded local evidence; other platforms and longer campaigns remain unverified.
The runtime emitted `atos` symbolization warnings for some newly reached functions;
coverage instrumentation remained active. The final target also passes standalone
`cargo check` and clippy. Ordinary workspace tests pass (816 passed, three explicitly
ignored), including all sixteen committed fuzz seeds. SIMD core tests, generated
scanner/parser checks, formatting, all-feature workspace clippy and grammar/catalog/
binding/project bootstrap fixed-point checks also pass.
