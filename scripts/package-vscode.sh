#!/usr/bin/env bash
# Build a native release and package its extension on the target host:
# fully optimized with PGO by default, or a quicker plain optimized build with --no-pgo.
set -euo pipefail
usage="usage: scripts/package-vscode.sh [--no-pgo] <vsce-target>"
pgo=()
if [[ "${1:-}" == "--no-pgo" ]]; then
  pgo=(--no-pgo)
  shift
fi
platform="${1:?$usage}"
root="$(cd "$(dirname "$0")/.." && pwd)"
python3 "$root/scripts/release/build.py" ${pgo[@]+"${pgo[@]}"} "$platform"
(
  cd "$root/editors/vscode"
  npm ci
  npm run compile
  npm test
)
python3 "$root/scripts/release/package.py" ${pgo[@]+"${pgo[@]}"} "$platform"
