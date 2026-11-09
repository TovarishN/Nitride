//! Native language-author test suites and deterministic execution.
mod frontend {
    include!(concat!(env!("OUT_DIR"), "/testing.rs"));
}
mod suite;
pub use suite::*;
