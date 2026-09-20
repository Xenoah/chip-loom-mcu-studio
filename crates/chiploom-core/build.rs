//! Captures build-time provenance so `chiploom --version` can identify the
//! exact source revision a binary was produced from.
//!
//! Every value degrades gracefully: a build from a source tarball with no git
//! metadata still succeeds, it simply reports `unknown` for the revision.

use std::process::Command;

fn main() {
    // Only the values we actually read are emitted, so a stale environment
    // cannot silently leak into the binary.
    emit("CHIPLOOM_BUILD_TARGET", std::env::var("TARGET").ok());
    emit("CHIPLOOM_BUILD_PROFILE", std::env::var("PROFILE").ok());
    emit("CHIPLOOM_BUILD_HOST", std::env::var("HOST").ok());
    emit("CHIPLOOM_BUILD_RUSTC", rustc_version());
    emit(
        "CHIPLOOM_GIT_SHA",
        git(&["rev-parse", "--short=12", "HEAD"]),
    );
    emit("CHIPLOOM_GIT_DIRTY", git_dirty());
    emit("CHIPLOOM_BUILD_TIMESTAMP", timestamp());

    // Rebuild when the revision moves, but never fail if .git is absent.
    println!("cargo:rerun-if-changed=build.rs");
    for path in ["../../.git/HEAD", "../../.git/index"] {
        if std::path::Path::new(path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    println!("cargo:rerun-if-env-changed=CHIPLOOM_BUILD_TIMESTAMP");
}

fn emit(key: &str, value: Option<String>) {
    let value = value.unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env={key}={value}");
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

fn git_dirty() -> Option<String> {
    let out = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(
        if out.stdout.is_empty() {
            "false"
        } else {
            "true"
        }
        .to_owned(),
    )
}

fn rustc_version() -> Option<String> {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let out = Command::new(rustc).arg("--version").output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Reproducible builds set `SOURCE_DATE_EPOCH`; otherwise fall back to now.
fn timestamp() -> Option<String> {
    if let Ok(existing) = std::env::var("CHIPLOOM_BUILD_TIMESTAMP") {
        return Some(existing);
    }
    let secs: u64 = match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(v) => v.trim().parse().ok()?,
        Err(_) => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs(),
    };
    Some(format_rfc3339(secs))
}

/// Formats seconds-since-epoch as RFC 3339 UTC without pulling in a date crate.
fn format_rfc3339(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days, Howard Hinnant's algorithm.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}
