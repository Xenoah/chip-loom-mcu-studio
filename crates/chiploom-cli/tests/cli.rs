//! End-to-end tests for the `chiploom` binary.
//!
//! These run the real executable, so they cover the parts unit tests cannot:
//! process exit codes, what lands on stdout versus stderr, and whether the three
//! commands Phase 0 promises actually work when typed.
//!
//! Every test runs against a throwaway home directory. Without that, a developer
//! with a `~/.config/chiploom/config.toml` would see different results from CI,
//! which is exactly the class of bug this suite exists to catch.

use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

/// Builds an invocation whose configuration cannot come from the real machine.
///
/// Isolation goes through Chip Loom's own `CHIPLOOM_*_DIR` overrides rather than
/// through `HOME` and the platform variables. That is not a preference: on
/// Windows the platform locations come from the Known Folder API, so overriding
/// `%APPDATA%` does nothing and a test that relied on it would read -- and write
/// to -- the real user profile. Using the documented overrides also means these
/// tests exercise the mechanism a CI user would actually reach for.
fn chiploom(home: &Path) -> Command {
    let mut command = Command::cargo_bin("chiploom").expect("the chiploom binary must be built");
    command
        // The overrides that work on every platform.
        .env("CHIPLOOM_CONFIG_DIR", home.join("config"))
        .env("CHIPLOOM_DATA_DIR", home.join("data"))
        .env("CHIPLOOM_CACHE_DIR", home.join("cache"))
        // Deliberately *not* overriding HOME, USERPROFILE or the XDG variables.
        // Platform discovery is left working and the overrides above win over it,
        // which is the path a real user takes. Redirecting USERPROFILE on Windows
        // breaks discovery outright, and a suite that depended on that would be
        // testing the failure path and nothing else.
        //
        // Nothing else the developer exported may leak in.
        .env_remove("CHIPLOOM_CONFIG")
        .env_remove("CHIPLOOM_LOG")
        .env_remove("CHIPLOOM_LOG_LEVEL")
        .env_remove("CHIPLOOM_LOG_FORMAT")
        .env_remove("CHIPLOOM_LOG_FILE")
        .env_remove("CHIPLOOM_OFFLINE")
        .env_remove("CHIPLOOM_NETWORK_TIMEOUT")
        .env_remove("CHIPLOOM_NETWORK_RETRIES")
        // Keeps assertions on output free of escape sequences.
        .env("NO_COLOR", "1")
        .current_dir(home);
    command
}

/// The user-global configuration file inside an isolated home.
fn global_config(home: &Path) -> std::path::PathBuf {
    home.join("config/config.toml")
}

fn home() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir")
}

// ---------------------------------------------------------------------------
// The three commands Phase 0 must deliver.
// ---------------------------------------------------------------------------

#[test]
fn version_prints_one_parsable_line() {
    let home = home();
    let output = chiploom(home.path()).arg("--version").assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf-8");

    assert_eq!(stdout.trim().lines().count(), 1, "{stdout}");
    let mut fields = stdout.trim().split(' ');
    assert_eq!(fields.next(), Some("chiploom"));
    // `chiploom --version | cut -d' ' -f2` has to yield a version.
    assert_eq!(fields.next(), Some(env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_lists_every_command() {
    let home = home();
    chiploom(home.path())
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("version"))
        .stdout(predicate::str::contains("config"))
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("completions"));
}

#[test]
fn short_help_also_works() {
    let home = home();
    chiploom(home.path())
        .arg("-h")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn doctor_succeeds_on_a_clean_machine() {
    let home = home();
    chiploom(home.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Chip Loom diagnostics"))
        .stdout(predicate::str::contains("Ready."));
}

#[test]
fn doctor_creates_the_directories_it_reports_on() {
    let home = home();
    chiploom(home.path()).arg("doctor").assert().success();

    // The write probe is what makes `doctor` meaningful, and it leaves the
    // directory tree in place rather than merely guessing about it.
    for dir in ["config", "data", "cache"] {
        assert!(home.path().join(dir).is_dir(), "`{dir}` was not created");
    }
}

#[test]
fn doctor_emits_valid_json() {
    let home = home();
    let output = chiploom(home.path())
        .args(["doctor", "--format", "json"])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf-8");

    let report: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be JSON");
    assert!(report["checks"].as_array().expect("checks").len() >= 10);
    assert_eq!(report["build"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(report["durationMs"].is_u64());
    // JSON output must not carry the human report as well.
    assert!(!stdout.contains("Chip Loom diagnostics"), "{stdout}");
}

#[test]
fn doctor_skips_network_checks_unless_asked() {
    let home = home();
    let output = chiploom(home.path())
        .args(["doctor", "--format", "json"])
        .assert()
        .success();
    let report: serde_json::Value =
        serde_json::from_slice(&output.get_output().stdout).expect("JSON");

    let network = report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["id"] == "network.reachability")
        .expect("network check");
    assert_eq!(network["status"], "skipped");
}

#[test]
fn doctor_fails_with_code_seven_when_a_directory_is_unusable() {
    let home = home();
    // A file where the data directory must go. Portable, and exactly what a
    // half-removed installation looks like.
    std::fs::write(home.path().join("data"), b"not a directory").expect("write blocker");

    chiploom(home.path())
        .arg("doctor")
        .assert()
        .code(7)
        .stdout(predicate::str::contains("fail"))
        .stdout(predicate::str::contains("cannot run Chip Loom correctly"));
}

#[test]
fn doctor_strict_turns_warnings_into_a_failure() {
    let home = home();
    std::fs::write(
        home.path().join("chiploom.toml"),
        "[log]\nlvl = \"debug\"\n",
    )
    .expect("write project file");

    // Without --strict an unknown key is only a warning.
    chiploom(home.path()).arg("doctor").assert().success();
    chiploom(home.path())
        .args(["doctor", "--strict"])
        .assert()
        .code(7)
        .stdout(predicate::str::contains("--strict"));
}

// ---------------------------------------------------------------------------
// Configuration.
// ---------------------------------------------------------------------------

#[test]
fn config_show_emits_toml_that_can_be_written_back() {
    let home = home();
    std::fs::write(
        home.path().join("chiploom.toml"),
        "[project]\nname = \"blinky\"\n",
    )
    .expect("write project file");

    let output = chiploom(home.path())
        .args(["config", "show"])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf-8");

    let parsed: toml::Value = toml::from_str(&stdout).expect("output must be valid TOML");
    assert_eq!(parsed["project"]["name"].as_str(), Some("blinky"));
}

#[test]
fn config_path_reports_the_isolated_home() {
    let home = home();
    let output = chiploom(home.path())
        .args(["config", "path"])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf-8");

    // Proves the isolation these tests depend on is actually in effect.
    assert!(
        stdout.contains(&home.path().display().to_string()),
        "{stdout}"
    );
    assert!(stdout.contains("toolchains"), "{stdout}");
}

#[test]
fn config_check_reports_unknown_keys_without_failing() {
    let home = home();
    std::fs::write(
        home.path().join("chiploom.toml"),
        "[network]\nproxy = \"http://nope\"\n",
    )
    .expect("write project file");

    chiploom(home.path())
        .args(["config", "check"])
        .assert()
        // A project that mentions a key a newer Chip Loom added must not fail CI.
        .success()
        .stdout(predicate::str::contains("network.proxy"));
}

#[test]
fn an_invalid_configuration_file_fails_with_code_three_and_names_the_file() {
    let home = home();
    std::fs::write(
        home.path().join("chiploom.toml"),
        "[log\nlevel = \"debug\"\n",
    )
    .expect("write project file");

    chiploom(home.path())
        .args(["config", "show"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("chiploom.toml"))
        .stderr(predicate::str::contains("hint:"));
}

#[test]
fn a_config_file_named_on_the_command_line_must_exist() {
    let home = home();
    chiploom(home.path())
        .args(["--config", "definitely-absent.toml", "config", "show"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn an_unparsable_environment_variable_fails_with_code_three() {
    let home = home();
    chiploom(home.path())
        .env("CHIPLOOM_OFFLINE", "yse")
        .args(["config", "show"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("CHIPLOOM_OFFLINE"));
}

#[test]
fn the_directory_flag_changes_where_the_project_is_found() {
    let home = home();
    let project = home.path().join("workspace/blinky");
    std::fs::create_dir_all(&project).expect("create project dir");
    std::fs::write(
        project.join("chiploom.toml"),
        "[project]\nname = \"from-C-flag\"\n",
    )
    .expect("write project file");

    chiploom(home.path())
        .args([
            "-C",
            project.to_str().expect("utf-8 path"),
            "config",
            "show",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("from-C-flag"));
}

#[test]
fn no_global_config_ignores_the_user_file() {
    let home = home();
    let global = global_config(home.path());
    std::fs::create_dir_all(global.parent().expect("parent")).expect("create config dir");
    std::fs::write(&global, "[log]\nlevel = \"trace\"\n").expect("write global config");

    chiploom(home.path())
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("level = \"trace\""));

    chiploom(home.path())
        .args(["--no-global-config", "config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("level = \"info\""));
}

// ---------------------------------------------------------------------------
// Usage errors and the IPC server.
// ---------------------------------------------------------------------------

#[test]
fn the_config_dir_override_relocates_the_user_global_file() {
    let home = home();
    let elsewhere = home.path().join("ci-config");
    std::fs::create_dir_all(&elsewhere).expect("create dir");
    std::fs::write(elsewhere.join("config.toml"), "[network]\nretries = 11\n")
        .expect("write config");

    chiploom(home.path())
        .env("CHIPLOOM_CONFIG_DIR", &elsewhere)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("retries = 11"));

    // And it is reported, so `config path` stays truthful about where it looked.
    chiploom(home.path())
        .env("CHIPLOOM_CONFIG_DIR", &elsewhere)
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ci-config"));
}

#[test]
fn an_unknown_subcommand_is_a_usage_error() {
    let home = home();
    chiploom(home.path()).arg("teleport").assert().code(2);
}

#[test]
fn serve_without_a_transport_is_refused() {
    let home = home();
    chiploom(home.path())
        .arg("serve")
        .assert()
        .code(8)
        .stderr(predicate::str::contains("--stdio"));
}

#[test]
fn serve_stdio_completes_a_session() {
    let home = home();
    let session = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientName":"integration-test","protocolVersion":1}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"core/version"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"shutdown"}"#,
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    ]
    .join("\n");

    let output = chiploom(home.path())
        .args(["serve", "--stdio"])
        .write_stdin(format!("{session}\n"))
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf-8");
    let frames: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("every stdout line must be a JSON frame"))
        .collect();

    assert_eq!(frames.len(), 3, "{stdout}");
    assert_eq!(frames[0]["result"]["serverName"], "chiploom-core");
    assert_eq!(frames[1]["result"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(frames[2]["result"].is_null());
}

#[test]
fn serve_keeps_stdout_free_of_logs_even_at_maximum_verbosity() {
    let home = home();
    // The single most important property of the transport: `-vvv` produces a lot
    // of logging, and none of it may reach the protocol channel.
    let output = chiploom(home.path())
        .args(["-vvv", "serve", "--stdio"])
        .write_stdin(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\
             \"params\":{\"clientName\":\"noisy\",\"protocolVersion\":1}}\n\
             {\"jsonrpc\":\"2.0\",\"method\":\"exit\"}\n",
        )
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).expect("utf-8");
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|err| panic!("non-protocol data on stdout: {line:?} ({err})"));
    }

    let stderr = String::from_utf8(output.get_output().stderr.clone()).expect("utf-8");
    assert!(
        !stderr.trim().is_empty(),
        "-vvv should have produced logs on stderr"
    );
}

#[test]
fn completions_are_generated_for_supported_shells() {
    let home = home();
    for shell in ["bash", "zsh", "fish", "powershell"] {
        chiploom(home.path())
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains("chiploom"));
    }
}

#[test]
fn a_log_file_receives_json_records() {
    let home = home();
    let log = home.path().join("run.log");

    chiploom(home.path())
        .args([
            "-v",
            "--log-file",
            log.to_str().expect("utf-8 path"),
            "doctor",
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&log).expect("log file must exist");
    assert!(!content.trim().is_empty(), "the log file is empty");
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|err| panic!("log line is not JSON: {line:?} ({err})"));
    }
}

#[test]
fn the_binary_does_not_hang_when_stdin_is_closed() {
    let home = home();
    // An editor that is killed closes the pipe; the core must exit, not spin.
    chiploom(home.path())
        .args(["serve", "--stdio"])
        .write_stdin("")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success();
}
