# Rust-only language preparation

The admitted languages now build from `.ngr`, catalog and project manifest inputs
using Rust alone. Cargo no longer reads exported `language.json` files. Existing
exports remain independent comparison fixtures; the .NET reference tools remain
development-only and are not used as a frontend fallback.

```sh
cargo build --release --workspace --locked
./target/release/nitride prepare languages/power/project.json
./target/release/nitride prepare languages/power/project.json --output /tmp/power.json
./target/release/nitride --power languages/power/sample.powercalc
```

## Admission and diagnostics

The frontend supports ordered modules, direct imports, qualified/local references,
implicit extension-target references, tokens with exclusions, literals, BMP
character classes/ranges, any character, predicates, labels, groups, ordered
choices, repetition and named extensible alternatives with precedence. Supported
typed clauses are sequential typed declarations, references, literal lowering
and fixed-operation lowering, subject to the existing catalog/profile limits.

Preparation rejects unsupported declarations/clauses, duplicate identities,
duplicate sequence labels, invalid fields, unknown/ambiguous references, lexical
references to syntax, invalid precedence, conflicting extension operators,
duplicate extension alternative names, nullable prefixes/tokens, unreachable
choices, non-progressing repetitions and left recursion. Nullable tokens are an
explicitly stricter native-profile restriction. Grammar nesting is bounded.

Syntax-only semantic blocks are opaque balanced regions, **not validated C# or
Nitrogen semantic statements**. Their omission is recorded in metadata. Ordinary
strings/chars/comments are scanned to locate braces; raw/verbatim C# strings are
outside admission. Parsed declarative clauses are validated before omission.
This exception permits the unchanged XML fixture without claiming semantic
compatibility. General .ngr functions, helpers, recovery, quotations and full
Nitrogen grammar validation remain outside this milestone.

Native parsing and reference errors contain the source path, one-based line and
column and UTF-8 byte span, including non-ASCII prefixes. Shared codegen/profile
gate errors currently fall back to the first source at byte zero with a message.
Normalization matches parsed reference JSON, including ordering, identities and
SHA-256 hashes; object formatting need not match. Legacy exports lacking the
optional `negated:false` field are compared using that documented default.

## Evidence

- **55 Rust tests** pass, including all previous runtime tests and new frontend,
  project, CLI and output-ownership regressions.
- **68 prepared profiles** match the live .NET exporter: four existing profiles,
  48 generated composed syntax profiles and 16 typed grammar variations. They
  exercise Unicode/control-character hash escaping and imported/extended rules.
- Both preparers reject **14 invalid grammars** in the live comparison corpus.
- All **11 .NET exporter tests**, **51 frozen runtime observations** and **128
  generated runtime cases** pass against the final native release.
- Formatter, strict clippy and locked release builds pass.
- A fresh independent review reproduced three defects: duplicate sequence labels,
  unvalidated omitted clause fields and deletion of a pre-existing temporary
  file after `create_new` failed. Regression tests failed before the repairs and
  passed afterward. Valid repeated labels in separate choices remain supported.

Fresh deployment check: copy only Git-visible source files, omitting `tools/`
and every `languages/*/language.json`; build offline in a separate target folder
with PATH restricted to `/usr/bin:/bin:/usr/sbin:/sbin`, absolute Cargo/rustc
paths and no DOTNET_ROOT. The build succeeds, and native preparation and sample
execution succeed in that environment. The host still has .NET installed
elsewhere; no clean-machine uninstall test was performed.

## Footprint and platform limits

Measured locally on macOS arm64 with Rust 1.99.1:

| Artifact | Bytes |
|---|---:|
| Combined execution/preparation CLI | 651,328 |
| Prior execution CLI | 452,512 |
| Profiling executable | 452,496 |

Preparation adds 198,816 bytes (194.2 KiB) to the combined CLI, now 636.1 KiB.
`otool -L` lists only libSystem. `nitride-core` still has no third-party
dependencies; SHA-256 and JSON preparation dependencies belong to tooling.
The prior [engine allocation measurements](stage3-evidence.md) describe the
unchanged runtime engine. Preparation's peak memory and final combined-CLI RSS
have not been measured; executable size alone does not establish process memory.

The CI matrix includes native preparation on macOS/Linux/Windows, but remote CI
has not run. Only local macOS execution is verified. Work remains local,
uncommitted and unpublished; existing Nitrogen tools are not replaced.

```sh
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/prepare_parity.py
python3 scripts/test_reference.py
python3 scripts/test_composition.py
python3 scripts/check_reference.py
python3 scripts/check_stage2_reference.py
python3 scripts/differential.py
python3 scripts/stage2_differential.py
```
