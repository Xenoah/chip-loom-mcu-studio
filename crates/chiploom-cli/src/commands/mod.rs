//! Command implementations.
//!
//! Each command is a thin adapter: it turns parsed arguments into a core call,
//! then hands the result to [`crate::render`] or to JSON. No command makes a
//! decision that the core could not also make for the editor.

mod completions;
mod config;
mod doctor;
mod serve;
mod version;

use chiploom_core::config::Loaded;
use chiploom_core::{ExitCode, Result};

use crate::cli::{Command, GlobalArgs};

/// Everything a command needs, assembled once in `main`.
#[derive(Debug)]
pub(crate) struct Context {
    /// The effective configuration and where it came from.
    pub(crate) loaded: Loaded,
    /// The global flags this run was invoked with.
    pub(crate) global: GlobalArgs,
}

impl Context {
    /// Whether results should be emitted as JSON.
    #[must_use]
    pub(crate) fn wants_json(&self) -> bool {
        self.global.format == crate::cli::OutputFormat::Json
    }

    /// The palette to render with, resolved against this terminal.
    #[must_use]
    pub(crate) fn palette(&self) -> crate::render::Palette {
        crate::render::Palette::resolve(
            self.global.color,
            std::io::IsTerminal::is_terminal(&std::io::stdout()),
            std::env::var_os("NO_COLOR").is_some(),
        )
    }
}

/// Runs the requested command.
///
/// # Errors
/// Propagates whatever the command failed with; `main` renders it.
pub(crate) fn dispatch(command: &Command, context: &Context) -> Result<ExitCode> {
    match command {
        Command::Doctor(args) => doctor::run(args, context),
        Command::Version => version::run(context),
        Command::Config(args) => config::run(args, context),
        Command::Serve(args) => serve::run(args, context),
        Command::Completions(args) => Ok(completions::run(args)),
    }
}
