# Authoring kit acceptance

The native development package at commit `8d438fb` was built with
`python3 scripts/release/build.py --no-pgo darwin-arm64`, trained against its final
payload and packaged into a VSIX. It remains engine version 0.4.0; no release was
published. The independent bundled library version is 1.0.0.

[Checked package report](authoring/2026-11-09-packaged-acceptance.json) records
payload/VSIX hashes, clean source provenance, the canonical bundled library
manifest, harness fingerprints, VS Code version and nine passing observations.
These are extension-host API observations in a disposable profile, not
painted-frame, screenshot or new-window interaction acceptance.

The real bundled CLI created a typed Calculator project and evaluated its sample
to 42. The packaged extension navigated the qualified token to its canonical
read-only library document, inspected its content and returned no formatting
edits. An added missing grammar produced a structured diagnostic; its resolved
create-only quick fix created an empty Extra module without duplicating the
existing Program. Removing the pin produced the expected grammar errors; the
resolved versioned pin fix restored diagnostics. Existing editor typing,
independent scheduling, obsolete-edit and unsaved-grammar reload checks passed.

Two live integration failures were reproduced and fixed before acceptance:
unlisted nested standalone `nitride.nproj` files now supply their own grammar
library context, and macOS workspace-root aliases remain usable while symlinks
below the root are refused.

One fresh whole-branch review found four Important issues. Each received a
failing regression and a passing fix in the single fix pass:

- A resolved local Nitride.Lexical module suppresses a conflicting bundled pin.
- Pin ownership is reconstructed from current project text rather than a
  retained reader's previous inputs.
- A missing start rule uses an admitted TODO literal placeholder.
- Existing module names prevent duplicate start modules and choose a
  nonconflicting empty scaffold name.

The focused action suite passes all 10 tests. The extension compiles and passes
19 tests; release/editor/benchmark tooling passes 43 tests. The four frontend
bootstrap fixed points and strict workspace all-features Clippy pass. Optional
SIMD core and native/generated scanner checks, including the shared lexical
library oracle, pass. The full post-review workspace test run also passes (`cargo test --locked
--workspace --no-default-features`).

A broad optional Python discovery run also attempted the legacy Nitrogen/.NET
reference checks: 14 failures and 1 error came from missing Reference and
RecoveryReference binaries. Those checks did not establish reference parity in
this checkout. Native language preparation/runtime and the owning CI tooling
suite are independently verified. Cross-platform filesystem behavior still
needs the PR platform jobs; no global filesystem atomicity is claimed.

To reproduce the packaged workflow:

```sh
python3 scripts/release/build.py --no-pgo darwin-arm64
(cd editors/vscode && npm run compile && npm test)
python3 scripts/release/package.py --no-pgo darwin-arm64
python3 scripts/editor_acceptance.py \
  --code '/Applications/Visual Studio Code.app/Contents/MacOS/Code' \
  --vsix dist/no-pgo/nitride-0.4.0-darwin-arm64.vsix \
  --output /tmp/authoring-acceptance.json
```
