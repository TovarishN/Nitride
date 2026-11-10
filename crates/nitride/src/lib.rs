//! Embed an admitted Nitride language in a native Rust application.
//!
//! [`Engine`] parses, checks and runs source using explicit host bindings.
//! Core syntax, semantics and binding types are re-exported for generated
//! language tables and applications that need lower-level control.
//!
//! Enable the `build` feature in **build-dependencies** to use
//! `build::compile_project` from a Cargo build script. Runtime builds need
//! only this crate and `nitride-core`; no CLI, .NET or Wasmtime is required.
//!
//! Public source APIs are compatible within a `0.x` minor release series.
//! Breaking changes require a new minor release and migration notes. Generated
//! Rust is build output: regenerate it with the same facade version used at
//! runtime. This is not a stable ABI or serialized grammar format promise.
pub use nitride_core as core;
pub use nitride_core::*;
mod engine;
pub use engine::{Engine, Error, Phase};
#[cfg(feature = "build")]
pub mod build;
