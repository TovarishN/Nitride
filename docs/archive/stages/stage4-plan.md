# Rust language preparation implementation plan

Execute inline under the user's Go approval of the Rust frontend milestone.
Spec: docs/design.md build-machine architecture plus the approved next-step
description: Rust .ngr frontend, composition, same normalized data, useful source
diagnostics and `nitride prepare`. No commits, pushes or publication.

## Contracts and boundaries

- New build/development crate `nitride-prepare`: safe Rust, `serde_json`, `sha2`,
  and existing `nitride-codegen`. Core engine dependencies stay unchanged.
- `prepare_sources(&[Source], catalog, start, mode)` returns a validated JSON
  value or a structured error containing source path, UTF-8 byte span and message.
  `prepare_project(path)` resolves the ordered manifest relative to its folder.
- Parse current lexical/syntax forms and supported declarative clauses. Reject
  unrecognized features, duplicate identities, missing/ambiguous imports,
  invalid precedence, lexical-to-syntax references and unsupported semantics.
- Syntax-only semantic blocks are opaque balanced regions, recorded as omitted;
  their contents are not parsed as C# or checked as Nitrogen semantic statements.
  This intentional boundary must be stated in docs and diagnostics metadata.
- Preserve normalized expression/rule order and identities, catalog data and
  hashes. Multi-source hashing must match System.Text.Json's default escaping.
  JSON object layout/whitespace need not match, parsed values must match.
- No full .ngr compatibility, .NET helper translation, runtime grammar loading,
  LSP, or new execution semantics. Bound frontend nesting for structured errors.

## Task 1: Frontend and composition

Files: new crate lexer.rs, parser.rs, normalize.rs, lib.rs; tests/frontend.rs.

- [x] Add fixture parity tests against all four checked-in exported profiles,
  plus malformed/Unicode spans, escapes, imports, extensions, duplicate rules,
  nullability/recursion, precedence and unsupported clauses; observe failure.
- [x] Implement byte-oriented lexer, recursive grammar parser with depth guard,
  typed AST and explicit opaque-block scanning. Resolve symbols and extensions
  in deterministic module order and generate normalized version-one data.
- [x] Reuse codegen for complete supported-profile validation before returning.
  Check hashes and normalized values against the reference fixtures.

## Task 2: Project/CLI/build integration

Files: manifest adapter; CLI Cargo.toml/main.rs/build.rs; portable/project.json;
CLI tests and existing CI configuration.

- [x] Add failing tests for relocated manifests, invalid JSON/missing files,
  CLI diagnostics, invalid input preserving existing output and stdout export.
- [x] Implement `nitride prepare <project.json> [--output <file>]`; output only
  after all validation, protect manifest/catalog/grammar inputs from overwrite.
- [x] Prepare every language directly from .ngr in build.rs, tracking all
  grammar/catalog/manifest inputs. Cargo builds require no dotnet or old exports.
- [x] Add a Rust-only CI preparation check for admitted profiles.

## Task 3: Live parity, deployment and review

- [x] Add development script comparing native preparation to live .NET export
  for relocated fixtures and generated supported grammars; test rejection cases.
- [x] Run workspace tests/fmt/clippy, existing exporter tests and all runtime
  reference comparisons; exercise a source-only Cargo build with export JSON
  removed and no dotnet on PATH. Measure preparation's binary/dependency impact.
- [x] One independent final review; repair material findings, record evidence
  and update README/progress with exact admission limits.

Pre-flight: normalization consumes parser AST; manifest adapter supplies exact
source bytes and path identities; build.rs and CLI both call the same validated
preparation API. Existing exported JSON is an independent reference, never a
frontend fallback. Original Nitrogen product files remain untouched.
