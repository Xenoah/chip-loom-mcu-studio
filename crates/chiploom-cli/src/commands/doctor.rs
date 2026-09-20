//! `chiploom doctor`.

use chiploom_core::{ExitCode, Result, doctor};

use super::Context;
use crate::cli::DoctorArgs;
use crate::render;

/// Runs every diagnostic and reports the result.
///
/// # Errors
/// Fails only if the output stream fails. A failing *check* is reported through
/// the exit code, not as an error: the report is the answer the user asked for.
pub(crate) fn run(args: &DoctorArgs, context: &Context) -> Result<ExitCode> {
    let options = doctor::Options {
        online: args.online,
        write_probe: !args.no_write_probe,
    };
    let report = doctor::run(&context.loaded, options);

    let mut stdout = std::io::stdout().lock();
    if context.wants_json() {
        render::json(&mut stdout, &report)
            .map_err(|source| chiploom_core::Error::io("write to stdout", "<stdout>", source))?;
    } else {
        render::doctor_report(&mut stdout, &report, context.palette(), args.strict)
            .map_err(|source| chiploom_core::Error::io("write to stdout", "<stdout>", source))?;
    }

    // Warnings fail the command only when the user asked for that, so CI on a
    // machine without `git` or VS Code is not permanently red.
    if args.strict && report.has_warnings() && !report.has_errors() {
        return Ok(ExitCode::DiagnosticsFailed);
    }
    Ok(report.exit_code())
}
