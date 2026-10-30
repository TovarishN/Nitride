# Native release implementation plan

> **For agentic workers:** Use superpowers:executing-plans to implement in this session. The user requested judgment and continuous execution.

**Goal:** Version Nitride consistently and publish optimized native CLI archives and VS Code extensions through GitHub Releases.

**Architecture:** Cargo's workspace version is authoritative. A version tool updates and checks Cargo and npm lockfiles. Native release jobs train PGO independently for each target and x64 CPU variant; a baseline Rust launcher selects AVX2 safely. Release artifacts never link Wasmtime. GitHub publishes only after the whole native matrix succeeds.

**Tech Stack:** Rust, Python standard library, GitHub Actions, vsce, LLVM PGO and Linux BOLT.

## Global constraints

- Nine native VS Code targets: win32-x64/arm64, linux-x64/arm64/armhf, alpine-x64/arm64, darwin-x64/arm64. No web/WASM package.
- Release builds use `--no-default-features` to omit Wasmtime; ordinary builds retain existing behavior.
- PGO plus full LTO, opt-level 3 and one codegen unit; baseline and AVX2 variants on x64; Apple M1 tuning on macOS ARM64. No `target-cpu=native` in distributable binaries.
- BOLT on glibc Linux x64/ARM64, using instrumentation for hosted runners. Preserve symbols/relocations until BOLT completes, then strip. Other targets use PGO/LTO.
- Existing untracked `.claude/` is unrelated and preserved. Work in current checkout; no commit, push or tag until changes are verified.

## Review focus

- Older x64 CPUs must never execute AVX2 before detection.
- Version mismatch or invalid tags must prevent publication.
- Stale binaries from another target must never enter a VSIX.
- No-default-features must compile without Wasmtime and fail clearly on component calls.
- Training must cover actual LSP traffic and reject missing/empty profiles.

### Task 1: Versioning and native-only CLI

- [x] Write behavioral tests for version synchronization, invalid versions/tags, and native-only component rejection; run RED.
- [x] Add optional `wasm` default feature and guarded component operations; retain native/process functionality.
- [x] Implement `scripts/version.py check [--tag TAG]` and `set VERSION`; update both lockfiles.
- [x] Run Python tests and CLI default/no-default-feature tests.

### Task 2: Optimized builds and packaging

- [x] Write and run failing launcher tests for CPU variant choice, argument forwarding and exit status.
- [x] Implement a baseline-compiled Rust launcher and target matrix.
- [x] Implement PGO training including CLI parse/prepare/recovery plus an LSP session, target-local profile merging and optimized rebuild; Linux BOLT instrumentation/rewrite.
- [x] Stage only current target binaries in fresh directories, package CLI and VSIX, record optimization metadata and checksums. Keep host packaging helper usable.
- [x] Run release tooling tests, compile/test extension, and build/package a real macOS ARM64 PGO release locally.

### Task 3: Release workflow and documentation

- [x] Add version validation, build matrix, containerized Alpine/ARMv7 training, artifact validation and all-or-nothing GitHub publication.
- [x] Document version bump/tag commands, artifacts, CPU compatibility, optimization stages and platform limitations.
- [x] Validate workflow, Rust formatting/clippy/tests and Python tests. Get independent review, resolve material findings, and report local vs hosted verification explicitly.

## Verification and decisions

- Local workspace baseline and no-default CLI/LSP suites passed; default and native-only clippy passed with warnings denied. Extension: 18 tests. Release scripts: six tests, including read-only container payload staging and tampered archive detection. Launcher: Rust CPU selection test and real process argument/stdio/exit tests. actionlint passed. A real macOS ARM64 PGO build and VSIX/CLI packaging passed.
- Independent review found container output ownership would prevent packaging. Reproduced PermissionError, then moved staging to a host-owned temporary directory; regression passes.
- Decision: glibc x64/ARM64 releases require glibc 2.39 (Ubuntu 24.04); ARMv7 requires glibc 2.36 (Bookworm). Documented rather than claiming every VS Code-supported Linux distribution. Cost: older glibc distributions need a source build or Alpine/musl environment.
- Hosted verification and first GitHub release remain pending.
