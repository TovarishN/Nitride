//! `nitride lsp`: a Language Server Protocol server over Nitride's engine.
//!
//! JSON-RPC over stdio with std threads only. Positions are UTF-16 on the
//! wire and UTF-8 byte offsets inside; `lines` converts at the boundary.
pub mod analysis;
pub mod lines;
pub mod manifest;
pub mod registry;
mod rpc;
mod server;
mod uri;

use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct ServeOptions {
    /// Quiet time after an edit before documents are re-analysed. Zero
    /// analyses after every message, which keeps scripted sessions exact.
    pub debounce: Duration,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(150),
        }
    }
}

/// Serves one client until `exit` or the end of `input`. Pending analyses
/// finish before the `shutdown` response.
pub fn serve(
    input: impl std::io::BufRead + Send + 'static,
    output: impl std::io::Write,
    options: ServeOptions,
) -> std::io::Result<()> {
    server::run(input, output, options)
}
