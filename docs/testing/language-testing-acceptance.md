# Native language testing acceptance

Native `.ntest` suites now cover parse, compile and run expectations through the
shared runner, CLI, generated project templates and VS Code Testing API. The
engine remains version 0.4.0 and the immutable lexical library remains 1.0.0.
No release was published.

The [checked package report](2026-11-10-packaged-acceptance.json) records
the clean product source commit `9d182ae31bd984c394bbfdd39c0d674853d170a0`,
native payload and VSIX hashes, library manifest, harness hashes and VS Code
1.145.2. This development darwin-arm64 package was built with `--no-pgo` and
trained against its final payload. The acceptance harness changes were applied
after packaging; their exact hashes are recorded independently.

All 12 extension-host API checks passed in a disposable profile. The Testing
checks exercise discovery, suite and case selection, value and diagnostic
results, unsaved fixture and grammar snapshots without saving, invalid-suite
recovery and cancellation that retains completed results without late success.
Existing authoring, navigation, diagnostic scheduling and grammar-reload API
checks also passed.

The disposable VS Code window could not become focused, including after its
native focus command. The latest run explicitly used `--allow-unfocused` and
records focused editor typing in `unavailable_checks`. That option substitutes
an API edit only to establish input for subsequent independent checks; it does
not claim typing acceptance. The default harness still requires focused typing,
and inherited environment settings cannot silently enable the exception.
These observations do not establish painted Testing view, screenshot or
new-window interaction acceptance.

One fresh whole-branch review found three Important issues. Each received an
observed failing regression followed by a passing fix in one continuing pass:

- An unavailable compile capability is infrastructure failure, so it cannot
  satisfy an expected compile rejection.
- Plain integral expectation literals outside the exact signed64/unsigned64
  JSON result range are rejected before float decoding. Decimal and exponent
  notation use binary64. Exact expected/actual text survives JavaScript decoding
  for value diffs; arbitrary precision integral assertions remain unsupported.
- A dependency opened after snapshot capture is registered and compared with
  the frozen input. Editing it cancels the worker and prevents stale success.

The post-review full native workspace test suite, strict all-features/all-targets
Clippy, formatting and four frontend bootstrap fixed points passed. The shared
testing crate passes 12 tests, the extension passes 29 tests, and the final
release/editor/benchmark Python tooling suite passes 44 tests. Optional SIMD
core and native/generated/shared-library scanner checks also passed.
Windows Node worker and controller checks were added to the native Windows CI
job; local packaged acceptance establishes macOS behavior only. Windows GUI
process cleanup remains unverified by this report.

Reproduce the package and scoped API acceptance with:

```sh
python3 scripts/release/build.py --no-pgo darwin-arm64
(cd editors/vscode && npm run compile && npm test)
python3 scripts/release/package.py --no-pgo darwin-arm64
python3 scripts/editor_acceptance.py \
  --code '/Applications/Visual Studio Code.app/Contents/MacOS/Code' \
  --vsix dist/no-pgo/nitride-0.4.0-darwin-arm64.vsix \
  --trace --allow-unfocused --output /tmp/language-testing-acceptance.json
```

Omit `--allow-unfocused` to require the focused typing interaction as well.
