//! `chiploom config show | path | check`.

use chiploom_core::{Error, ExitCode, Result};

use super::Context;
use crate::cli::{ConfigCommand, ConfigShowArgs};
use crate::render;

/// Inspects configuration.
///
/// # Errors
/// Fails if the output stream fails or the configuration cannot be serialized.
pub(crate) fn run(command: &ConfigCommand, context: &Context) -> Result<ExitCode> {
    match command {
        ConfigCommand::Show(args) => show(args, context),
        ConfigCommand::Path => path(context),
        ConfigCommand::Check => check(context),
    }
}

fn show(args: &ConfigShowArgs, context: &Context) -> Result<ExitCode> {
    let mut stdout = std::io::stdout().lock();
    if context.wants_json() {
        render::json(&mut stdout, &context.loaded).map_err(stdout_error)?;
    } else {
        render::config_show(
            &mut stdout,
            &context.loaded,
            args.sources,
            context.palette(),
        )
        .map_err(|err| Error::Internal(format!("failed to render the configuration: {err}")))?;
    }
    Ok(ExitCode::Success)
}

fn path(context: &Context) -> Result<ExitCode> {
    let mut stdout = std::io::stdout().lock();
    if context.wants_json() {
        render::json(
            &mut stdout,
            &serde_json::json!({
                "sources": context.loaded.sources,
                "paths": context.loaded.paths,
            }),
        )
        .map_err(stdout_error)?;
    } else {
        render::config_path(&mut stdout, &context.loaded, context.palette())
            .map_err(stdout_error)?;
    }
    Ok(ExitCode::Success)
}

fn check(context: &Context) -> Result<ExitCode> {
    let mut stdout = std::io::stdout().lock();
    if context.wants_json() {
        render::json(
            &mut stdout,
            &serde_json::json!({
                "valid": true,
                "sources": context.loaded.sources,
                "warnings": context.loaded.warnings,
            }),
        )
        .map_err(stdout_error)?;
    } else {
        render::config_check(&mut stdout, &context.loaded, context.palette())
            .map_err(stdout_error)?;
    }

    // Reaching this point means every file parsed: an invalid file fails during
    // loading, long before the command runs. Unknown keys are warnings only, so
    // `config check` succeeds -- a project pinned to an older Chip Loom must not
    // fail CI for mentioning a key a newer one added.
    Ok(ExitCode::Success)
}

fn stdout_error(source: std::io::Error) -> Error {
    Error::io("write to stdout", "<stdout>", source)
}
