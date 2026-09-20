//! Layered configuration loading.
//!
//! Precedence, lowest to highest:
//!
//! 1. built-in defaults
//! 2. the user-global `config.toml`
//! 3. the project's `chiploom.toml`, found by walking up from the working
//!    directory
//! 4. `CHIPLOOM_*` environment variables
//! 5. command-line flags
//!
//! Loading never silently discards information: every layer that was consulted
//! is reported in [`Loaded::sources`] along with whether it existed, and keys
//! that no layer understood are reported in [`Loaded::warnings`]. That is what
//! makes `chiploom config show` able to answer "why is this value what it is?".

mod model;
mod patch;

pub use model::{Config, Log, LogFormat, LogLevel, Network, PathOverrides, Project};
pub use patch::{ConfigPatch, LogPatch, NetworkPatch, PathsPatch, ProjectPatch};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{ConfigError, Error, Result};
use crate::paths::{Paths, find_project_root};

/// Prefix for every Chip Loom environment variable.
pub const ENV_PREFIX: &str = "CHIPLOOM_";

/// Points Chip Loom at a specific configuration file, replacing discovery.
pub const ENV_CONFIG_FILE: &str = "CHIPLOOM_CONFIG";

/// Relocates the directory the user-global `config.toml` is read from.
///
/// Environment-only, because it decides which file to read and so cannot itself
/// come from a file. It is also the only portable way to relocate the directory:
/// on Windows the platform locations come from the Known Folder API rather than
/// from `%APPDATA%`, so overriding that variable achieves nothing.
pub const ENV_CONFIG_DIR: &str = "CHIPLOOM_CONFIG_DIR";

/// Relocates the data directory. Equivalent to `paths.data_dir`.
pub const ENV_DATA_DIR: &str = "CHIPLOOM_DATA_DIR";

/// Relocates the cache directory. Equivalent to `paths.cache_dir`.
pub const ENV_CACHE_DIR: &str = "CHIPLOOM_CACHE_DIR";

/// Which layer a value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Compiled-in defaults. Always present.
    Defaults,
    /// The user-global `config.toml`.
    GlobalFile,
    /// The project's `chiploom.toml`.
    ProjectFile,
    /// A file named explicitly with `--config` or `CHIPLOOM_CONFIG`.
    ExplicitFile,
    /// `CHIPLOOM_*` environment variables.
    Environment,
    /// Command-line flags.
    CommandLine,
}

/// Whether a layer contributed anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    /// The layer existed and its values were merged.
    Applied,
    /// The layer was looked for and is not there. Not an error.
    Missing,
    /// The layer was deliberately not consulted.
    Skipped,
    /// The layer exists but contributed no values.
    Empty,
}

/// One consulted configuration layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// Which layer this is.
    pub kind: SourceKind,
    /// The file it came from, for file-backed layers.
    pub path: Option<PathBuf>,
    /// Whether it contributed.
    pub status: SourceStatus,
}

/// Something worth telling the user that did not stop the load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    /// Stable identifier, e.g. `config.unknown_key`.
    pub code: &'static str,
    /// Human-readable explanation.
    pub message: String,
}

/// The result of a successful load.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    /// The effective configuration.
    pub config: Config,
    /// Resolved storage locations, with configuration overrides applied.
    pub paths: Paths,
    /// Every layer that was consulted, in precedence order.
    pub sources: Vec<Source>,
    /// Non-fatal problems found while loading.
    pub warnings: Vec<Warning>,
}

impl Serialize for Paths {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Paths", 8)?;
        state.serialize_field("configDir", self.config_dir())?;
        state.serialize_field("globalConfigFile", &self.global_config_file())?;
        state.serialize_field("dataDir", self.data_dir())?;
        state.serialize_field("toolchainsDir", &self.toolchains_dir())?;
        state.serialize_field("targetPacksDir", &self.target_packs_dir())?;
        state.serialize_field("cacheDir", self.cache_dir())?;
        state.serialize_field("logDir", &self.log_dir())?;
        state.serialize_field("projectRoot", &self.project_root())?;
        state.end()
    }
}

/// Builds a [`Loaded`] from the layers that apply to one invocation.
///
/// The environment is captured explicitly rather than read from the process at
/// the point of use. That keeps loading a pure function of its inputs, so tests
/// can exercise every override without mutating global process state.
#[derive(Debug, Clone)]
pub struct Loader {
    cwd: PathBuf,
    base_paths: Paths,
    explicit_file: Option<PathBuf>,
    skip_global: bool,
    skip_project: bool,
    env: BTreeMap<String, String>,
    overrides: ConfigPatch,
}

impl Loader {
    /// Creates a loader from the real process environment and working directory.
    ///
    /// # Errors
    /// Fails if the working directory or the platform directories cannot be
    /// determined.
    pub fn from_process() -> Result<Self> {
        let cwd = std::env::current_dir()
            .map_err(|source| Error::io("read the working directory", ".", source))?;
        let env = std::env::vars()
            .filter(|(key, _)| key.starts_with(ENV_PREFIX))
            .collect::<BTreeMap<_, _>>();

        // The overrides are passed to resolution rather than applied after it, so
        // a host whose home directory cannot be determined is still usable when
        // the user has said where everything goes.
        let dir = |name: &str| env.get(name).map(PathBuf::from);
        let base_paths =
            Paths::resolve(dir(ENV_CONFIG_DIR), dir(ENV_DATA_DIR), dir(ENV_CACHE_DIR))?;

        Ok(Self::new(cwd, base_paths).with_env(env))
    }

    /// Creates a loader with an explicit working directory and base paths.
    #[must_use]
    pub fn new(cwd: PathBuf, base_paths: Paths) -> Self {
        Self {
            cwd,
            base_paths,
            explicit_file: None,
            skip_global: false,
            skip_project: false,
            env: BTreeMap::new(),
            overrides: ConfigPatch::default(),
        }
    }

    /// Replaces discovery with one specific file, as `--config` does.
    #[must_use]
    pub fn with_explicit_file(mut self, file: Option<PathBuf>) -> Self {
        self.explicit_file = file;
        self
    }

    /// Skips the user-global file, as `--no-global-config` does. Used by CI to
    /// guarantee a run is not influenced by the machine it lands on.
    #[must_use]
    pub const fn skip_global(mut self, skip: bool) -> Self {
        self.skip_global = skip;
        self
    }

    /// Skips project discovery entirely.
    #[must_use]
    pub const fn skip_project(mut self, skip: bool) -> Self {
        self.skip_project = skip;
        self
    }

    /// Supplies the `CHIPLOOM_*` variables to consider.
    #[must_use]
    pub fn with_env(mut self, env: BTreeMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// Supplies the highest-precedence layer, built from command-line flags.
    #[must_use]
    pub fn with_overrides(mut self, overrides: ConfigPatch) -> Self {
        self.overrides = overrides;
        self
    }

    /// Reads and merges every applicable layer.
    ///
    /// # Errors
    /// Fails when a file that was named explicitly is missing, when any file is
    /// unreadable or invalid, or when a value is out of range.
    pub fn load(mut self) -> Result<Loaded> {
        // Applied before anything is read: it decides which file to read.
        self.base_paths = self
            .base_paths
            .clone()
            .with_config_dir(self.env.get(ENV_CONFIG_DIR).map(PathBuf::from));

        let mut sources = vec![Source {
            kind: SourceKind::Defaults,
            path: None,
            status: SourceStatus::Applied,
        }];
        let mut warnings = Vec::new();
        let mut merged = ConfigPatch::default();

        self.merge_user_file(&mut merged, &mut sources, &mut warnings)?;
        let project_root = self.merge_project_file(&mut merged, &mut sources, &mut warnings)?;

        let env_patch = patch_from_env(&self.env)?;
        let env_empty = env_patch.unknown_keys().is_empty() && !patch_has_values(&env_patch);
        merged.merge(env_patch);
        sources.push(Source {
            kind: SourceKind::Environment,
            path: None,
            status: if env_empty {
                SourceStatus::Empty
            } else {
                SourceStatus::Applied
            },
        });

        let cli_empty = !patch_has_values(&self.overrides);
        merged.merge(self.overrides);
        sources.push(Source {
            kind: SourceKind::CommandLine,
            path: None,
            status: if cli_empty {
                SourceStatus::Empty
            } else {
                SourceStatus::Applied
            },
        });

        let config = merged.resolve()?;

        // Relative path overrides are resolved against the project root when
        // there is one, so `data_dir = "vendor/toolchains"` in a checked-in
        // chiploom.toml means the same thing from any subdirectory.
        let anchor = project_root.as_deref().unwrap_or(&self.cwd);
        let paths = self
            .base_paths
            .with_data_dir(
                config
                    .paths
                    .data_dir
                    .as_deref()
                    .map(|dir| absolutize(anchor, dir)),
            )
            .with_cache_dir(
                config
                    .paths
                    .cache_dir
                    .as_deref()
                    .map(|dir| absolutize(anchor, dir)),
            )
            .with_project_root(project_root);

        Ok(Loaded {
            config,
            paths,
            sources,
            warnings,
        })
    }

    /// Merges whichever user-level file applies: the one named explicitly, or
    /// the platform-global one.
    fn merge_user_file(
        &self,
        merged: &mut ConfigPatch,
        sources: &mut Vec<Source>,
        warnings: &mut Vec<Warning>,
    ) -> Result<()> {
        // `CHIPLOOM_CONFIG` acts exactly like `--config`, with the flag winning.
        let explicit = self
            .explicit_file
            .clone()
            .or_else(|| self.env.get(ENV_CONFIG_FILE).map(PathBuf::from));

        if let Some(file) = explicit {
            // Named explicitly, so its absence is a real error: silently ignoring
            // it would run with a configuration the user did not ask for.
            if !file.is_file() {
                return Err(ConfigError::NotFound { path: file }.into());
            }
            let (layer, status) = read_patch(&file, warnings)?;
            merged.merge(layer);
            sources.push(Source {
                kind: SourceKind::ExplicitFile,
                path: Some(file),
                status,
            });
            return Ok(());
        }

        let global = self.base_paths.global_config_file();
        let status = if self.skip_global {
            SourceStatus::Skipped
        } else if global.is_file() {
            let (layer, status) = read_patch(&global, warnings)?;
            merged.merge(layer);
            status
        } else {
            SourceStatus::Missing
        };
        sources.push(Source {
            kind: SourceKind::GlobalFile,
            path: Some(global),
            status,
        });
        Ok(())
    }

    /// Finds the project, if any, and merges its `chiploom.toml`.
    ///
    /// Returns the project root so the caller can anchor relative path overrides
    /// to it.
    fn merge_project_file(
        &self,
        merged: &mut ConfigPatch,
        sources: &mut Vec<Source>,
        warnings: &mut Vec<Warning>,
    ) -> Result<Option<PathBuf>> {
        let project_root = if self.skip_project {
            None
        } else {
            find_project_root(&self.cwd)
        };

        match project_root.as_ref() {
            Some(root) => {
                let file = root.join(crate::paths::PROJECT_FILE);
                let (layer, status) = read_patch(&file, warnings)?;
                merged.merge(layer);
                sources.push(Source {
                    kind: SourceKind::ProjectFile,
                    path: Some(file),
                    status,
                });
            }
            None => sources.push(Source {
                kind: SourceKind::ProjectFile,
                path: None,
                status: if self.skip_project {
                    SourceStatus::Skipped
                } else {
                    SourceStatus::Missing
                },
            }),
        }

        Ok(project_root)
    }
}

fn absolutize(anchor: &Path, dir: &Path) -> PathBuf {
    if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        anchor.join(dir)
    }
}

fn read_patch(path: &Path, warnings: &mut Vec<Warning>) -> Result<(ConfigPatch, SourceStatus)> {
    let text = std::fs::read_to_string(path)
        .map_err(|source| Error::io("read configuration file", path, source))?;
    let layer = ConfigPatch::from_toml(&text, path)?;

    for key in layer.unknown_keys() {
        warnings.push(Warning {
            code: "config.unknown_key",
            message: format!(
                "`{}` sets `{key}`, which this version of Chip Loom does not use",
                path.display()
            ),
        });
    }

    let status = if text.trim().is_empty() {
        SourceStatus::Empty
    } else {
        SourceStatus::Applied
    };
    Ok((layer, status))
}

/// Translates `CHIPLOOM_*` variables into a patch.
///
/// # Errors
/// Fails when a variable is set but cannot be parsed, because silently ignoring
/// `CHIPLOOM_OFFLINE=yse` would be worse than stopping.
fn patch_from_env(env: &BTreeMap<String, String>) -> Result<ConfigPatch, ConfigError> {
    let mut patch = ConfigPatch::default();

    if let Some(raw) = env.get("CHIPLOOM_LOG_LEVEL") {
        patch.log.level =
            Some(
                raw.parse()
                    .map_err(|err: ConfigError| ConfigError::InvalidEnv {
                        name: "CHIPLOOM_LOG_LEVEL".to_owned(),
                        message: err.to_string(),
                    })?,
            );
    }
    if let Some(raw) = env.get("CHIPLOOM_LOG_FORMAT") {
        patch.log.format =
            Some(
                raw.parse()
                    .map_err(|err: ConfigError| ConfigError::InvalidEnv {
                        name: "CHIPLOOM_LOG_FORMAT".to_owned(),
                        message: err.to_string(),
                    })?,
            );
    }
    if let Some(raw) = env.get("CHIPLOOM_LOG_FILE") {
        patch.log.file = Some(PathBuf::from(raw));
    }
    if let Some(raw) = env.get(ENV_DATA_DIR) {
        patch.paths.data_dir = Some(PathBuf::from(raw));
    }
    if let Some(raw) = env.get(ENV_CACHE_DIR) {
        patch.paths.cache_dir = Some(PathBuf::from(raw));
    }
    if let Some(raw) = env.get("CHIPLOOM_OFFLINE") {
        patch.network.offline = Some(parse_bool("CHIPLOOM_OFFLINE", raw)?);
    }
    if let Some(raw) = env.get("CHIPLOOM_NETWORK_TIMEOUT") {
        patch.network.timeout_secs = Some(parse_number("CHIPLOOM_NETWORK_TIMEOUT", raw)?);
    }
    if let Some(raw) = env.get("CHIPLOOM_NETWORK_RETRIES") {
        patch.network.retries = Some(parse_number("CHIPLOOM_NETWORK_RETRIES", raw)?);
    }

    Ok(patch)
}

fn parse_bool(name: &str, raw: &str) -> Result<bool, ConfigError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" | "" => Ok(false),
        other => Err(ConfigError::InvalidEnv {
            name: name.to_owned(),
            message: format!("`{other}` is not a boolean; expected one of 1, 0, true, false"),
        }),
    }
}

fn parse_number<T: std::str::FromStr>(name: &str, raw: &str) -> Result<T, ConfigError> {
    raw.trim().parse().map_err(|_| ConfigError::InvalidEnv {
        name: name.to_owned(),
        message: format!("`{raw}` is not a non-negative whole number"),
    })
}

fn patch_has_values(patch: &ConfigPatch) -> bool {
    patch.project.name.is_some()
        || patch.paths.data_dir.is_some()
        || patch.paths.cache_dir.is_some()
        || patch.log.level.is_some()
        || patch.log.format.is_some()
        || patch.log.file.is_some()
        || patch.log.timestamps.is_some()
        || patch.network.offline.is_some()
        || patch.network.timeout_secs.is_some()
        || patch.network.retries.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _temp: tempfile::TempDir,
        cwd: PathBuf,
        paths: Paths,
    }

    /// Builds an isolated home + project tree so no test can read the real
    /// machine's configuration.
    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        let config_dir = root.join("home/config");
        let cwd = root.join("work/firmware/src");
        std::fs::create_dir_all(&config_dir).expect("config dir");
        std::fs::create_dir_all(&cwd).expect("cwd");
        let paths = Paths::new(config_dir, root.join("home/data"), root.join("home/cache"));
        Fixture {
            _temp: temp,
            cwd,
            paths,
        }
    }

    fn loader(fixture: &Fixture) -> Loader {
        Loader::new(fixture.cwd.clone(), fixture.paths.clone())
    }

    #[test]
    fn a_bare_machine_loads_the_defaults() {
        let fixture = fixture();
        let loaded = loader(&fixture).load().expect("load");

        assert_eq!(loaded.config, Config::default());
        assert!(loaded.warnings.is_empty());
        assert!(loaded.paths.project_root().is_none());
        // Even a missing layer is reported, so `config show` can say where it looked.
        let global = loaded
            .sources
            .iter()
            .find(|source| source.kind == SourceKind::GlobalFile)
            .expect("global source recorded");
        assert_eq!(global.status, SourceStatus::Missing);
    }

    #[test]
    fn the_project_file_overrides_the_global_file() {
        let fixture = fixture();
        std::fs::write(
            fixture.paths.global_config_file(),
            "[log]\nlevel = \"warn\"\nformat = \"json\"\n",
        )
        .expect("write global");
        let project_root = fixture
            .cwd
            .parent()
            .expect("parent")
            .parent()
            .expect("grandparent");
        std::fs::write(
            project_root.join(crate::paths::PROJECT_FILE),
            "[project]\nname = \"blinky\"\n[log]\nlevel = \"debug\"\n",
        )
        .expect("write project");

        let loaded = loader(&fixture).load().expect("load");

        assert_eq!(loaded.config.log.level, LogLevel::Debug);
        // The global file's format survives: the project file never mentioned it.
        assert_eq!(loaded.config.log.format, LogFormat::Json);
        assert_eq!(loaded.config.project.name.as_deref(), Some("blinky"));
        assert_eq!(
            loaded
                .paths
                .project_root()
                .map(|p| std::fs::canonicalize(p).expect("canonicalize")),
            Some(std::fs::canonicalize(project_root).expect("canonicalize root")),
        );
    }

    #[test]
    fn the_environment_overrides_files_and_flags_override_the_environment() {
        let fixture = fixture();
        std::fs::write(
            fixture.paths.global_config_file(),
            "[log]\nlevel = \"warn\"\n",
        )
        .expect("write global");

        let env = BTreeMap::from([("CHIPLOOM_LOG_LEVEL".to_owned(), "debug".to_owned())]);
        let loaded = loader(&fixture).with_env(env.clone()).load().expect("load");
        assert_eq!(loaded.config.log.level, LogLevel::Debug);

        let mut overrides = ConfigPatch::default();
        overrides.log.level = Some(LogLevel::Trace);
        let loaded = loader(&fixture)
            .with_env(env)
            .with_overrides(overrides)
            .load()
            .expect("load");
        assert_eq!(loaded.config.log.level, LogLevel::Trace);
    }

    #[test]
    fn skip_global_isolates_a_run_from_the_machine() {
        let fixture = fixture();
        std::fs::write(
            fixture.paths.global_config_file(),
            "[log]\nlevel = \"trace\"\n",
        )
        .expect("write global");

        let loaded = loader(&fixture).skip_global(true).load().expect("load");
        assert_eq!(loaded.config.log.level, LogLevel::Info);
        let global = loaded
            .sources
            .iter()
            .find(|source| source.kind == SourceKind::GlobalFile)
            .expect("global recorded");
        assert_eq!(global.status, SourceStatus::Skipped);
    }

    #[test]
    fn an_explicit_file_replaces_discovery_and_must_exist() {
        let fixture = fixture();
        std::fs::write(
            fixture.paths.global_config_file(),
            "[log]\nlevel = \"trace\"\n",
        )
        .expect("write global");
        let explicit = fixture.cwd.join("ci.toml");
        std::fs::write(&explicit, "[network]\noffline = true\n").expect("write explicit");

        let loaded = loader(&fixture)
            .with_explicit_file(Some(explicit.clone()))
            .load()
            .expect("load");
        assert!(loaded.config.network.offline);
        // The global file was replaced, not merged under.
        assert_eq!(loaded.config.log.level, LogLevel::Info);
        assert!(
            loaded
                .sources
                .iter()
                .all(|source| source.kind != SourceKind::GlobalFile),
            "an explicit file must not also consult the global file",
        );

        let err = loader(&fixture)
            .with_explicit_file(Some(fixture.cwd.join("absent.toml")))
            .load()
            .expect_err("a file the user named must exist");
        assert_eq!(err.exit_code(), crate::error::ExitCode::Config);
    }

    #[test]
    fn the_config_dir_override_relocates_the_global_file() {
        let fixture = fixture();
        // Written where the override points, not where the platform would put it.
        let elsewhere = fixture.cwd.join("ci-config");
        std::fs::create_dir_all(&elsewhere).expect("create dir");
        std::fs::write(elsewhere.join("config.toml"), "[network]\nretries = 11\n")
            .expect("write config");
        // ...and something different where the platform would.
        std::fs::write(
            fixture.paths.global_config_file(),
            "[network]\nretries = 1\n",
        )
        .expect("write platform config");

        let env = BTreeMap::from([(
            ENV_CONFIG_DIR.to_owned(),
            elsewhere.to_string_lossy().into_owned(),
        )]);
        let loaded = loader(&fixture).with_env(env).load().expect("load");

        assert_eq!(loaded.config.network.retries, 11);
        assert_eq!(loaded.paths.config_dir(), elsewhere);
        // `config path` has to stay truthful about where it actually looked.
        let global = loaded
            .sources
            .iter()
            .find(|source| source.kind == SourceKind::GlobalFile)
            .expect("global recorded");
        assert_eq!(
            global.path.as_deref(),
            Some(elsewhere.join("config.toml").as_path())
        );
    }

    #[test]
    fn chiploom_config_env_var_behaves_like_the_flag() {
        let fixture = fixture();
        let explicit = fixture.cwd.join("from-env.toml");
        std::fs::write(&explicit, "[network]\nretries = 9\n").expect("write explicit");

        let env = BTreeMap::from([(
            ENV_CONFIG_FILE.to_owned(),
            explicit.to_string_lossy().into_owned(),
        )]);
        let loaded = loader(&fixture).with_env(env).load().expect("load");
        assert_eq!(loaded.config.network.retries, 9);
    }

    #[test]
    fn an_unparsable_environment_variable_stops_the_load() {
        let fixture = fixture();
        let env = BTreeMap::from([("CHIPLOOM_OFFLINE".to_owned(), "yse".to_owned())]);
        let err = loader(&fixture)
            .with_env(env)
            .load()
            .expect_err("typo must not be ignored");
        let message = err.to_string();
        assert!(message.contains("CHIPLOOM_OFFLINE"), "{message}");
    }

    #[test]
    fn environment_booleans_accept_the_usual_spellings() {
        let fixture = fixture();
        for raw in ["1", "true", "TRUE", "yes", "on"] {
            let env = BTreeMap::from([("CHIPLOOM_OFFLINE".to_owned(), raw.to_owned())]);
            let loaded = loader(&fixture).with_env(env).load().expect("load");
            assert!(loaded.config.network.offline, "`{raw}` should mean offline");
        }
        for raw in ["0", "false", "no", "off"] {
            let env = BTreeMap::from([("CHIPLOOM_OFFLINE".to_owned(), raw.to_owned())]);
            let loaded = loader(&fixture).with_env(env).load().expect("load");
            assert!(!loaded.config.network.offline, "`{raw}` should mean online");
        }
    }

    #[test]
    fn unknown_keys_become_warnings_and_name_their_file() {
        let fixture = fixture();
        std::fs::write(
            fixture.paths.global_config_file(),
            "[log]\nlvl = \"debug\"\n",
        )
        .expect("write global");

        let loaded = loader(&fixture).load().expect("load");
        assert_eq!(loaded.warnings.len(), 1);
        let warning = &loaded.warnings[0];
        assert_eq!(warning.code, "config.unknown_key");
        assert!(warning.message.contains("log.lvl"), "{}", warning.message);
        assert!(
            warning.message.contains("config.toml"),
            "{}",
            warning.message
        );
    }

    #[test]
    fn an_invalid_file_stops_the_load_and_names_it() {
        let fixture = fixture();
        std::fs::write(
            fixture.paths.global_config_file(),
            "[log\nlevel = \"debug\"\n",
        )
        .expect("write global");

        let err = loader(&fixture).load().expect_err("broken TOML must fail");
        assert!(err.to_string().contains("config.toml"), "{err}");
        assert_eq!(err.exit_code(), crate::error::ExitCode::Config);
    }

    #[test]
    fn relative_path_overrides_anchor_to_the_project_root() {
        let fixture = fixture();
        let project_root = fixture
            .cwd
            .parent()
            .expect("parent")
            .parent()
            .expect("grandparent");
        std::fs::write(
            project_root.join(crate::paths::PROJECT_FILE),
            "[paths]\ndata_dir = \"vendor/toolchains\"\n",
        )
        .expect("write project");

        // Loaded from a nested directory, so a naive join against the cwd would
        // produce `work/firmware/src/vendor/toolchains`.
        let loaded = loader(&fixture).load().expect("load");
        // Project discovery canonicalizes, so the expectation must too: on macOS
        // the temporary directory lives under a symlinked `/var`.
        let expected = std::fs::canonicalize(project_root)
            .expect("canonicalize project root")
            .join("vendor/toolchains");
        assert_eq!(loaded.paths.data_dir(), expected);
    }

    #[test]
    fn absolute_path_overrides_are_left_alone() {
        let fixture = fixture();
        let absolute = if cfg!(windows) {
            r"C:\chiploom\data"
        } else {
            "/opt/chiploom/data"
        };
        let env = BTreeMap::from([("CHIPLOOM_DATA_DIR".to_owned(), absolute.to_owned())]);

        let loaded = loader(&fixture).with_env(env).load().expect("load");
        assert_eq!(loaded.paths.data_dir(), Path::new(absolute));
    }
}
