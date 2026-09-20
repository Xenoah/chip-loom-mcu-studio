//! The partially-specified form of the configuration, and how layers combine.
//!
//! Each layer -- global file, project file, environment, command line -- is
//! deserialized into a [`ConfigPatch`] where every field is optional. Patches
//! are then merged in precedence order and only at the end resolved against the
//! defaults.
//!
//! Doing it this way is what prevents the classic layering bug: if each file
//! were deserialized straight into a fully-defaulted `Config`, a project file
//! that says nothing about `log.level` would still overwrite the global value
//! with the default.
//!
//! Unknown keys are collected rather than rejected. A project pinned to an older
//! Chip Loom must still open a `chiploom.toml` written by a newer one, and
//! `chiploom config check` is where the user hears about keys that did nothing.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use super::model::{Config, LogFormat, LogLevel, Network, PathOverrides, Project};
use crate::error::ConfigError;

/// A configuration layer, with every field optional.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigPatch {
    /// Project identity.
    #[serde(default)]
    pub project: ProjectPatch,
    /// Storage location overrides.
    #[serde(default)]
    pub paths: PathsPatch,
    /// Diagnostics output.
    #[serde(default)]
    pub log: LogPatch,
    /// Network policy.
    #[serde(default)]
    pub network: NetworkPatch,
    /// Top-level keys this build does not know about.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, toml::Value>,
}

/// Optional form of [`Project`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectPatch {
    /// Human-readable project name.
    pub name: Option<String>,
    /// Keys inside `[project]` this build does not know about.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, toml::Value>,
}

/// Optional form of [`PathOverrides`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PathsPatch {
    /// Replaces the platform data directory.
    pub data_dir: Option<PathBuf>,
    /// Replaces the platform cache directory.
    pub cache_dir: Option<PathBuf>,
    /// Keys inside `[paths]` this build does not know about.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, toml::Value>,
}

/// Optional form of [`crate::config::Log`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogPatch {
    /// Minimum severity.
    pub level: Option<LogLevel>,
    /// Rendering style.
    pub format: Option<LogFormat>,
    /// Additional JSON log file.
    pub file: Option<PathBuf>,
    /// Whether human output carries timestamps.
    pub timestamps: Option<bool>,
    /// Keys inside `[log]` this build does not know about.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, toml::Value>,
}

/// Optional form of [`Network`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NetworkPatch {
    /// Forbid all network access.
    pub offline: Option<bool>,
    /// Per-request timeout, in seconds.
    pub timeout_secs: Option<u64>,
    /// Retry budget per request.
    pub retries: Option<u32>,
    /// Keys inside `[network]` this build does not know about.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, toml::Value>,
}

impl ConfigPatch {
    /// Overlays `higher` on top of `self`, with `higher` winning wherever it has
    /// an opinion.
    pub fn merge(&mut self, higher: Self) {
        let Self {
            project,
            paths,
            log,
            network,
            unknown,
        } = higher;

        overwrite(&mut self.project.name, project.name);
        extend(&mut self.project.unknown, project.unknown);

        overwrite(&mut self.paths.data_dir, paths.data_dir);
        overwrite(&mut self.paths.cache_dir, paths.cache_dir);
        extend(&mut self.paths.unknown, paths.unknown);

        overwrite(&mut self.log.level, log.level);
        overwrite(&mut self.log.format, log.format);
        overwrite(&mut self.log.file, log.file);
        overwrite(&mut self.log.timestamps, log.timestamps);
        extend(&mut self.log.unknown, log.unknown);

        overwrite(&mut self.network.offline, network.offline);
        overwrite(&mut self.network.timeout_secs, network.timeout_secs);
        overwrite(&mut self.network.retries, network.retries);
        extend(&mut self.network.unknown, network.unknown);

        extend(&mut self.unknown, unknown);
    }

    /// Resolves the merged patch against the defaults.
    ///
    /// # Errors
    /// Fails when a value parsed as the right type but is still unusable, such
    /// as a zero network timeout.
    pub fn resolve(self) -> Result<Config, ConfigError> {
        let defaults = Config::default();

        if let Some(secs) = self.network.timeout_secs
            && secs == 0
        {
            return Err(ConfigError::InvalidValue {
                key: "network.timeout_secs".to_owned(),
                message: "a timeout of 0 seconds would fail every request before it starts; \
                          use a positive number of seconds"
                    .to_owned(),
            });
        }

        Ok(Config {
            project: Project {
                name: self.project.name,
            },
            paths: PathOverrides {
                data_dir: self.paths.data_dir,
                cache_dir: self.paths.cache_dir,
            },
            log: crate::config::Log {
                level: self.log.level.unwrap_or(defaults.log.level),
                format: self.log.format.unwrap_or(defaults.log.format),
                file: self.log.file,
                timestamps: self.log.timestamps.unwrap_or(defaults.log.timestamps),
            },
            network: Network {
                offline: self.network.offline.unwrap_or(defaults.network.offline),
                timeout: self
                    .network
                    .timeout_secs
                    .map_or(defaults.network.timeout, Duration::from_secs),
                retries: self.network.retries.unwrap_or(defaults.network.retries),
            },
        })
    }

    /// Every unknown key in this patch, as dotted paths, sorted.
    ///
    /// Reported by `chiploom config check` so a typo like `log.lvl` is visible
    /// instead of silently doing nothing.
    #[must_use]
    pub fn unknown_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.unknown.keys().cloned().collect();
        let sections: [(&str, &BTreeMap<String, toml::Value>); 4] = [
            ("project", &self.project.unknown),
            ("paths", &self.paths.unknown),
            ("log", &self.log.unknown),
            ("network", &self.network.unknown),
        ];
        for (section, map) in sections {
            keys.extend(map.keys().map(|key| format!("{section}.{key}")));
        }
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    /// Parses TOML text into a patch.
    ///
    /// # Errors
    /// Fails when the text is not valid TOML or a known key has the wrong type.
    pub fn from_toml(text: &str, path: &std::path::Path) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|err| ConfigError::Invalid {
            path: path.to_path_buf(),
            message: err.message().trim().to_owned(),
        })
    }
}

fn overwrite<T>(target: &mut Option<T>, higher: Option<T>) {
    if let Some(value) = higher {
        *target = Some(value);
    }
}

fn extend(target: &mut BTreeMap<String, toml::Value>, higher: BTreeMap<String, toml::Value>) {
    target.extend(higher);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn patch(text: &str) -> ConfigPatch {
        ConfigPatch::from_toml(text, Path::new("test.toml")).expect("valid TOML")
    }

    #[test]
    fn a_silent_layer_does_not_clobber_a_lower_one() {
        // The whole reason patches exist: the project file says nothing about
        // the log level, so the global file's `debug` must survive.
        let mut merged = patch("[log]\nlevel = \"debug\"\n");
        merged.merge(patch("[project]\nname = \"blinky\"\n"));

        let config = merged.resolve().expect("resolve");
        assert_eq!(config.log.level, LogLevel::Debug);
        assert_eq!(config.project.name.as_deref(), Some("blinky"));
    }

    #[test]
    fn a_higher_layer_wins_where_it_speaks() {
        let mut merged = patch("[log]\nlevel = \"debug\"\nformat = \"json\"\n");
        merged.merge(patch("[log]\nlevel = \"trace\"\n"));

        let config = merged.resolve().expect("resolve");
        assert_eq!(config.log.level, LogLevel::Trace);
        // ...and leaves alone the key it did not mention.
        assert_eq!(config.log.format, LogFormat::Json);
    }

    #[test]
    fn empty_layers_resolve_to_the_defaults() {
        let config = ConfigPatch::default().resolve().expect("resolve");
        assert_eq!(config, Config::default());
    }

    #[test]
    fn unknown_keys_are_collected_not_rejected() {
        let patch = patch(
            "experimental = true\n\
             [log]\n\
             lvl = \"debug\"\n\
             [network]\n\
             proxy = \"http://example.invalid\"\n",
        );
        assert_eq!(
            patch.unknown_keys(),
            ["experimental", "log.lvl", "network.proxy"]
        );
        // Forward compatibility: the file still resolves.
        assert_eq!(patch.resolve().expect("resolve").log.level, LogLevel::Info);
    }

    #[test]
    fn a_wrong_type_on_a_known_key_is_an_error() {
        let err = ConfigPatch::from_toml("[log]\nlevel = 7\n", Path::new("bad.toml"))
            .expect_err("a number is not a log level");
        assert!(matches!(err, ConfigError::Invalid { .. }));
    }

    #[test]
    fn a_zero_timeout_is_rejected_with_a_reason() {
        let err = patch("[network]\ntimeout_secs = 0\n")
            .resolve()
            .expect_err("zero timeout is unusable");
        let message = err.to_string();
        assert!(message.contains("network.timeout_secs"), "{message}");
    }

    #[test]
    fn timeout_seconds_become_a_duration() {
        let config = patch("[network]\ntimeout_secs = 90\nretries = 0\n")
            .resolve()
            .expect("resolve");
        assert_eq!(config.network.timeout, Duration::from_secs(90));
        // Zero retries is a legitimate choice; only zero timeout is not.
        assert_eq!(config.network.retries, 0);
    }
}
