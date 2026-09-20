//! Version and build provenance.
//!
//! `chiploom --version` has to be enough to reproduce a report: the release
//! version alone does not distinguish a tagged build from a developer's working
//! tree. Everything here is captured by `build.rs` at compile time.

use std::fmt::Write as _;

use serde::Serialize;

/// The crate version, which is also the release version of the whole product.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The version of the Core IPC protocol this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// Everything known about how this binary was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    /// Release version, e.g. `0.1.0-pre.0`.
    pub version: &'static str,
    /// Core IPC protocol version.
    pub protocol_version: u32,
    /// Abbreviated git commit, or `unknown` for a build from a source archive.
    pub commit: &'static str,
    /// Whether the working tree had uncommitted tracked changes at build time.
    pub dirty: bool,
    /// Target triple this binary runs on.
    pub target: &'static str,
    /// Host triple it was compiled on.
    pub host: &'static str,
    /// Cargo profile, `debug` or `release`.
    pub profile: &'static str,
    /// Compiler that produced it.
    pub rustc: &'static str,
    /// RFC 3339 UTC build time.
    pub built_at: &'static str,
}

/// Build provenance for this binary.
#[must_use]
pub const fn build_info() -> BuildInfo {
    BuildInfo {
        version: VERSION,
        protocol_version: PROTOCOL_VERSION,
        commit: env!("CHIPLOOM_GIT_SHA"),
        // `build.rs` emits the string "true" only when tracked files differed.
        dirty: matches!(env!("CHIPLOOM_GIT_DIRTY").as_bytes(), b"true"),
        target: env!("CHIPLOOM_BUILD_TARGET"),
        host: env!("CHIPLOOM_BUILD_HOST"),
        profile: env!("CHIPLOOM_BUILD_PROFILE"),
        rustc: env!("CHIPLOOM_BUILD_RUSTC"),
        built_at: env!("CHIPLOOM_BUILD_TIMESTAMP"),
    }
}

impl BuildInfo {
    /// The version, with the commit it was built from when that is known:
    /// `0.1.0-pre.0 (a1b2c3d4e5f6)`.
    ///
    /// This is what `clap` is given for `--version`, which prints the binary
    /// name itself, so the name is deliberately absent here.
    #[must_use]
    pub fn version_line(&self) -> String {
        let mut line = self.version.to_owned();
        if self.commit != "unknown" {
            let dirty = if self.dirty { "-dirty" } else { "" };
            // Writing into a `String` cannot fail; the result has nowhere to go.
            let _ = write!(line, " ({}{dirty})", self.commit);
        }
        line
    }

    /// The single line `chiploom version` prints, name included.
    ///
    /// Shaped so the common case stays short and a development build is
    /// immediately recognisable as one.
    #[must_use]
    pub fn short_line(&self) -> String {
        format!("chiploom {}", self.version_line())
    }

    /// The multi-line report `chiploom version --verbose` prints.
    #[must_use]
    pub fn detailed(&self) -> Vec<(&'static str, String)> {
        vec![
            ("version", self.version.to_owned()),
            ("protocol", self.protocol_version.to_string()),
            (
                "commit",
                if self.dirty {
                    format!("{} (dirty)", self.commit)
                } else {
                    self.commit.to_owned()
                },
            ),
            ("target", self.target.to_owned()),
            ("host", self.host.to_owned()),
            ("profile", self.profile.to_owned()),
            ("rustc", self.rustc.to_owned()),
            ("built", self.built_at.to_owned()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_is_populated() {
        let info = build_info();
        assert_eq!(info.version, VERSION);
        assert!(!info.version.is_empty());
        // `build.rs` always emits something, even in a tarball build.
        assert!(!info.target.is_empty());
        assert!(!info.built_at.is_empty());
        assert!(
            info.profile == "debug" || info.profile == "release",
            "{}",
            info.profile
        );
    }

    #[test]
    fn the_version_line_starts_with_the_binary_name() {
        // `--version` output is parsed by installers and CI; keep the shape.
        let line = build_info().short_line();
        assert!(line.starts_with("chiploom "), "{line}");
        assert!(line.contains(VERSION), "{line}");
    }

    #[test]
    fn the_bare_version_line_omits_the_binary_name() {
        // `clap` prints the name itself, so including it here would double it.
        let line = build_info().version_line();
        assert!(!line.contains("chiploom"), "{line}");
        assert!(line.starts_with(VERSION), "{line}");
    }

    #[test]
    fn the_detailed_report_covers_every_field() {
        let detailed = build_info().detailed();
        let keys: Vec<_> = detailed.iter().map(|(key, _)| *key).collect();
        for expected in [
            "version", "protocol", "commit", "target", "profile", "rustc", "built",
        ] {
            assert!(keys.contains(&expected), "missing `{expected}` in {keys:?}");
        }
    }

    #[test]
    fn the_build_timestamp_is_rfc3339_utc() {
        let built = build_info().built_at;
        assert_eq!(built.len(), 20, "unexpected timestamp shape: {built}");
        assert!(built.ends_with('Z'), "{built}");
        assert_eq!(&built[4..5], "-", "{built}");
        assert_eq!(&built[10..11], "T", "{built}");
    }
}
