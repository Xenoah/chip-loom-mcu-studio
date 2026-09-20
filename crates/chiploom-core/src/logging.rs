//! Logging setup.
//!
//! Two rules shape this module, and both come from the fact that the CLI and the
//! IPC server share one binary:
//!
//! * **Diagnostics never touch stdout.** `chiploom serve --stdio` uses stdout as
//!   the protocol channel; a stray log line there corrupts the session. Human
//!   output goes to stderr, always.
//! * **Initialisation happens once.** A second call is a no-op rather than a
//!   panic, so embedding the core in a test harness or a long-lived process
//!   cannot bring it down.

use std::path::Path;
use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::{Log, LogFormat};
use crate::error::{Error, Result};

/// Environment variable that overrides the computed filter, using the full
/// `tracing` filter grammar (`chiploom_core::ipc=trace,info`).
pub const ENV_FILTER_VAR: &str = "CHIPLOOM_LOG";

static INITIALIZED: OnceLock<()> = OnceLock::new();

/// A handle kept alive for the lifetime of the process when logging to a file.
///
/// Dropping it flushes and closes the file, so it must outlive the program's
/// work; [`init`] returns it for the caller to hold.
#[derive(Debug)]
pub struct LoggingGuard {
    _file: Option<std::fs::File>,
}

/// Installs the global subscriber described by `config`.
///
/// Returns `Ok(None)` when logging was already initialised, which lets a caller
/// distinguish "I set this up" from "someone else did" without treating the
/// second case as a failure.
///
/// # Errors
/// Fails when `log.file` is set and its directory cannot be created, or the file
/// cannot be opened for appending.
pub fn init(config: &Log) -> Result<Option<LoggingGuard>> {
    if INITIALIZED.get().is_some() {
        return Ok(None);
    }

    let filter = EnvFilter::try_from_env(ENV_FILTER_VAR)
        .unwrap_or_else(|_| EnvFilter::new(config.level.as_filter_str()));

    let file = match config.file.as_deref() {
        Some(path) => Some(open_log_file(path)?),
        None => None,
    };

    // stderr, never stdout: stdout belongs to the IPC protocol.
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()));

    let registry = tracing_subscriber::registry().with(filter);

    // Each arm builds a differently-typed layer, so the match cannot be hoisted.
    match (config.format, config.timestamps, file.as_ref()) {
        (LogFormat::Json, _, file) => {
            let base = stderr_layer.json().flatten_event(true);
            match file {
                Some(file) => registry.with(base).with(json_file_layer(file)?).init(),
                None => registry.with(base).init(),
            }
        }
        (LogFormat::Pretty, true, file) => {
            let base = stderr_layer.pretty();
            match file {
                Some(file) => registry.with(base).with(json_file_layer(file)?).init(),
                None => registry.with(base).init(),
            }
        }
        (LogFormat::Pretty, false, file) => {
            let base = stderr_layer.pretty().without_time();
            match file {
                Some(file) => registry.with(base).with(json_file_layer(file)?).init(),
                None => registry.with(base).init(),
            }
        }
        (LogFormat::Compact, true, file) => {
            let base = stderr_layer.compact();
            match file {
                Some(file) => registry.with(base).with(json_file_layer(file)?).init(),
                None => registry.with(base).init(),
            }
        }
        (LogFormat::Compact, false, file) => {
            // The default: one terse line per record, no timestamp, because an
            // interactive command's output is read as it happens.
            let base = stderr_layer.compact().without_time().with_target(false);
            match file {
                Some(file) => registry.with(base).with(json_file_layer(file)?).init(),
                None => registry.with(base).init(),
            }
        }
    }

    let _ = INITIALIZED.set(());
    Ok(Some(LoggingGuard { _file: file }))
}

/// A log file always receives JSON with timestamps, whatever the terminal shows:
/// files are read later, by tools, out of context.
fn json_file_layer<S>(
    file: &std::fs::File,
) -> Result<
    tracing_subscriber::fmt::Layer<
        S,
        tracing_subscriber::fmt::format::JsonFields,
        tracing_subscriber::fmt::format::Format<tracing_subscriber::fmt::format::Json>,
        std::fs::File,
    >,
>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let handle = file
        .try_clone()
        .map_err(|source| Error::io("duplicate the log file handle", "<log file>", source))?;
    Ok(tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_ansi(false)
        .with_writer(handle))
}

fn open_log_file(path: &Path) -> Result<std::fs::File> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|source| Error::io("create the log directory", parent, source))?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| Error::io("open the log file", path, source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LogLevel;

    #[test]
    fn a_log_file_and_its_parent_directory_are_created() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("nested/deeper/chiploom.log");

        let file = open_log_file(&path).expect("open log file");
        drop(file);

        assert!(
            path.is_file(),
            "log file was not created at {}",
            path.display()
        );
    }

    #[test]
    fn opening_a_log_file_appends_rather_than_truncates() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("chiploom.log");
        std::fs::write(&path, b"earlier run\n").expect("seed file");

        drop(open_log_file(&path).expect("reopen"));

        let content = std::fs::read_to_string(&path).expect("read back");
        assert!(
            content.contains("earlier run"),
            "previous log content was lost"
        );
    }

    #[test]
    fn init_is_idempotent() {
        let config = Log {
            level: LogLevel::Warn,
            ..Log::default()
        };
        // Whichever call wins the race in a shared test process, neither panics
        // and a second call reports that someone else got there first.
        let first = init(&config).expect("first init");
        let second = init(&config).expect("second init");
        assert!(first.is_some() || second.is_none());
        assert!(second.is_none());
    }
}
