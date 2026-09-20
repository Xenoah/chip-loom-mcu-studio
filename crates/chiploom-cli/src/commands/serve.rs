//! `chiploom serve --stdio`.

use chiploom_core::{Error, ExitCode, Result, ipc};

use super::Context;
use crate::cli::ServeArgs;

/// Serves one Core IPC session.
///
/// # Errors
/// Fails if no transport was selected, or if the transport fails mid-session.
pub(crate) fn run(args: &ServeArgs, context: &Context) -> Result<ExitCode> {
    if !args.stdio {
        return Err(Error::Unsupported(
            "`chiploom serve` needs a transport; pass `--stdio`".to_owned(),
        ));
    }

    // Nothing may be written to stdout but protocol frames, so this command
    // prints no banner and no summary. Logging already goes to stderr.
    let outcome = ipc::serve_stdio(context.loaded.clone())?;

    if outcome.is_clean() {
        Ok(ExitCode::Success)
    } else {
        // The session worked, but the client did not say goodbye. Worth a log
        // line, not a failure: editors get killed all the time.
        tracing::debug!(?outcome, "session ended without a shutdown request");
        Ok(ExitCode::Success)
    }
}
