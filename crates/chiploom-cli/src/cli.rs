//! Command-line surface.
//!
//! Everything here is declaration only. Each command's behaviour lives in
//! [`crate::commands`], and the decisions those commands make live in
//! `chiploom-core` -- so a capability is never reachable from the CLI but not
//! from the editor.

use std::path::PathBuf;
use std::sync::LazyLock;

use chiploom_core::config::{ConfigPatch, LogFormat, LogLevel};
use clap::{Args, Parser, Subcommand, ValueEnum};

/// The `chiploom` command.
#[derive(Debug, Parser)]
#[command(
    name = "chiploom",
    bin_name = "chiploom",
    about = "Chip Loom -- an integrated MCU development toolchain.",
    long_about = "Chip Loom builds, flashes, monitors and debugs microcontroller firmware \
                  from one tool, on Windows, macOS and Linux.\n\n\
                  Run `chiploom doctor` first: it reports whether this machine is ready \
                  and what to do about anything that is not.",
    version = SHORT_VERSION.as_str(),
    propagate_version = true,
    arg_required_else_help = true,
    max_term_width = 100
)]
pub(crate) struct Cli {
    /// Global options, shared by every subcommand.
    #[command(flatten)]
    pub(crate) global: GlobalArgs,

    /// The subcommand to run.
    #[command(subcommand)]
    pub(crate) command: Command,
}

/// Options accepted before or after the subcommand.
#[derive(Debug, Clone, Args)]
pub(crate) struct GlobalArgs {
    /// Use this configuration file instead of searching for one.
    ///
    /// Replaces both the user-global file and project discovery.
    #[arg(short = 'c', long, value_name = "FILE", global = true)]
    pub(crate) config: Option<PathBuf>,

    /// Ignore the user-global configuration file.
    ///
    /// Use this in CI so a run cannot be influenced by the machine it lands on.
    #[arg(long, global = true)]
    pub(crate) no_global_config: bool,

    /// Run as if Chip Loom had been started in this directory.
    #[arg(short = 'C', long = "directory", value_name = "DIR", global = true)]
    pub(crate) directory: Option<PathBuf>,

    /// Print more diagnostics. Repeat for more detail: -v, -vv, -vvv.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub(crate) verbose: u8,

    /// Print fewer diagnostics. Repeat to go quieter: -q, -qq.
    #[arg(short, long, action = clap::ArgAction::Count, global = true, conflicts_with = "verbose")]
    pub(crate) quiet: u8,

    /// How diagnostics are rendered on stderr.
    #[arg(long, value_name = "FORMAT", global = true)]
    pub(crate) log_format: Option<LogFormatArg>,

    /// Also append every diagnostic to this file, as JSON.
    #[arg(long, value_name = "FILE", global = true)]
    pub(crate) log_file: Option<PathBuf>,

    /// How command results are printed on stdout.
    #[arg(long, value_name = "FORMAT", default_value = "text", global = true)]
    pub(crate) format: OutputFormat,

    /// When to colour output.
    #[arg(long, value_name = "WHEN", default_value = "auto", global = true)]
    pub(crate) color: ColorChoice,

    /// Forbid all network access for this run.
    #[arg(long, global = true)]
    pub(crate) offline: bool,
}

impl GlobalArgs {
    /// Builds the highest-precedence configuration layer from these flags.
    #[must_use]
    pub(crate) fn to_patch(&self) -> ConfigPatch {
        let mut patch = ConfigPatch::default();

        // Verbosity is relative to whatever the files and environment resolved
        // to, so it is applied after loading, not here. Only an explicit level
        // belongs in the patch.
        if let Some(format) = self.log_format {
            patch.log.format = Some(format.into());
        }
        if let Some(file) = self.log_file.clone() {
            patch.log.file = Some(file);
        }
        if self.offline {
            patch.network.offline = Some(true);
        }
        patch
    }

    /// Applies `-v`/`-q` to an already-resolved level.
    #[must_use]
    pub(crate) fn adjust_level(&self, level: LogLevel) -> LogLevel {
        if self.verbose > 0 {
            level.more_verbose(self.verbose)
        } else if self.quiet > 0 {
            level.less_verbose(self.quiet)
        } else {
            level
        }
    }
}

/// How command results are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum OutputFormat {
    /// Aligned, human-readable text. The default.
    Text,
    /// A single JSON document, for scripts and CI.
    Json,
}

/// When to emit ANSI colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ColorChoice {
    /// Colour when stdout is a terminal and `NO_COLOR` is unset.
    Auto,
    /// Always colour.
    Always,
    /// Never colour.
    Never,
}

/// Mirror of [`LogFormat`] so `clap` can derive value parsing for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum LogFormatArg {
    /// One terse line per record.
    Compact,
    /// Multi-line, with fields expanded.
    Pretty,
    /// One JSON object per line.
    Json,
}

impl From<LogFormatArg> for LogFormat {
    fn from(value: LogFormatArg) -> Self {
        match value {
            LogFormatArg::Compact => Self::Compact,
            LogFormatArg::Pretty => Self::Pretty,
            LogFormatArg::Json => Self::Json,
        }
    }
}

/// The subcommands `chiploom` accepts.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Report whether this machine can run Chip Loom.
    ///
    /// Runs a series of checks and prints one line per check. Exits non-zero
    /// only if a check failed outright; warnings do not fail the command.
    Doctor(DoctorArgs),

    /// Print version and build provenance.
    ///
    /// Prints one line by default; `-v` adds the full build detail.
    Version,

    /// Inspect configuration.
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Run the Core IPC server. Editors invoke this; people rarely need to.
    Serve(ServeArgs),

    /// Generate a shell completion script.
    Completions(CompletionsArgs),
}

/// Arguments for `chiploom doctor`.
#[derive(Debug, Args)]
pub(crate) struct DoctorArgs {
    /// Also test that the hosts Chip Loom downloads from are reachable.
    #[arg(long)]
    pub(crate) online: bool,

    /// Do not write probe files; report only what can be seen.
    ///
    /// Makes the run read-only, at the cost of not proving writability.
    #[arg(long)]
    pub(crate) no_write_probe: bool,

    /// Exit non-zero on warnings as well as failures.
    #[arg(long)]
    pub(crate) strict: bool,
}

/// Arguments for `chiploom serve`.
#[derive(Debug, Args)]
pub(crate) struct ServeArgs {
    /// Serve the protocol on stdin and stdout.
    ///
    /// Required today, and named explicitly so other transports can be added
    /// without changing what existing clients invoke.
    #[arg(long)]
    pub(crate) stdio: bool,
}

/// `chiploom config` subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    /// Print the configuration this machine would run with.
    Show(ConfigShowArgs),

    /// Print every location Chip Loom reads configuration from.
    Path,

    /// Validate configuration files and report anything ignored.
    Check,
}

/// Arguments for `chiploom config show`.
#[derive(Debug, Args)]
pub(crate) struct ConfigShowArgs {
    /// Also print which layer each value came from.
    #[arg(long)]
    pub(crate) sources: bool,
}

/// Arguments for `chiploom completions`.
#[derive(Debug, Args)]
pub(crate) struct CompletionsArgs {
    /// The shell to generate a script for.
    #[arg(value_enum)]
    pub(crate) shell: clap_complete::Shell,
}

/// What both `-V` and `--version` print: one line, so a script can read the
/// version out of it. The full build record is `chiploom version -v`.
///
/// `clap` needs this with the static lifetime, and it is only knowable at run
/// time, so it is computed once on first use.
static SHORT_VERSION: LazyLock<String> =
    LazyLock::new(|| chiploom_core::build_info().version_line());

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_tree_is_well_formed() {
        // Catches duplicate flags, bad defaults and conflicting shorthands at
        // test time rather than when a user types them.
        Cli::command().debug_assert();
    }

    #[test]
    fn the_three_phase_zero_commands_parse() {
        for args in [
            vec!["chiploom", "doctor"],
            vec!["chiploom", "version"],
            vec!["chiploom", "config", "show"],
            vec!["chiploom", "serve", "--stdio"],
        ] {
            Cli::try_parse_from(&args).unwrap_or_else(|err| panic!("{args:?} failed: {err}"));
        }
    }

    #[test]
    fn global_flags_are_accepted_after_the_subcommand() {
        // `chiploom doctor --format json` is how people actually type it.
        let cli =
            Cli::try_parse_from(["chiploom", "doctor", "--format", "json", "-vv"]).expect("parse");
        assert_eq!(cli.global.format, OutputFormat::Json);
        assert_eq!(cli.global.verbose, 2);
    }

    #[test]
    fn verbose_and_quiet_cannot_be_combined() {
        assert!(Cli::try_parse_from(["chiploom", "doctor", "-v", "-q"]).is_err());
    }

    #[test]
    fn verbosity_flags_move_the_resolved_level() {
        let cli = Cli::try_parse_from(["chiploom", "doctor", "-vv"]).expect("parse");
        assert_eq!(cli.global.adjust_level(LogLevel::Info), LogLevel::Trace);

        let cli = Cli::try_parse_from(["chiploom", "doctor", "-q"]).expect("parse");
        assert_eq!(cli.global.adjust_level(LogLevel::Info), LogLevel::Warn);

        let cli = Cli::try_parse_from(["chiploom", "doctor"]).expect("parse");
        assert_eq!(cli.global.adjust_level(LogLevel::Debug), LogLevel::Debug);
    }

    #[test]
    fn offline_and_log_settings_reach_the_config_patch() {
        let cli = Cli::try_parse_from([
            "chiploom",
            "doctor",
            "--offline",
            "--log-format",
            "json",
            "--log-file",
            "run.log",
        ])
        .expect("parse");

        let patch = cli.global.to_patch();
        assert_eq!(patch.network.offline, Some(true));
        assert_eq!(patch.log.format, Some(LogFormat::Json));
        assert_eq!(
            patch.log.file.as_deref(),
            Some(std::path::Path::new("run.log"))
        );
    }

    #[test]
    fn an_empty_command_line_asks_for_help_rather_than_doing_nothing() {
        let err = Cli::try_parse_from(["chiploom"]).expect_err("bare invocation");
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn the_version_subcommand_takes_detail_from_the_global_verbosity_flag() {
        // `-v` is global, so `version` must not declare a second `--verbose`.
        let cli = Cli::try_parse_from(["chiploom", "version"]).expect("parse");
        assert_eq!(cli.global.verbose, 0);
        let cli = Cli::try_parse_from(["chiploom", "version", "-v"]).expect("parse");
        assert_eq!(cli.global.verbose, 1);
    }

    #[test]
    fn the_version_flag_reports_this_build() {
        let err = Cli::try_parse_from(["chiploom", "--version"]).expect_err("version exits");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
        assert!(err.to_string().contains(chiploom_core::VERSION), "{err}");
    }

    #[test]
    fn the_version_flag_prints_a_single_parsable_line() {
        // Installers do `chiploom --version | cut -d' ' -f2`; keep that working.
        let err = Cli::try_parse_from(["chiploom", "--version"]).expect_err("version exits");
        let text = err.to_string();
        assert_eq!(text.trim().lines().count(), 1, "{text}");
        let mut fields = text.trim().split(' ');
        assert_eq!(fields.next(), Some("chiploom"));
        assert_eq!(fields.next(), Some(chiploom_core::VERSION));
    }
}
