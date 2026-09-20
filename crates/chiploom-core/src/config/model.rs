//! The effective configuration Chip Loom runs with, after every layer has been
//! merged.
//!
//! These types are always fully populated -- there is no "unset" state to check
//! at the point of use. The optional, partially-specified form lives in
//! [`super::patch`].

use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// The fully resolved configuration for one invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Config {
    /// Project identity.
    pub project: Project,
    /// Storage location overrides.
    pub paths: PathOverrides,
    /// Diagnostics output.
    pub log: Log,
    /// How Chip Loom is allowed to reach the network.
    pub network: Network,
}

/// Project identity, read from the project's `chiploom.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Project {
    /// Human-readable project name. Defaults to the project directory name.
    pub name: Option<String>,
}

/// Overrides for the platform storage locations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PathOverrides {
    /// Replaces the platform data directory (toolchains, target packs).
    pub data_dir: Option<PathBuf>,
    /// Replaces the platform cache directory (downloads, build cache, logs).
    pub cache_dir: Option<PathBuf>,
}

/// Diagnostics output settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Log {
    /// Minimum severity that reaches the output.
    pub level: LogLevel,
    /// How records are rendered.
    pub format: LogFormat,
    /// Optional file that receives a copy of every record, always as JSON.
    pub file: Option<PathBuf>,
    /// Whether human-readable output carries timestamps.
    pub timestamps: bool,
}

impl Default for Log {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            format: LogFormat::Compact,
            file: None,
            timestamps: false,
        }
    }
}

/// Network policy. Chip Loom downloads toolchains from Phase 2 onwards, so this
/// exists from Phase 0 to keep offline installations a first-class case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Network {
    /// When true, Chip Loom must never open a network connection, and commands
    /// that would need one fail with a clear message instead.
    pub offline: bool,
    /// Per-request timeout. Serialized as `timeout_secs`, the key used in
    /// `chiploom.toml`, so a serialized `Config` can be written back to a file.
    #[serde(rename = "timeout_secs", serialize_with = "serialize_duration_secs")]
    pub timeout: Duration,
    /// How many times a failed request is retried before giving up.
    pub retries: u32,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            offline: false,
            timeout: Duration::from_secs(30),
            retries: 3,
        }
    }
}

fn serialize_duration_secs<S: serde::Serializer>(
    value: &Duration,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_u64(value.as_secs())
}

/// Severity levels, ordered from least to most verbose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Only failures.
    Error,
    /// Failures and things that need attention.
    Warn,
    /// Normal progress reporting. The default.
    Info,
    /// Detail useful when diagnosing a problem.
    Debug,
    /// Everything, including per-message protocol traffic.
    Trace,
}

impl LogLevel {
    /// The value understood by `tracing`'s `EnvFilter`.
    #[must_use]
    pub const fn as_filter_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    /// Raises verbosity by `steps`, saturating at [`LogLevel::Trace`]. This is
    /// how repeated `-v` flags are applied.
    #[must_use]
    pub const fn more_verbose(self, steps: u8) -> Self {
        let mut level = self;
        let mut remaining = steps;
        while remaining > 0 {
            level = match level {
                Self::Error => Self::Warn,
                Self::Warn => Self::Info,
                Self::Info => Self::Debug,
                Self::Debug | Self::Trace => Self::Trace,
            };
            remaining -= 1;
        }
        level
    }

    /// Lowers verbosity by `steps`, saturating at [`LogLevel::Error`]. This is
    /// how repeated `-q` flags are applied.
    #[must_use]
    pub const fn less_verbose(self, steps: u8) -> Self {
        let mut level = self;
        let mut remaining = steps;
        while remaining > 0 {
            level = match level {
                Self::Trace => Self::Debug,
                Self::Debug => Self::Info,
                Self::Info => Self::Warn,
                Self::Warn | Self::Error => Self::Error,
            };
            remaining -= 1;
        }
        level
    }

    /// Every accepted spelling, for error messages and shell completion.
    pub const VARIANTS: [&'static str; 5] = ["error", "warn", "info", "debug", "trace"];
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_filter_str())
    }
}

impl FromStr for LogLevel {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Ok(Self::Error),
            // `warning` is what most people type; accept it rather than fail.
            "warn" | "warning" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            other => Err(ConfigError::InvalidValue {
                key: "log.level".to_owned(),
                message: format!(
                    "`{other}` is not a log level; expected one of {}",
                    Self::VARIANTS.join(", ")
                ),
            }),
        }
    }
}

/// How log records are rendered to the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// One line per record. The default.
    Compact,
    /// Multi-line, with fields expanded. Useful when reading a failure closely.
    Pretty,
    /// One JSON object per line, for machines and log shippers.
    Json,
}

impl LogFormat {
    /// Every accepted spelling.
    pub const VARIANTS: [&'static str; 3] = ["compact", "pretty", "json"];
}

impl std::fmt::Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Compact => "compact",
            Self::Pretty => "pretty",
            Self::Json => "json",
        })
    }
}

impl FromStr for LogFormat {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "compact" => Ok(Self::Compact),
            "pretty" | "full" => Ok(Self::Pretty),
            "json" => Ok(Self::Json),
            other => Err(ConfigError::InvalidValue {
                key: "log.format".to_owned(),
                message: format!(
                    "`{other}` is not a log format; expected one of {}",
                    Self::VARIANTS.join(", ")
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbosity_saturates_at_both_ends() {
        assert_eq!(LogLevel::Info.more_verbose(1), LogLevel::Debug);
        assert_eq!(LogLevel::Info.more_verbose(2), LogLevel::Trace);
        // Nine -v flags must not wrap around to `error`.
        assert_eq!(LogLevel::Info.more_verbose(9), LogLevel::Trace);
        assert_eq!(LogLevel::Info.less_verbose(1), LogLevel::Warn);
        assert_eq!(LogLevel::Info.less_verbose(9), LogLevel::Error);
        assert_eq!(LogLevel::Info.more_verbose(0), LogLevel::Info);
    }

    #[test]
    fn level_parsing_is_forgiving_but_bounded() {
        assert_eq!(
            "TRACE".parse::<LogLevel>().expect("uppercase"),
            LogLevel::Trace
        );
        assert_eq!(
            "  warn ".parse::<LogLevel>().expect("padded"),
            LogLevel::Warn
        );
        assert_eq!(
            "warning".parse::<LogLevel>().expect("alias"),
            LogLevel::Warn
        );
        let err = "chatty"
            .parse::<LogLevel>()
            .expect_err("nonsense must be rejected");
        // The message has to list what is accepted, or the user has to read source.
        assert!(err.to_string().contains("trace"), "{err}");
    }

    #[test]
    fn format_parsing_accepts_documented_aliases() {
        assert_eq!("json".parse::<LogFormat>().expect("json"), LogFormat::Json);
        assert_eq!(
            "full".parse::<LogFormat>().expect("alias"),
            LogFormat::Pretty
        );
        assert!("xml".parse::<LogFormat>().is_err());
    }

    #[test]
    fn defaults_are_the_documented_ones() {
        let config = Config::default();
        assert_eq!(config.log.level, LogLevel::Info);
        assert_eq!(config.log.format, LogFormat::Compact);
        assert!(!config.network.offline);
        assert_eq!(config.network.timeout, Duration::from_secs(30));
        assert_eq!(config.network.retries, 3);
    }
}
