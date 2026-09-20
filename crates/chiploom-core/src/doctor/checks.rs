//! The individual diagnostics.
//!
//! Each function returns a [`Check`] (or several) and swallows its own failures
//! into [`Status::Error`]. None of them may panic or propagate.

use std::fmt::Write as _;
use std::path::Path;

use super::Check;
#[cfg(test)]
use super::Status;
use crate::config::{Loaded, SourceKind, SourceStatus};
use crate::paths::Paths;
use crate::version::build_info;

/// Which host Chip Loom would download toolchains for, in Phase 2's terms.
/// Reported here so a bug report says it without being asked.
pub(super) fn build_provenance() -> Check {
    let info = build_info();
    let mut detail = format!(
        "chiploom {} for {} ({} build), protocol v{}",
        info.version, info.target, info.profile, info.protocol_version
    );
    if info.commit != "unknown" {
        let _ = write!(detail, ", commit {}", info.commit);
    }

    if info.dirty {
        Check::warn("core.build", "Chip Loom build", detail).with_hint(
            "This binary was built from a working tree with uncommitted changes, so its \
             behaviour may not match any released version.",
        )
    } else {
        Check::ok("core.build", "Chip Loom build", detail)
    }
}

/// Operating system identity, in enough detail to reproduce a report.
pub(super) fn host_platform() -> Check {
    let name = sysinfo::System::name().unwrap_or_else(|| std::env::consts::OS.to_owned());
    let version = sysinfo::System::os_version().unwrap_or_else(|| "unknown version".to_owned());
    let kernel = sysinfo::System::kernel_version().unwrap_or_else(|| "unknown".to_owned());

    Check::ok(
        "host.platform",
        "Host platform",
        format!(
            "{name} {version} (kernel {kernel}), {} on {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    )
}

/// CPU and memory, because parallel builds and linking are sized from these.
pub(super) fn host_resources() -> Check {
    let parallelism = std::thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get);

    let mut system = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing()
            .with_memory(sysinfo::MemoryRefreshKind::nothing().with_ram()),
    );
    system.refresh_memory();
    let total_mib = system.total_memory() / (1024 * 1024);

    let detail = if total_mib == 0 {
        format!("{parallelism} usable CPU threads; memory size unavailable")
    } else {
        format!("{parallelism} usable CPU threads, {total_mib} MiB RAM")
    };

    // Linking a firmware image is small work; the floor is about being able to
    // unpack a toolchain at all.
    if total_mib > 0 && total_mib < 1024 {
        Check::warn("host.resources", "Host resources", detail).with_hint(
            "Under 1 GiB of RAM: unpacking a toolchain or linking a large image may fail. \
             Close other work before building.",
        )
    } else if parallelism == 0 {
        Check::warn("host.resources", "Host resources", detail)
            .with_hint("Chip Loom could not read the CPU count and will build single-threaded.")
    } else {
        Check::ok("host.resources", "Host resources", detail)
    }
}

/// Which configuration layers were consulted, and whether any key was ignored.
pub(super) fn configuration(loaded: &Loaded) -> Vec<Check> {
    let applied: Vec<String> = loaded
        .sources
        .iter()
        .filter(|source| source.status == SourceStatus::Applied)
        .map(describe_source)
        .collect();

    let mut detail = format!("layers applied: {}", applied.join(", "));
    let _ = write!(
        detail,
        "; effective log level {} ({})",
        loaded.config.log.level, loaded.config.log.format
    );
    if loaded.config.network.offline {
        detail.push_str("; network access is disabled by configuration");
    }

    let check = if loaded.warnings.is_empty() {
        Check::ok("config.layers", "Configuration", detail)
    } else {
        let ignored: Vec<&str> = loaded
            .warnings
            .iter()
            .map(|warning| warning.message.as_str())
            .collect();
        Check::warn(
            "config.layers",
            "Configuration",
            format!("{detail}. {}", ignored.join(". ")),
        )
        .with_hint(
            "Unrecognised keys have no effect. Remove them, or check the spelling against \
             `docs/configuration.md`.",
        )
    };

    vec![check]
}

fn describe_source(source: &crate::config::Source) -> String {
    let label = match source.kind {
        SourceKind::Defaults => "defaults",
        SourceKind::GlobalFile => "global file",
        SourceKind::ProjectFile => "project file",
        SourceKind::ExplicitFile => "explicit file",
        SourceKind::Environment => "environment",
        SourceKind::CommandLine => "command line",
    };
    match source.path.as_deref() {
        Some(path) => format!("{label} ({})", path.display()),
        None => label.to_owned(),
    }
}

/// The three storage locations, each with an optional write probe.
pub(super) fn storage(loaded: &Loaded, write_probe: bool) -> Vec<Check> {
    let paths = &loaded.paths;
    vec![
        storage_check(
            "paths.config",
            "Configuration directory",
            paths.config_dir(),
            write_probe,
            "configuration",
        ),
        storage_check(
            "paths.data",
            "Data directory",
            paths.data_dir(),
            write_probe,
            "toolchains and target packs",
        ),
        storage_check(
            "paths.cache",
            "Cache directory",
            paths.cache_dir(),
            write_probe,
            "downloads, build caches and logs",
        ),
    ]
}

fn storage_check(
    id: &'static str,
    title: &'static str,
    dir: &Path,
    write_probe: bool,
    purpose: &str,
) -> Check {
    let shown = dir.display().to_string();

    if !write_probe {
        let state = if dir.is_dir() {
            "exists (not probed for writability)"
        } else {
            "does not exist yet (not probed)"
        };
        return Check::skipped(id, title, format!("{shown} -- {state}"));
    }

    match Paths::ensure_writable(dir) {
        Ok(()) => Check::ok(id, title, format!("{shown} is writable")),
        Err(err) => {
            Check::error(id, title, format!("{shown} is not usable: {err}")).with_hint(format!(
                "Chip Loom stores {purpose} here. Fix the permissions, or point it elsewhere \
                 with `paths` in chiploom.toml or the CHIPLOOM_DATA_DIR / CHIPLOOM_CACHE_DIR \
                 environment variables."
            ))
        }
    }
}

/// Whether this command is running inside a Chip Loom project.
pub(super) fn project(loaded: &Loaded) -> Check {
    match loaded.paths.project_root() {
        Some(root) => {
            let name = loaded
                .config
                .project
                .name
                .clone()
                .or_else(|| {
                    root.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "unnamed".to_owned());
            Check::ok(
                "project.root",
                "Project",
                format!("`{name}` at {}", root.display()),
            )
        }
        // Not being in a project is normal for `doctor`, `target list` and friends.
        None => Check::skipped(
            "project.root",
            "Project",
            format!(
                "no {} found in this directory or any parent",
                crate::paths::PROJECT_FILE
            ),
        ),
    }
}

/// `git`, needed from Phase 8 for Git-sourced libraries.
pub(super) fn git() -> Check {
    match which::which("git") {
        Ok(path) => Check::ok("tools.git", "git", format!("found at {}", path.display())),
        Err(_) => Check::warn("tools.git", "git", "not found on PATH").with_hint(
            "Chip Loom builds and flashes without git. It is needed only for libraries \
             fetched from a Git repository.",
        ),
    }
}

/// The VS Code CLI. Absence is not a problem -- the extension works without the
/// `code` launcher, and plenty of users only ever touch the CLI.
pub(super) fn vscode() -> Check {
    for candidate in ["code", "code-insiders", "codium"] {
        if let Ok(path) = which::which(candidate) {
            return Check::ok(
                "tools.vscode",
                "VS Code CLI",
                format!("`{candidate}` found at {}", path.display()),
            );
        }
    }
    Check::skipped(
        "tools.vscode",
        "VS Code CLI",
        "no `code` launcher found on PATH",
    )
}

/// Reachability of the hosts Chip Loom downloads from, only when asked.
pub(super) fn network(loaded: &Loaded, online: bool) -> Check {
    const HOSTS: [(&str, u16); 2] = [("github.com", 443), ("crates.io", 443)];

    if loaded.config.network.offline {
        return Check::skipped(
            "network.reachability",
            "Network",
            "skipped: this configuration sets `network.offline = true`",
        );
    }
    if !online {
        return Check::skipped(
            "network.reachability",
            "Network",
            "skipped: pass `--online` to test connectivity",
        );
    }

    let timeout = loaded.config.network.timeout;
    let mut reachable = Vec::new();
    let mut failures = Vec::new();

    for (host, port) in HOSTS {
        match probe_tcp(host, port, timeout) {
            Ok(()) => reachable.push(format!("{host}:{port}")),
            Err(message) => failures.push(format!("{host}:{port} ({message})")),
        }
    }

    if failures.is_empty() {
        Check::ok(
            "network.reachability",
            "Network",
            format!("reached {}", reachable.join(", ")),
        )
    } else if reachable.is_empty() {
        Check::error(
            "network.reachability",
            "Network",
            format!("could not reach {}", failures.join(", ")),
        )
        .with_hint(
            "Chip Loom needs outbound HTTPS to download toolchains. Behind a proxy, set \
             HTTPS_PROXY. To work entirely offline, set `network.offline = true` and import \
             toolchains from an offline package.",
        )
    } else {
        Check::warn(
            "network.reachability",
            "Network",
            format!(
                "reached {}; could not reach {}",
                reachable.join(", "),
                failures.join(", ")
            ),
        )
        .with_hint("Some downloads may fail. Check DNS and any proxy configuration.")
    }
}

/// A TCP connect is the cheapest honest reachability test: it needs no HTTP
/// client, and it fails fast rather than hanging on a black-holed route.
fn probe_tcp(host: &str, port: u16, timeout: std::time::Duration) -> Result<(), String> {
    use std::net::ToSocketAddrs;

    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|err| format!("name lookup failed: {err}"))?
        .collect::<Vec<_>>();

    let mut last_error = "no addresses returned".to_owned();
    for address in addresses {
        match std::net::TcpStream::connect_timeout(&address, timeout) {
            Ok(_) => return Ok(()),
            Err(err) => last_error = err.to_string(),
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_and_resource_checks_never_fail() {
        // These two run on every machine, including CI containers with odd
        // /proc mounts. They may warn, but they must not error or panic.
        assert_ne!(host_platform().status, Status::Error);
        assert_ne!(host_resources().status, Status::Error);
    }

    #[test]
    fn the_platform_check_names_the_architecture() {
        let check = host_platform();
        assert!(
            check.detail.contains(std::env::consts::ARCH),
            "{}",
            check.detail
        );
        assert!(
            check.detail.contains(std::env::consts::OS),
            "{}",
            check.detail
        );
    }

    #[test]
    fn the_build_check_reports_this_binarys_version() {
        let check = build_provenance();
        assert!(
            check.detail.contains(crate::version::VERSION),
            "{}",
            check.detail
        );
    }

    #[test]
    fn a_tcp_probe_to_an_unroutable_address_fails_rather_than_hanging() {
        let started = std::time::Instant::now();
        let result = probe_tcp(
            "this-host-does-not-exist.invalid",
            443,
            std::time::Duration::from_millis(500),
        );
        assert!(result.is_err());
        // `.invalid` never resolves, so this must be bounded by DNS, not by the
        // connect timeout multiplied by an address list.
        assert!(started.elapsed() < std::time::Duration::from_secs(30));
    }

    #[test]
    fn the_vscode_check_never_errors() {
        // Absence of an editor is not a diagnostic failure for a CLI-first tool.
        assert_ne!(vscode().status, Status::Error);
    }
}
