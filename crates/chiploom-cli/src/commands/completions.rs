//! `chiploom completions <SHELL>`.

use chiploom_core::ExitCode;
use clap::CommandFactory;

use crate::cli::{Cli, CompletionsArgs};

/// Writes a completion script for the requested shell to stdout.
///
/// Cannot fail: `clap_complete` swallows write errors itself, so there is no
/// `Result` to propagate.
pub(crate) fn run(args: &CompletionsArgs) -> ExitCode {
    let mut command = Cli::command();
    let name = command.get_name().to_owned();
    clap_complete::generate(args.shell, &mut command, name, &mut std::io::stdout());
    ExitCode::Success
}
