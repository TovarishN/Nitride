#!/bin/sh
# Builds ../gregorian.wasm: a core module for wasm32-unknown-unknown, wrapped
# as a component by wasm-tools. Needs rustup's stable toolchain with the
# wasm32-unknown-unknown target, and wasm-tools. RUSTUP (default: rustup on
# PATH) picks the rustup binary; its toolchain's cargo and rustc are used
# directly, so another Rust earlier on PATH does not interfere.
set -eu
cd "$(dirname "$0")"
rustup="${RUSTUP:-rustup}"
cargo="$("$rustup" which --toolchain stable cargo)"
RUSTC="$("$rustup" which --toolchain stable rustc)" \
  "$cargo" build --release --target wasm32-unknown-unknown
wasm-tools component new target/wasm32-unknown-unknown/release/calendar_component.wasm \
  -o ../gregorian.wasm
wasm-tools validate ../gregorian.wasm
