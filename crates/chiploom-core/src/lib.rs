//! Chip Loom core.
//!
//! This crate holds every decision Chip Loom makes. The `chiploom` CLI and the
//! VS Code extension are both thin front ends over it:
//!
//! ```text
//!            ┌──────────────────────┐        ┌──────────────────────┐
//!            │  VS Code extension   │        │      chiploom CLI    │
//!            │     (TypeScript)     │        │        (Rust)        │
//!            └──────────┬───────────┘        └──────────┬───────────┘
//!                       │ JSON-RPC 2.0 over stdio       │ direct calls
//!                       └───────────────┬───────────────┘
//!                                       ▼
//!                            ┌─────────────────────┐
//!                            │    chiploom-core    │
//!                            │ config · logging ·  │
//!                            │ diagnostics · IPC   │
//!                            └─────────────────────┘
//! ```
//!
//! The rules that keep that boundary real:
//!
//! * **The core never prints and never exits.** It returns [`error::Error`]; the
//!   front end decides how to render it. That is why the same `doctor` run can
//!   become terminal text, a JSON document, or a JSON-RPC result with no
//!   duplicated logic.
//! * **The core owns no terminal.** Human-facing formatting lives in the CLI.
//! * **Every capability reaches both front ends.** A feature added to the core is
//!   exposed as a CLI command *and* an IPC method, so the extension can never
//!   diverge from what the CLI can do.
//!
//! # Getting started
//!
//! ```no_run
//! # fn main() -> Result<(), chiploom_core::Error> {
//! use chiploom_core::{config::Loader, doctor};
//!
//! let loaded = Loader::from_process()?.load()?;
//! let report = doctor::run(&loaded, doctor::Options::default());
//! assert!(!report.has_errors());
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod config;
pub mod doctor;
pub mod error;
pub mod ipc;
pub mod logging;
pub mod paths;
pub mod version;

pub use error::{ConfigError, Error, ExitCode, ProtocolError, Result};
pub use version::{BuildInfo, PROTOCOL_VERSION, VERSION, build_info};
