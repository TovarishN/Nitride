//! Native language-author test suites and deterministic execution.
mod frontend {
    include!(concat!(env!("OUT_DIR"), "/testing.rs"));
}
mod suite;
pub use suite::*;
mod compare;
mod runner;
mod snapshot;
pub use compare::values_equal;
pub use runner::{Discovery, Report, Runner, discover};
pub use snapshot::{Snapshot, identity};
