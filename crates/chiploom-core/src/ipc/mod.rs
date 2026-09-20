//! Inter-process communication between the Chip Loom core and its clients.
//!
//! The VS Code extension is a client; so is any other editor integration, and so
//! is `chiploom serve --stdio` driven from a script. Keeping the protocol here
//! rather than in the CLI is what enforces the workspace's central rule: the CLI
//! and the extension are two front ends over one core, and neither may grow
//! behaviour the other cannot reach.

pub mod protocol;
mod server;

pub use protocol::{
    Capabilities, DoctorParams, Id, Incoming, InitializeParams, InitializeResult, Notification,
    PongResult, Response, RpcError, codes,
};
pub use server::{Outcome, Server};

use crate::config::Loaded;
use crate::error::Result;

/// Serves one session on this process's stdin and stdout.
///
/// # Errors
/// Fails only if the transport itself fails; protocol-level problems are
/// reported to the client and the session continues.
pub fn serve_stdio(loaded: Loaded) -> Result<Outcome> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut server = Server::new(loaded);
    tracing::info!(session = server.session_id(), "serving Core IPC on stdio");
    server.serve(stdin.lock(), stdout.lock())
}
