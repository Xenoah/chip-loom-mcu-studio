//! Where Chip Loom keeps things on each operating system, and how a project
//! root is discovered.
//!
//! Three kinds of location are kept strictly apart, because they have different
//! lifetimes and different backup expectations:
//!
//! * **config** -- hand-edited, small, belongs in the user's dotfiles.
//! * **data** -- installed toolchains, target packs; large, reproducible but
//!   expensive to re-fetch.
//! * **cache** -- build caches and downloads in flight; safe to delete at any
//!   moment.
//!
//! Every path can be overridden from configuration, which is what makes CI runs
//! and offline installations possible.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The file name Chip Loom looks for when discovering a project.
pub const PROJECT_FILE: &str = "chiploom.toml";

/// The directory Chip Loom writes per-project state into, inside a project root.
pub const PROJECT_STATE_DIR: &str = ".chiploom";

/// The application name used for platform directory lookups.
const APP_NAME: &str = "chiploom";

/// Resolved locations for one invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    config_dir: PathBuf,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    project_root: Option<PathBuf>,
}

impl Paths {
    /// Resolves platform defaults, with no project and no overrides.
    ///
    /// # Errors
    /// Fails if the platform exposes no home directory at all, which makes
    /// every subsequent location undefined.
    pub fn discover() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("", "", APP_NAME).ok_or_else(|| {
            Error::Environment(
                "cannot determine this user's home directory, so Chip Loom has nowhere to \
                 store configuration or toolchains"
                    .to_owned(),
            )
        })?;

        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
            cache_dir: dirs.cache_dir().to_path_buf(),
            project_root: None,
        })
    }

    /// Builds a set of paths explicitly. Used by tests and by callers that have
    /// already resolved overrides.
    #[must_use]
    pub fn new(config_dir: PathBuf, data_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            config_dir,
            data_dir,
            cache_dir,
            project_root: None,
        }
    }

    /// Returns a copy with the project root attached.
    #[must_use]
    pub fn with_project_root(mut self, root: Option<PathBuf>) -> Self {
        self.project_root = root;
        self
    }

    /// Returns a copy with `config_dir` replaced.
    ///
    /// Unlike the other two, this override cannot come from a configuration file
    /// -- it decides *which* file to read -- so it is set from the environment
    /// only. On Windows it is the only way to redirect the location at all: the
    /// platform directories come from the Known Folder API, not from `%APPDATA%`,
    /// so setting that variable has no effect.
    #[must_use]
    pub fn with_config_dir(mut self, dir: Option<PathBuf>) -> Self {
        if let Some(dir) = dir {
            self.config_dir = dir;
        }
        self
    }

    /// Returns a copy with `data_dir` replaced, when configuration overrides it.
    #[must_use]
    pub fn with_data_dir(mut self, dir: Option<PathBuf>) -> Self {
        if let Some(dir) = dir {
            self.data_dir = dir;
        }
        self
    }

    /// Returns a copy with `cache_dir` replaced, when configuration overrides it.
    #[must_use]
    pub fn with_cache_dir(mut self, dir: Option<PathBuf>) -> Self {
        if let Some(dir) = dir {
            self.cache_dir = dir;
        }
        self
    }

    /// Directory holding the user-global `config.toml`.
    #[must_use]
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// The user-global configuration file, whether or not it exists.
    #[must_use]
    pub fn global_config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// Directory for durable data: toolchains, target packs, registries.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Where downloaded toolchains are unpacked (Phase 2 owns the contents).
    #[must_use]
    pub fn toolchains_dir(&self) -> PathBuf {
        self.data_dir.join("toolchains")
    }

    /// Where target packs are installed (Phase 1 owns the contents).
    #[must_use]
    pub fn target_packs_dir(&self) -> PathBuf {
        self.data_dir.join("targets")
    }

    /// Directory for disposable data.
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Where in-flight and verified downloads land.
    #[must_use]
    pub fn downloads_dir(&self) -> PathBuf {
        self.cache_dir.join("downloads")
    }

    /// Where rotating log files are written.
    #[must_use]
    pub fn log_dir(&self) -> PathBuf {
        self.cache_dir.join("logs")
    }

    /// The discovered project root, if the command ran inside a project.
    #[must_use]
    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }

    /// The project's `chiploom.toml`, if a project was found.
    #[must_use]
    pub fn project_config_file(&self) -> Option<PathBuf> {
        self.project_root
            .as_ref()
            .map(|root| root.join(PROJECT_FILE))
    }

    /// Per-project state directory, if a project was found.
    #[must_use]
    pub fn project_state_dir(&self) -> Option<PathBuf> {
        self.project_root
            .as_ref()
            .map(|root| root.join(PROJECT_STATE_DIR))
    }

    /// Creates a directory and proves it is writable by round-tripping a probe
    /// file.
    ///
    /// A bare `create_dir_all` succeeds on read-only mounts and on Windows
    /// directories that deny writes, so the probe is what actually answers the
    /// question `chiploom doctor` is asking.
    ///
    /// # Errors
    /// Fails if the directory cannot be created, written to, or cleaned up.
    pub fn ensure_writable(dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)
            .map_err(|source| Error::io("create directory", dir, source))?;

        let probe = dir.join(format!(".chiploom-write-probe-{}", std::process::id()));
        std::fs::write(&probe, b"chiploom")
            .map_err(|source| Error::io("write to directory", dir, source))?;
        let result = std::fs::read(&probe)
            .map_err(|source| Error::io("read back from directory", dir, source))
            .and_then(|content| {
                if content == b"chiploom" {
                    Ok(())
                } else {
                    Err(Error::Internal(format!(
                        "write probe in `{}` read back different bytes",
                        dir.display()
                    )))
                }
            });
        // Always clean up, even when the read-back failed.
        let _ = std::fs::remove_file(&probe);
        result
    }
}

/// Walks up from `start` looking for the nearest directory containing
/// `chiploom.toml`.
///
/// Returns `None` rather than an error when there is no project: most commands
/// (`doctor`, `target list`) are perfectly usable outside one.
#[must_use]
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    // `canonicalize` keeps `..` segments from making the walk loop forever, but
    // a non-existent start path is not fatal -- fall back to it verbatim.
    let start = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
    start
        .ancestors()
        .find(|dir| dir.join(PROJECT_FILE).is_file())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_discovery_finds_the_nearest_root() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        let nested = root.join("firmware/src/drivers");
        std::fs::create_dir_all(&nested).expect("create nested dirs");
        std::fs::write(root.join(PROJECT_FILE), "").expect("write project file");

        let found = find_project_root(&nested).expect("project root");
        assert_eq!(
            std::fs::canonicalize(&found).expect("canonicalize found"),
            std::fs::canonicalize(root).expect("canonicalize root"),
        );
    }

    #[test]
    fn project_discovery_prefers_the_closest_root() {
        let temp = tempfile::tempdir().expect("temp dir");
        let outer = temp.path();
        let inner = outer.join("vendor/blinky");
        std::fs::create_dir_all(&inner).expect("create inner dir");
        std::fs::write(outer.join(PROJECT_FILE), "").expect("write outer");
        std::fs::write(inner.join(PROJECT_FILE), "").expect("write inner");

        let found = find_project_root(&inner).expect("project root");
        assert_eq!(
            std::fs::canonicalize(&found).expect("canonicalize found"),
            std::fs::canonicalize(&inner).expect("canonicalize inner"),
        );
    }

    #[test]
    fn no_project_is_not_an_error() {
        let temp = tempfile::tempdir().expect("temp dir");
        assert!(find_project_root(temp.path()).is_none());
    }

    #[test]
    fn write_probe_accepts_a_writable_directory_and_leaves_nothing_behind() {
        let temp = tempfile::tempdir().expect("temp dir");
        let target = temp.path().join("data/toolchains");
        Paths::ensure_writable(&target).expect("writable");

        let leftovers: Vec<_> = std::fs::read_dir(&target)
            .expect("read dir")
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        assert!(
            leftovers.is_empty(),
            "probe file was not cleaned up: {leftovers:?}"
        );
    }

    #[test]
    fn derived_directories_hang_off_their_parents() {
        let paths = Paths::new(
            PathBuf::from("/cfg"),
            PathBuf::from("/data"),
            PathBuf::from("/cache"),
        );
        assert_eq!(
            paths.global_config_file(),
            PathBuf::from("/cfg/config.toml")
        );
        assert_eq!(paths.toolchains_dir(), PathBuf::from("/data/toolchains"));
        assert_eq!(paths.target_packs_dir(), PathBuf::from("/data/targets"));
        assert_eq!(paths.downloads_dir(), PathBuf::from("/cache/downloads"));
        assert_eq!(paths.log_dir(), PathBuf::from("/cache/logs"));
        assert!(paths.project_config_file().is_none());
    }
}
