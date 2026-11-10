# Embed a language in a Rust application

The `nitride` facade provides parsing, checking and host-bound execution for an
admitted language. Its default runtime dependency is `nitride-core`. Preparation
and code generation are enabled separately for Cargo build scripts. Neither
path invokes .NET or requires the CLI, LSP, process host or Wasmtime.

Start with the [standalone calculator](../examples/embedded-calculator/). It owns
its grammar and catalog, implements a generated typed host interface in its
tests, and prints `42`:

```sh
cargo run --locked -p nitride-embedded-calculator
cargo test --locked -p nitride-embedded-calculator
```

## Cargo setup

Crates have not been published as part of this change. For an application outside
the repository, point both dependencies to the same Nitride checkout:

```toml
[dependencies]
nitride = { path = "/path/to/Nitride/crates/nitride" }

[build-dependencies]
nitride = { path = "/path/to/Nitride/crates/nitride", features = ["build"] }
```

For a workspace, use Cargo resolver `2` or later so build-dependency features stay
separate from normal dependencies. A standalone edition-2024 application already
uses a suitable resolver. Keep `build` on the build dependency; it is unnecessary for the runtime API.

Copy the calculator's [language directory](../examples/embedded-calculator/language/)
into your application. Its `project.json` selects an ordered grammar/catalog set
and the admitted typed start rule. Replace those sources with your language as
needed; unsupported semantics fail preparation rather than being ignored.

## Build script

Add `build.rs` at the application root:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    nitride::build::compile_project("language/project.json", "calculator.rs")?;
    Ok(())
}
```

`compile_project` accepts `.nproj` or JSON project manifests supported by native
preparation. Relative project paths resolve from `CARGO_MANIFEST_DIR`, regardless
of the current directory. The output must be a single ASCII `.rs` filename;
it is written below Cargo's `OUT_DIR`. The helper emits both language/parser
tables and typed catalog-interface bindings and registers every prepared input
with `cargo:rerun-if-changed`, including the manifest, grammars and catalogs.

Call the helper once per bundled language with distinct output names. Generated
Rust is intermediate build output; keep the authoritative grammar, catalog and
project files in source control.

## Application code

Generated tables currently refer to the crate name `nitride_core`. Declare the
following alias **once at the crate root**, including when generated code is
included inside modules. It resolves those references through the facade, so the
application does not need a second runtime dependency. If you rename the facade
in Cargo, use that dependency name in the `extern crate` declaration.

```rust
extern crate nitride as nitride_core;

// A binary may use only a subset of generated interface helpers.
#[allow(dead_code)]
mod language {
    include!(concat!(env!("OUT_DIR"), "/calculator.rs"));
}

fn main() -> Result<(), nitride::Error> {
    let engine = nitride::Engine::new(&language::LANGUAGE);
    let bindings = nitride::host::bindings(&language::LANGUAGE);
    let report = engine.run("let x : Scalar = 6; x * 7;", &bindings)?;
    assert_eq!(report.values, [42.0]);
    println!("{}", report.values[0]);
    Ok(())
}
```

`host::bindings` supplies the built-in numeric, text and structural handlers
explicitly named by the catalog. It does not implement arbitrary interfaces.
The example's `Arithmetic` interface generates a Rust trait and a `bindings`
function; its acceptance test implements that trait and executes `double(21);`.
See [host bindings](host-binding.md) for contracts, effects and process/component
hosts. Nothing is bound implicitly by `Engine`.

## Parsing, diagnostics and execution

`Engine::new` borrows a language's tables and uses default limits. It accepts
both static generated tables and language tables borrowed from local owners.
`with_limits` sets parse/check/execute limits; `with_parse_options` explicitly
overrides parsing options. The default rejects unterminated trailing comments.

- `parse(source)` returns an owning syntax tree, including for syntax-only profiles.
- `compile(source)` parses and checks a typed profile, returning typed HIR.
- `run(source, bindings)` parses, checks, preflights the whole program, then executes.
- `run_with_values(source, bindings, values)` additionally accepts explicit built-in value providers.

A program rejected by parsing, semantic checks or preflight invokes no host
operations or value providers. Preflight checks binding signatures, contracts,
effects and required values before execution. Value providers are cached within
one execution. A host failure after execution starts can follow earlier effects;
execution is not transactional and does not roll those effects back.

Errors implement `std::error::Error`. `phase()` distinguishes `Parse`, `Check`,
`Preflight` and `Execute`; `diagnostics()` preserves every diagnostic from that
stage, including spans, codes and severity. `trace()` preserves operation IDs
invoked during a failed execution, including the failing call. Earlier-stage
errors have empty traces. Match `Phase` with a fallback arm because it is
non-exhaustive.

This convenience API performs strict full parsing. It does not retain edit
snapshots or select generated structural callbacks. Re-exported core APIs,
including `parse_snapshot_generated` with the generated `PARSER`, remain available
for applications that need incremental/generated parsing or recovery. Syntax-only
profiles cannot compile or execute through the facade.

## Compatibility contract

The facade follows the workspace version, currently `0.4.0`. Public source APIs,
including re-exported core types, remain compatible within a `0.x` minor series:
`0.4.x` patch releases must not require source migrations. Breaking changes require
a new minor release and migration notes. Before `1.0`, a minor upgrade can require
application changes.

Use the same facade version in normal and build dependencies and regenerate
language tables when upgrading. Generated Rust is tied to the matching engine
version; its text and internal layout are not stable interfaces. This promise
does not establish a stable binary ABI, serialized grammar format or package-name
availability on crates.io. Publication is separate work.
