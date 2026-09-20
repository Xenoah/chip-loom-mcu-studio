//! The single error type every Chip Loom layer reports through, and the process
//! exit codes it maps to.
//!
//! The rule the whole workspace follows: `chiploom-core` never prints and never
//! exits. It returns [`Error`]; the CLI decides how to render it and which
//! [`ExitCode`] to hand back to the shell, and the IPC server decides which
//! JSON-RPC error code to send. That keeps one failure taxonomy behind two very
//! different front ends.

use std::fmt;
use std::path::{Path, PathBuf};

/// Result alias used across the core crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Process exit codes, stable across releases because scripts and CI depend on
/// them. Values are deliberately sparse so later phases can slot in without
/// renumbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ExitCode {
    /// Everything the command was asked to do succeeded.
    Success = 0,
    /// A general, unclassified failure.
    Failure = 1,
    /// The command line itself was wrong (reserved for `clap`).
    Usage = 2,
    /// A configuration file was missing, unreadable or invalid.
    Config = 3,
    /// A filesystem operation failed.
    Io = 4,
    /// The IPC peer violated the protocol.
    Protocol = 5,
    /// The host environment cannot support the request.
    Environment = 6,
    /// `chiploom doctor` completed but reported at least one error.
    DiagnosticsFailed = 7,
    /// The request is understood but not supported on this build or platform.
    Unsupported = 8,
}

impl ExitCode {
    /// The raw value handed to the operating system.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl From<ExitCode> for std::process::ExitCode {
    fn from(code: ExitCode) -> Self {
        Self::from(code.as_u8())
    }
}

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u8())
    }
}

/// Every failure Chip Loom's core can report.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A configuration layer could not be loaded or made sense of.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// A filesystem operation failed, with the path that caused it.
    #[error("{operation} failed for `{}`", .path.display())]
    Io {
        /// What was being attempted, phrased as a noun ("read", "create directory").
        operation: &'static str,
        /// The path involved.
        path: PathBuf,
        /// The underlying OS error.
        #[source]
        source: std::io::Error,
    },

    /// The IPC peer sent something that is not valid for the protocol.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),

    /// The host environment is missing something Chip Loom needs.
    #[error("{0}")]
    Environment(String),

    /// Understood, but not available here.
    #[error("{0} is not supported on this platform or build")]
    Unsupported(String),

    /// An internal invariant broke. Always a bug in Chip Loom.
    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Convenience constructor that attaches the path and operation to an
    /// `io::Error`, because a bare "permission denied" is never actionable.
    pub fn io(operation: &'static str, path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            operation,
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    /// The exit code the CLI should terminate with for this error.
    #[must_use]
    pub const fn exit_code(&self) -> ExitCode {
        match self {
            Self::Config(_) => ExitCode::Config,
            Self::Io { .. } => ExitCode::Io,
            Self::Protocol(_) => ExitCode::Protocol,
            Self::Environment(_) => ExitCode::Environment,
            Self::Unsupported(_) => ExitCode::Unsupported,
            Self::Internal(_) => ExitCode::Failure,
        }
    }

    /// A short, machine-stable identifier, used in JSON output and log fields
    /// so tooling can branch on the kind without parsing prose.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Config(_) => "config",
            Self::Io { .. } => "io",
            Self::Protocol(_) => "protocol",
            Self::Environment(_) => "environment",
            Self::Unsupported(_) => "unsupported",
            Self::Internal(_) => "internal",
        }
    }

    /// A remediation hint shown under the error message when one applies.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Config(err) => Some(err.hint()),
            Self::Io { path, source, .. }
                if source.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                Some(format!(
                    "Check the permissions on `{}`, or point Chip Loom elsewhere with \
                     `paths.data_dir` in chiploom.toml.",
                    path.display()
                ))
            }
            Self::Environment(_) => {
                Some("Run `chiploom doctor` for a full report on this machine.".to_owned())
            }
            Self::Internal(_) => Some(
                "This is a bug in Chip Loom. Please report it with the output of \
                 `chiploom doctor --format json`."
                    .to_owned(),
            ),
            _ => None,
        }
    }
}

/// Failures specific to loading and validating configuration.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The file exists but is not valid TOML, or a value has the wrong type.
    #[error("`{}` is not valid Chip Loom configuration: {message}", .path.display())]
    Invalid {
        /// The offending file.
        path: PathBuf,
        /// The parser's explanation, including line and column where available.
        message: String,
    },

    /// A file named explicitly by the user does not exist. Discovered files
    /// that are absent are not an error -- they are simply skipped.
    #[error("configuration file `{}` does not exist", .path.display())]
    NotFound {
        /// The path that was requested.
        path: PathBuf,
    },

    /// A value parsed but is not usable.
    #[error("`{key}` is invalid: {message}")]
    InvalidValue {
        /// Dotted configuration key, e.g. `log.level`.
        key: String,
        /// Why the value was rejected, including the accepted values.
        message: String,
    },

    /// An environment variable override could not be parsed.
    #[error("environment variable `{name}` is invalid: {message}")]
    InvalidEnv {
        /// The variable name, e.g. `CHIPLOOM_LOG_LEVEL`.
        name: String,
        /// Why the value was rejected.
        message: String,
    },
}

impl ConfigError {
    /// Every configuration failure has an actionable next step, so unlike
    /// [`Error::hint`] this is not optional.
    fn hint(&self) -> String {
        match self {
            Self::Invalid { path, .. } => format!(
                "Fix `{}`, then run `chiploom config check` to confirm Chip Loom reads it.",
                path.display()
            ),
            Self::NotFound { .. } => {
                "Run `chiploom config path` to see where Chip Loom looks.".to_owned()
            }
            Self::InvalidValue { .. } | Self::InvalidEnv { .. } => {
                "Run `chiploom config show --format json` to inspect effective values.".to_owned()
            }
        }
    }
}

/// Violations of the Core IPC protocol, raised by both peers.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProtocolError {
    /// A frame was not valid JSON, or not a valid JSON-RPC object.
    #[error("malformed message: {0}")]
    Malformed(String),

    /// The peer's protocol version cannot be served by this build.
    #[error(
        "protocol version {requested} is not supported (this build speaks version {supported})"
    )]
    VersionMismatch {
        /// What the client asked for.
        requested: u32,
        /// What this build implements.
        supported: u32,
    },

    /// A request arrived before `initialize` completed.
    #[error("received `{method}` before the session was initialized")]
    NotInitialized {
        /// The method that arrived too early.
        method: String,
    },

    /// `initialize` was sent more than once on one session.
    #[error("the session is already initialized")]
    AlreadyInitialized,

    /// The transport closed or failed underneath us.
    #[error("transport failure: {0}")]
    Transport(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_stable() {
        // These numbers are a public contract: scripts and CI branch on them.
        assert_eq!(ExitCode::Success.as_u8(), 0);
        assert_eq!(ExitCode::Failure.as_u8(), 1);
        assert_eq!(ExitCode::Usage.as_u8(), 2);
        assert_eq!(ExitCode::Config.as_u8(), 3);
        assert_eq!(ExitCode::Io.as_u8(), 4);
        assert_eq!(ExitCode::Protocol.as_u8(), 5);
        assert_eq!(ExitCode::Environment.as_u8(), 6);
        assert_eq!(ExitCode::DiagnosticsFailed.as_u8(), 7);
        assert_eq!(ExitCode::Unsupported.as_u8(), 8);
    }

    #[test]
    fn config_errors_map_to_the_config_exit_code() {
        let err = Error::from(ConfigError::NotFound {
            path: PathBuf::from("/nope/chiploom.toml"),
        });
        assert_eq!(err.exit_code(), ExitCode::Config);
        assert_eq!(err.kind(), "config");
        assert!(err.hint().is_some());
    }

    #[test]
    fn io_errors_name_the_path_and_the_operation() {
        let err = Error::io(
            "create directory",
            "/root/forbidden",
            std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        );
        let rendered = err.to_string();
        assert!(rendered.contains("create directory"), "{rendered}");
        assert!(rendered.contains("/root/forbidden"), "{rendered}");
        assert_eq!(err.exit_code(), ExitCode::Io);
        // Permission problems must always suggest a way out.
        assert!(err.hint().is_some());
    }
}
