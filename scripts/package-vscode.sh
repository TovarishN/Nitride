#!/usr/bin/env bash
# Build a fully optimized native release and package its extension on the target host.
# For container targets see .github/workflows/release.yml.
set -euo pipefail
platform="${1:?usage: scripts/package-vscode.sh <vsce-target>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
python3 "$root/scripts/release/build.py" "$platform"
(
  cd "$root/editors/vscode"
  npm ci
  npm run compile
  npm test
)
python3 "$root/scripts/release/package.py" "$platform"
