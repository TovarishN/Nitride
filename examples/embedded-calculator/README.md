# Standalone Rust embedding example

This application owns its `.ngr` grammar, catalog and project manifest. It uses
only the `nitride` facade at runtime and its optional build-script helper to emit
Rust tables and typed interface bindings. It does not import the CLI or any
bundled CLI language.

From the Nitride repository root:

```sh
cargo run --locked -p nitride-embedded-calculator
cargo test --locked -p nitride-embedded-calculator
cargo tree --locked -p nitride-embedded-calculator -e normal
```

The program prints `42`; the normal dependency tree contains only this app,
`nitride` and `nitride-core`. Tests cover diagnostics before effects, preflight,
execution failures, limits, explicit values and the generated `Arithmetic` host
interface. The main program opts into the catalog's built-in arithmetic handlers.

To copy it outside this workspace, replace the workspace-inherited package
metadata in `Cargo.toml` with explicit values (version `0.1.0`, edition `2024`,
license `MIT`, rust-version `1.85`), and change both `nitride` dependency paths to
`/path/to/Nitride/crates/nitride`. Keep `build.rs`, `src/` and `language/` together.
The [embedding guide](../../docs/embedding.md) explains the full recipe and
compatibility contract.
