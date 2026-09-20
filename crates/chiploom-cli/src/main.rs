//! The `chiploom` command-line interface.
//!
//! This binary is a front end and nothing more. It parses arguments, loads
//! configuration, installs logging, calls into `chiploom-core`, renders the
//! result, and translates failures into exit codes. Every decision it reports on
//! was made by the core, which is what guarantees the CLI and the VS Code
//! extension can never drift apart.
//!
//! Failure handling is deliberate: nothing here panics on bad input. A user
//! error becomes a rendered message plus a documented exit code (see
//! [`chiploom_core::ExitCode`]), so scripts can branch on the number and people
//! can read the sentence.

// This crate is the presentation layer: printing to stdout is its job. The
// workspace lint stays on everywhere else, where stdout belongs to the protocol.
#![allow(clippy::print_stdout, clippy::print_stderr)]
#![warn(clippy::pedantic)]

mod cli;
mod commands;
mod render;

use std::io::Write;

use chiploom_core::config::Loader;
use chiploom_core::{Error, ExitCode};
use clap::Parser;

fn main() -> std::process::ExitCode {
    // `clap` renders its own errors and chooses 2 for a usage problem, which is
    // the convention `ExitCode::Usage` records.
    let cli = cli::Cli::parse();

    match run(&cli) {
        Ok(code) => code.into(),
        Err(err) => {
            report(&err);
            err.exit_code().into()
        }
    }
}

fn run(cli: &cli::Cli) -> Result<ExitCode, Error> {
    // `-C` has to take effect before configuration discovery, or a project would
    // be found relative to the wrong directory.
    if let Some(directory) = &cli.global.directory {
        std::env::set_current_dir(directory)
            .map_err(|source| Error::io("change the working directory to", directory, source))?;
    }

    let loaded = Loader::from_process()?
        .with_explicit_file(cli.global.config.clone())
        .skip_global(cli.global.no_global_config)
        .with_overrides(cli.global.to_patch())
        .load()?;

    // Verbosity flags adjust whatever the files resolved to, so they are applied
    // after loading rather than as another layer.
    let log = chiploom_core::config::Log {
        level: cli.global.adjust_level(loaded.config.log.level),
        ..loaded.config.log.clone()
    };
    // Held until the process ends so a file-backed logger flushes.
    let _logging = chiploom_core::logging::init(&log)?;

    // Configuration is loaded before logging can be installed -- the log level
    // comes out of it -- so the summary of that work is recorded here instead.
    tracing::debug!(
        version = chiploom_core::VERSION,
        log_level = %log.level,
        project_root = ?loaded.paths.project_root(),
        layers = loaded
            .sources
            .iter()
            .filter(|source| source.status == chiploom_core::config::SourceStatus::Applied)
            .count(),
        "configuration loaded"
    );

    for warning in &loaded.warnings {
        tracing::warn!(code = warning.code, "{}", warning.message);
    }

    let context = commands::Context {
        loaded,
        global: cli.global.clone(),
    };
    commands::dispatch(&cli.command, &context)
}

/// Renders a failure on stderr: what happened, and what to do about it.
fn report(err: &Error) {
    let mut stderr = std::io::stderr().lock();
    // Deliberately ignoring write failures: there is nowhere left to report them.
    let _ = writeln!(stderr, "error: {err}");

    let mut source = std::error::Error::source(err);
    while let Some(cause) = source {
        let _ = writeln!(stderr, "  caused by: {cause}");
        source = cause.source();
    }

    if let Some(hint) = err.hint() {
        let _ = writeln!(stderr, "\nhint: {hint}");
    }
}
