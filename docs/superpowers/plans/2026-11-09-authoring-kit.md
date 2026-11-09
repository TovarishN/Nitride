# Versioned Grammar Library and Authoring Kit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans for inline execution, with one fresh whole-branch review at the end.

**Goal:** Ship the approved offline versioned lexical library, reusable project templates and bounded project quick fixes.
**Architecture:** A data-only embedded stdlib registry feeds one preparation resolver. CLI, editor registry and scaffolding consume shared typed metadata/plans. Physical paths and read-only bundled URI identities remain separate.
**Tech Stack:** Existing Rust workspace, serde_json/sha2 already in the dependency graph, native project frontend/bootstrap, LSP JSON, TypeScript VS Code extension.
**Spec:** ../specs/2026-11-09-authoring-kit-design.md (user approved).

## Global Constraints
- Exact library pins; initial Nitride.Lexical@1.0.0, immutable source/manifest identity; no network, ranges or fallback.
- Nine public tokens and their precise policies are fixed by spec section 2. Keywords/trivia remain per-language.
- Preserve no-library projects and standalone generated/prepared runtime behavior; no numeric-rule-ID compatibility promise.
- No fake filesystem paths for bundled source, no virtual edits/renames, no physical watching of embedded sources.
- Typed starter executes to42 without a host process/component/grant. Two presets, one generator, new directories only.
- Quick fixes never overwrite existing/open buffers; bounded workspace targets, capability/version/staleness checks.
- No binding stubs, remote package manager, Unicode/C# literal expansion, general example migration or speculative performance work.
- Existing isolated worktree; preserve user main checkout; no publication in this feature task.

## Review Focus
- Equal library bytes under distinct versions must invalidate the right cached identities (Task2/3 tests).
- Shared lexical wrappers must preserve whole-keyword exclusion and token/trivia boundaries (Task1/2 runtime/generated tests).
- Virtual URI traversal/client edits cannot alter canonical preparation or write builtin definitions (Task3 tests).
- Concurrent file appearance, open unsaved targets and symlinked parents must not cause data loss (Task4/5 tests).
- Stale/ambiguous project context cannot generate a misleading write-producing action (Task5 tests).

### Task1: Embedded immutable library
Files: create crates/nitride-stdlib/{Cargo.toml,build.rs,src/lib.rs,libraries/manifest.json,libraries/Nitride.Lexical/1.0.0/Lexical.ngr}; modify Cargo.toml/Cargo.lock and crates/nitride-prepare/Cargo.toml.
Interfaces: Library {name,version,exports,sources}; LibrarySource {name,uri,text,sha256}; libraries()->&'static[Library]; find(name:&str,version:&str)->Option<&'static Library>; source(uri:&str)->Option<&'static LibrarySource>; valid_version(&str)->bool. Runtime crate has no dependencies; build-time metadata generation may use existing serde_json/sha2.
- [x] RED tests for explicit registry/version/source lookup, canonical versions, exact URI matching and frozen1.0 source/manifest hashes.
- [x] Add canonical grammar/manifest and generate embedded registry from that one metadata source; fail build on source hash mismatch. Reject malformed/traversal URIs through exact membership.
- [x] Verify token acceptance/rejection, local `except` boundaries and trivia in interpreted and generated parsers using a prepare/codegen integration fixture; include JSON string cases. No new core lexer special cases.
- [x] Run stdlib/prepare focused tests; commit library data/API.

### Task2: One project resolver and native discovery
Files: languages/project/Project.ngr, crates/nitride-prepare/src/{nproj.rs,project.rs,lib.rs,project_seed.rs}; create src/libraries.rs and tests/libraries.rs; CLI main.rs plus library command module/tests; release/{build,package}.py and test_release.py.
Interfaces: LibraryReference {name,version,span,name_span,version_span}; ResolvedLibrary {name,version,sources}; resolve_libraries(&[LibraryReference])->Result<Vec<ResolvedLibrary>,Vec<PrepareError>>; library_sources(&[ResolvedLibrary])->Vec<Source>. ProjectLanguage/PreparedProject expose library refs/resolved metadata separately from physical paths. Registry metadata JSON equals the checked-in manifest (including canonical URIs/source hashes).
- [x] RED `.nproj`/JSON fixtures for exact valid pins, no-library defaults, missing/malformed/duplicate/conflicting versions, precise nproj spans and physical-only input paths.
- [x] Extend the actual project grammar and decoder/references/snapshot; regenerate project seed with `cargo run --locked -p nitride-prepare --example bootstrap`, then verify --check.
- [x] Merge resolved library sources before local sources in shared preparation, preserving local order. Keep prepared execution JSON self-contained and provenance in sidecars/source fingerprints.
- [x] Add `nitride library list [--json]` and `nitride library show Name@Version`; test native binary output and unknown inputs without downloads.
- [x] Record bundled manifest in native release metadata; package tests reject mismatched cross-platform/library manifests and retain strict artifact provenance.
- [x] Run focused prepare/CLI/release tests, including old profiles and generated/runtime oracle fixtures; commit resolver/discovery.

### Task3: Editor resolution and read-only inspection
Files: LSP manifest.rs, registry.rs, grammar_index.rs, analysis.rs, navigation.rs, completion.rs, server.rs, uri.rs; create library_documents.rs and tests/libraries.rs; VS Code extension.ts and library document provider/helper tests.
Interfaces: explicit origin for index sources (physical path or bundled library identity); GrammarFile keeps URI plus optional physical path/read-only metadata. `nitride/librarySource {uri}` returns canonical content only. Initialization option libraryDocuments enables virtual navigation. Entry carries pins; registry uses Task2 resolver and library identities in fingerprints.
- [x] RED registry tests for selected token use and bad pins preserving visible errors/last-good policy; resolver-injected equal-text version fingerprint test (Task2), ambiguous-owner context rejection and retained old reader tests.
- [x] Load bundled sources through the shared resolver; never route their URIs through canonical filesystem paths/watch inputs. Include library identity/version/hash in cache keys.
- [x] Add scoped grammar indexing/completion/hover/definitions; respect selected project pins. Where one physical input has ambiguous owning pins, do not silently select a version or offer edits; return the valid definition alternatives or explicit unresolved context.
- [x] RED protocol tests for exact read-only source retrieval, unsupported/malformed URIs, virtual changes, rename/format/action rejection (also from a local reference), and a client without virtual-source capability.
- [x] Implement content request and VS Code read-only provider; preserve normal grammar coloring/outline/navigation and ordinary file behavior. Keep library exports out of indiscriminate workspace-symbol flooding.
- [x] Run focused LSP/extension suites and bootstrap fixed point; commit editor library support.

### Task4: Shared templates and exclusive creation
Files: create prepare/src/authoring.rs plus authoring/{templates,execute}.rs and tests/authoring.rs; CLI authoring command/tests; LSP authoring command handler; VS Code commands/new-language helper, package.json command contribution and tests.
Interfaces: TemplateKind::{Syntax,Typed}; TemplateOptions {name,extension,kind}; PlannedFile {relative_path,contents,kind}; ProjectPlan {files}; plan_language(&TemplateOptions)->Result<ProjectPlan,AuthoringError>; create_language(target:&Path,plan:&ProjectPlan)->Result<CreationReport,AuthoringError>. Pure planner is shared; executor records exclusively owned writes/partial failure info.
- [x] RED temporary-project tests for both presets: prepare/parse, typed run42 with no-default-features, exact pinned library and deterministic plans. Reject bad identifiers/extensions, existing directories and filename collisions.
- [x] Implement minimal syntax/typed recipes and README commands; use stdlib refs, no copied lexical grammar, no custom operations/handlers in typed starter.
- [x] RED executor tests for denied writes, collision/race and injected mid-creation failure. Use exclusive creation, manifest last, conservative rollback of only untouched owned files; report retained partial paths.
- [x] Add CLI `new language NAME --directory PATH --extension .suffix --kind syntax|typed` (typed default), --dry-run and --json planning output. Validate invocation and native execution.
- [x] Add server create-language command and standard VS Code inputs. Restrict editor targets to a normalized, symlink-checked open workspace; offer opening the created standalone folder in a new window. Do not edit parent include lists or close existing editors.
- [x] Run authoring/CLI/extension and command protocol tests; commit templates/creation.

### Task5: Structured project file/library quick fixes
Files: prepare nproj diagnostics/project references and authoring planner; LSP project_actions.rs, analysis/server/actions, protocol tests; extension capability/acceptance integration.
Interfaces: ProjectIssue {code,kind,span,target,project}; ProjectActionPlan references current project identity/version and bounded CreateFile or project text edits. Shared helpers produce appropriate missing grammar/catalog scaffolds. Resolve actions from current context, not diagnostic message parsing.
- [x] RED tests for missing declared grammar/catalog, explicit qualified Lexical reference without pin, ambiguous owners, existing start declarations and unrelated diagnostics/ranges.
- [x] Add typed issue metadata and contextual recipes; missing file actions resolve only that reference and may leave truthful semantic diagnostics. No inferred short-identifier imports or fake host bindings.
- [x] RED protocol tests: stale versions, current open targets, filesystem collisions/symlinks/outside roots, virtual targets, unsupported client resource capabilities and requested action kinds.
- [x] Implement create-only workspace edits with overwrite=false/ignoreIfExists=false and versioned existing-document edits. Suppress unsafe/stale actions; preserve normal client failure/undo behavior without promising global resource atomicity.
- [x] Run action/native protocol suites and project-focused packaged editor workflows; commit targeted quick fixes.

### Task6: End-to-end authoring evidence and integration
Files: docs authoring guide/status; extend disposable editor acceptance/tooling tests as needed, release training/library metadata gates and checked raw reports.
- [x] Exercise a generated typed language end-to-end: creation, stdlib navigation/read-only source, sample execution42, authoring error, quick-fix creation and restored diagnostics. Syntax preset prepares/parses too.
- [x] Run full workspace/native/strict Clippy/format/bootstrap/extension/tooling checks appropriate to all modified owners. Build native package and validate authoring with actual bundled CLI/server (no component dependency).
- [x] Fresh whole-branch review; one regression-backed fix pass for material findings. Keep no-library regression evidence and explicit unsupported-client limitations.
- [x] Commit completed plan/evidence, push and open reviewable feature PR(s) with the three staged commits/increments. Do not tag/publish; keep isolated checkout clean.

## Self-review
Spec sections1–3 map to Tasks1/2; section4 to3; section5 to4; section6 to5; acceptance/release evidence to6. Native templates and metadata stay explicit. Internal type names may evolve to existing owning types without changing contracts; record rulings in the task ledger. Execute inline under the user's approved scope and established workflow; no additional implementation permission checkpoint is needed.
