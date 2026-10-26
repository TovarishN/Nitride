#!/usr/bin/env bash
# Builds the native server for this host and packages it into a
# platform-specific VS Code extension: scripts/package-vscode.sh <vsce-target>
# (darwin-arm64, darwin-x64, linux-x64 or win32-x64; build each on its host).
set -euo pipefail
target="${1:?usage: scripts/package-vscode.sh <darwin-arm64|darwin-x64|linux-x64|win32-x64>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cargo build --release -p nitride-cli --manifest-path "$root/Cargo.toml"
binary=nitride
[[ "$target" == win32-* ]] && binary=nitride.exe
mkdir -p "$root/editors/vscode/server"
cp "$root/target/release/$binary" "$root/editors/vscode/server/$binary"
cd "$root/editors/vscode"
npm ci
npm run compile
npx vsce package --no-dependencies --allow-missing-repository --target "$target"
