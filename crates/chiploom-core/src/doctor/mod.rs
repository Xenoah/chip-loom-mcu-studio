//! `chiploom doctor`: a report on whether this machine can run Chip Loom.
//!
//! The engine is deliberately dumb -- it runs a list of independent checks and
//! collects their results. What matters is the contract each check honours:
//!
//! * A check **never** returns an error to the caller. A check that cannot run
//!   reports [`Status::Error`] with an explanation, because a doctor that
//!   aborts halfway through is useless for the one machine that needs it.
//! * A check that would need the network is [`Status::Skipped`] unless the
//!   caller asked for online checks, so `doctor` is safe and fast by default.
//! * Every non-`Ok` result carries a `hint` saying what to do about it.
//!
//! Later phases add checks (installed toolchains, probe drivers, udev rules)
//! by pushing onto the same list.

mod checks;

use std::time::Instant;

use serde::Serialize;

use crate::config::Loaded;
use crate::error::ExitCode;
use crate::version::{BuildInfo, build_info};

/// The outcome of a single check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The check was not run.
    Skipped,
    /// Everything the check looked at is fine.
    Ok,
    /// Usable now, but something will bite later.
    Warn,
    /// Chip Loom cannot work correctly in this state.
    Error,
}

impl Status {
    /// A fixed-width marker for terminal output.
    #[must_use]
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Error => "fail",
            Self::Skipped => "skip",
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.marker())
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    /// Stable dotted identifier, e.g. `paths.data`. Safe to grep for in CI.
    pub id: &'static str,
    /// Short human-readable name.
    pub title: &'static str,
    /// What the check found.
    pub status: Status,
    /// The finding, phrased so it makes sense on its own in a bug report.
    pub detail: String,
    /// What to do about a non-`Ok` result.
    pub hint: Option<String>,
}

impl Check {
    fn new(
        id: &'static str,
        title: &'static str,
        status: Status,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            id,
            title,
            status,
            detail: detail.into(),
            hint: None,
        }
    }

    fn ok(id: &'static str, title: &'static str, detail: impl Into<String>) -> Self {
        Self::new(id, title, Status::Ok, detail)
    }

    fn warn(id: &'static str, title: &'static str, detail: impl Into<String>) -> Self {
        Self::new(id, title, Status::Warn, detail)
    }

    fn error(id: &'static str, title: &'static str, detail: impl Into<String>) -> Self {
        Self::new(id, title, Status::Error, detail)
    }

    fn skipped(id: &'static str, title: &'static str, detail: impl Into<String>) -> Self {
        Self::new(id, title, Status::Skipped, detail)
    }

    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// How many checks landed in each status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Checks that passed.
    pub ok: usize,
    /// Checks that warned.
    pub warn: usize,
    /// Checks that failed.
    pub error: usize,
    /// Checks that did not run.
    pub skipped: usize,
}

/// A complete diagnostics run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Build provenance, so a pasted report identifies its binary.
    pub build: BuildInfo,
    /// Every check, in the order they ran.
    pub checks: Vec<Check>,
    /// Wall-clock duration of the run.
    pub duration_ms: u64,
}

impl Report {
    /// Counts by status.
    #[must_use]
    pub fn summary(&self) -> Summary {
        let mut summary = Summary::default();
        for check in &self.checks {
            match check.status {
                Status::Ok => summary.ok += 1,
                Status::Warn => summary.warn += 1,
                Status::Error => summary.error += 1,
                Status::Skipped => summary.skipped += 1,
            }
        }
        summary
    }

    /// The worst status in the report, which is the report's own status.
    #[must_use]
    pub fn status(&self) -> Status {
        self.checks
            .iter()
            .map(|check| check.status)
            .max()
            .unwrap_or(Status::Ok)
    }

    /// The process exit code for this report.
    ///
    /// Warnings do not fail the command: a machine with no `code` on `PATH` is
    /// perfectly able to build firmware, and CI that treats that as a failure
    /// would be unusable. Callers that want strictness pass `--strict`, which
    /// they implement by checking [`Report::status`] themselves.
    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        if self.has_errors() {
            ExitCode::DiagnosticsFailed
        } else {
            ExitCode::Success
        }
    }

    /// Whether any check failed outright.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == Status::Error)
    }

    /// Whether any check warned.
    #[must_use]
    pub fn has_warnings(&self) -> bool {
        self.checks.iter().any(|check| check.status == Status::Warn)
    }

    /// Looks a check up by its identifier.
    #[must_use]
    pub fn check(&self, id: &str) -> Option<&Check> {
        self.checks.iter().find(|check| check.id == id)
    }
}

/// What the caller wants the run to cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Run checks that open network connections. Off by default: `doctor` must
    /// be usable on an air-gapped machine without hanging on a DNS timeout.
    pub online: bool,
    /// Prove directories are writable by round-tripping a probe file. On by
    /// default; a caller inspecting a foreign machine's configuration can turn
    /// it off to keep the run read-only.
    pub write_probe: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            online: false,
            write_probe: true,
        }
    }
}

/// Runs every check against an already-loaded configuration.
///
/// Takes [`Loaded`] rather than loading it internally so that the report can
/// describe the same configuration the rest of the command is using -- including
/// the `--config` and `-v` flags the user passed.
#[must_use]
pub fn run(loaded: &Loaded, options: Options) -> Report {
    let started = Instant::now();

    let mut report_checks = Vec::with_capacity(12);
    report_checks.push(checks::build_provenance());
    report_checks.push(checks::host_platform());
    report_checks.push(checks::host_resources());
    report_checks.extend(checks::configuration(loaded));
    report_checks.extend(checks::storage(loaded, options.write_probe));
    report_checks.push(checks::project(loaded));
    report_checks.push(checks::git());
    report_checks.push(checks::vscode());
    report_checks.push(checks::network(loaded, options.online));

    // Saturating: a duration this long means the clock moved, not that the run
    // took 584 million years.
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    // Logged as well as returned, so a `--log-file` capture of a support session
    // holds every finding even when the report itself was not saved.
    for check in &report_checks {
        tracing::debug!(
            id = check.id,
            status = %check.status,
            detail = %check.detail,
            "diagnostic complete"
        );
    }
    tracing::debug!(
        checks = report_checks.len(),
        duration_ms,
        online = options.online,
        write_probe = options.write_probe,
        "diagnostics finished"
    );

    Report {
        build: build_info(),
        checks: report_checks,
        duration_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Loader;
    use crate::paths::Paths;

    fn loaded_in(root: &std::path::Path) -> Loaded {
        let paths = Paths::new(root.join("config"), root.join("data"), root.join("cache"));
        Loader::new(root.to_path_buf(), paths)
            .load()
            .expect("load config")
    }

    #[test]
    fn a_healthy_machine_reports_no_errors() {
        let temp = tempfile::tempdir().expect("temp dir");
        let report = run(&loaded_in(temp.path()), Options::default());

        assert!(
            !report.has_errors(),
            "unexpected failures: {:#?}",
            report.checks
        );
        assert_eq!(report.exit_code(), ExitCode::Success);
        // Every check must be identifiable and explained.
        for check in &report.checks {
            assert!(!check.id.is_empty());
            assert!(!check.detail.is_empty(), "`{}` reported nothing", check.id);
            // A skipped check is self-explanatory; anything actionable is not.
            if matches!(check.status, Status::Warn | Status::Error) {
                assert!(check.hint.is_some(), "`{}` needs a hint", check.id);
            }
        }
    }

    #[test]
    fn the_report_covers_the_documented_check_ids() {
        let temp = tempfile::tempdir().expect("temp dir");
        let report = run(&loaded_in(temp.path()), Options::default());

        for id in [
            "core.build",
            "host.platform",
            "host.resources",
            "config.layers",
            "paths.config",
            "paths.data",
            "paths.cache",
            "project.root",
            "tools.git",
            "tools.vscode",
            "network.reachability",
        ] {
            assert!(report.check(id).is_some(), "missing check `{id}`");
        }
    }

    #[test]
    fn network_checks_are_skipped_unless_asked_for() {
        let temp = tempfile::tempdir().expect("temp dir");
        let report = run(&loaded_in(temp.path()), Options::default());
        let network = report.check("network.reachability").expect("network check");
        assert_eq!(network.status, Status::Skipped);
    }

    #[test]
    fn offline_configuration_keeps_network_checks_skipped_even_when_asked() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            temp.path().join(crate::paths::PROJECT_FILE),
            "[network]\noffline = true\n",
        )
        .expect("write project file");

        let report = run(
            &loaded_in(temp.path()),
            Options {
                online: true,
                write_probe: true,
            },
        );
        let network = report.check("network.reachability").expect("network check");
        // An explicit offline policy outranks `--online`: the user said never.
        assert_eq!(network.status, Status::Skipped);
        assert!(network.detail.contains("offline"), "{}", network.detail);
    }

    #[test]
    fn unwritable_storage_is_an_error_with_a_hint() {
        let temp = tempfile::tempdir().expect("temp dir");
        // A regular file where a directory must go: portable across platforms,
        // unlike chmod, and it is exactly what a stale install looks like.
        let blocker = temp.path().join("data");
        std::fs::write(&blocker, b"not a directory").expect("write blocker");

        let report = run(&loaded_in(temp.path()), Options::default());
        let data = report.check("paths.data").expect("data check");
        assert_eq!(data.status, Status::Error, "{data:#?}");
        assert!(data.hint.is_some());
        assert!(report.has_errors());
        assert_eq!(report.exit_code(), ExitCode::DiagnosticsFailed);
    }

    #[test]
    fn the_write_probe_can_be_turned_off() {
        let temp = tempfile::tempdir().expect("temp dir");
        let blocker = temp.path().join("data");
        std::fs::write(&blocker, b"not a directory").expect("write blocker");

        let report = run(
            &loaded_in(temp.path()),
            Options {
                online: false,
                write_probe: false,
            },
        );
        let data = report.check("paths.data").expect("data check");
        // Without the probe the check reports what it can see, not a failure.
        assert_ne!(data.status, Status::Error, "{data:#?}");
    }

    #[test]
    fn configuration_warnings_surface_in_the_report() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            temp.path().join(crate::paths::PROJECT_FILE),
            "[log]\nlvl = \"debug\"\n",
        )
        .expect("write project file");

        let report = run(&loaded_in(temp.path()), Options::default());
        let config = report.check("config.layers").expect("config check");
        assert_eq!(config.status, Status::Warn);
        assert!(config.detail.contains("log.lvl"), "{}", config.detail);
        // A warning alone must not fail the command.
        assert_eq!(report.exit_code(), ExitCode::Success);
        assert!(report.has_warnings());
    }

    #[test]
    fn summary_counts_add_up_to_the_check_count() {
        let temp = tempfile::tempdir().expect("temp dir");
        let report = run(&loaded_in(temp.path()), Options::default());
        let summary = report.summary();
        assert_eq!(
            summary.ok + summary.warn + summary.error + summary.skipped,
            report.checks.len()
        );
    }

    #[test]
    fn report_status_is_the_worst_check_status() {
        // Status ordering is what makes `max()` correct; assert it directly.
        assert!(Status::Error > Status::Warn);
        assert!(Status::Warn > Status::Ok);
        assert!(Status::Ok > Status::Skipped);
    }
}
