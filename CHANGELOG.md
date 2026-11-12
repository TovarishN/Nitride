# Changelog

Release dates use Europe/Tallinn. Entries summarize the linked published release
notes; the **Unreleased** section distinguishes merged development from local
feature-branch changes. Performance and acceptance claims retain the scope of
their linked reports.

## Unreleased

### Merged after 0.4.0

- Versioned lexical libraries, standalone language starters and bounded authoring
  fixes. [PR #53](https://github.com/TovarishN/Nitride/pull/53), [authoring guide](docs/guides/authoring.md).
- Native `.ntest` suites with phase/diagnostic/value assertions, generated starter
  tests and VS Code Testing backed by cancellable native workers.
  [PR #54](https://github.com/TovarishN/Nitride/pull/54), [testing guide](docs/guides/language-testing.md).

### Current feature branch — not merged or released

The [improvement tracker](docs/project/improvement-tracker.md) maps this work to
the original review suggestions and records remaining scope.

- Opt-in aarch64 NEON ASCII-run classification, direct vector-boundary tests,
  macOS feature-enabled CI coverage and matched default/scalar/NEON measurements.
  [ARM results and limits](docs/benchmarks/scanners/neon-2026-11-10.md).
- Standalone `nitride` Rust facade, optional Cargo build helper, owned calculator
  example and documented source compatibility contract. [Embedding guide](docs/guides/embedding.md).
- Shared-parser module split, documentation index/architecture guide and a bounded
  incremental fuzz target with replayable seeds. [Architecture](docs/project/architecture.md), [fuzzing](fuzz/README.md).
- Bounded catalog examples and iterative conformance evaluation, with depth/work
  checks before host effects. [Panic audit](docs/testing/panic-audit.md).
- Direct process/component crate tests, strict JSON-RPC envelopes and outgoing
  value shapes, component type checks and shared WIT-name normalization.
  [Host-binding coverage](docs/testing/host-bindings.md).
- Archived stage 2–45 evidence with byte-preserved raw reports, a path/hash
  manifest, repaired repository links and separate generated-report defaults.
  [Archive guide](docs/archive/stages/README.md).
- Organized the remaining root documentation into topic directories, preserving
  raw report bytes and routing fresh compiler reports to generated output.
  [Documentation index](docs/README.md), [migration map](docs/archive/layout-manifest.json).
- Investigated the reported macOS CI cancellations: GitHub runner-acquisition
  failures, followed by a passing `main` checkpoint. [CI evidence](docs/testing/macos-runner-capacity.md).

## [0.4.0](https://github.com/TovarishN/Nitride/releases/tag/v0.4.0) — 2026-11-09

- Owned editable runtime syntax snapshots and transactional UTF-16 ranged edits.
- LSP background analysis, independent document scheduling and cooperative
  parser/recovery cancellation.
- Direct snapshot projection and arena reuse with dated editor measurements.
- Configurable trivia, strict JSON admission and Calendar planning/duration workflows.
- Stable-revision LSP PGO training and verified native packaging for Linux x64,
  Windows x64 and macOS arm64. Distributed binaries omit the component runtime.

[Release verification and limitations](docs/releases/0.4.0.md) ·
[Changes since 0.3.0](https://github.com/TovarishN/Nitride/compare/v0.3.0...v0.4.0).

## [0.3.0](https://github.com/TovarishN/Nitride/releases/tag/v0.3.0) — 2026-11-07

- Structural equality for repeated check captures and `NGR0146` diagnostics for
  unsupported token captures.
- Descendant captures over plain syntax categories.

[Changes since 0.2.1](https://github.com/TovarishN/Nitride/compare/v0.2.1...v0.3.0).

## [0.2.1](https://github.com/TovarishN/Nitride/releases/tag/v0.2.1) — 2026-11-07

- Report independent grammar errors together instead of stopping at the first.
- Reduce recovery cache churn, repeated source lookup and small-file setup costs.
- Improve recovery coverage accounting, step accounting and keyword search.

[Changes since 0.2.0](https://github.com/TovarishN/Nitride/compare/v0.2.0...v0.2.1).

## [0.2.0](https://github.com/TovarishN/Nitride/releases/tag/v0.2.0) — 2026-11-03

- Optional ASCII run batching and AVX2 scanning, with CPU-targeting and PGO audits.
- Native release/version synchronization and a component-free VS Code server build.
- `.nproj` highlighting/error repairs and on-demand component-backed inlay hints.
- Named patterns, descendant search, inside/parent context and scoped parse context.
- Full-parse storage/setup improvements with paired measurements.

[Changes since 0.1.0](https://github.com/TovarishN/Nitride/compare/v0.1.0...v0.2.0).

## [0.1.0](https://github.com/TovarishN/Nitride/releases/tag/v0.1.0) — 2026-10-30

- Initial native CLI and VS Code packages with optimized release builds.
- Typed built-in/user-defined operations, declarative templates, quote rewrites,
  syntax helpers, built-in values and checked text literals.
- Native self-hosted grammar preparation, CSharp4 syntax admission and recovery.
- LSP diagnostics, navigation, editing help and palette coloring.
- `.ncatalog` and `.nproj` authoring, rule-local checks, typed interface contracts,
  process/component bindings and explicit effects.

[Initial release history](https://github.com/TovarishN/Nitride/commits/v0.1.0).
