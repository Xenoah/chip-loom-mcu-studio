//! `chiploom version`.

use chiploom_core::{ExitCode, Result, build_info};

use super::Context;
use crate::render;

/// Prints version and build provenance.
///
/// One line by default; the global `-v` flag asks for the whole build record,
/// which is what a bug report needs.
///
/// # Errors
/// Fails only if the output stream fails.
pub(crate) fn run(context: &Context) -> Result<ExitCode> {
    let info = build_info();
    let mut stdout = std::io::stdout().lock();

    let outcome = if context.wants_json() {
        render::json(&mut stdout, &info)
    } else {
        render::version(
            &mut stdout,
            &info,
            context.global.verbose > 0,
            context.palette(),
        )
    };
    outcome.map_err(|source| chiploom_core::Error::io("write to stdout", "<stdout>", source))?;

    Ok(ExitCode::Success)
}
