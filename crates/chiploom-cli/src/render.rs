//! Turning core values into terminal output.
//!
//! All human-facing formatting lives here, which is the other half of the rule
//! that `chiploom-core` never prints: the core produces a
//! [`chiploom_core::doctor::Report`], and this module decides what that looks
//! like for a person. The JSON form is produced by the same values' `Serialize`
//! implementations, so the two can never drift apart in content.
//!
//! Everything writes into a `&mut impl Write` rather than to stdout directly, so
//! the tests below assert on real rendered output.

use std::io::Write;

use chiploom_core::config::{Loaded, SourceKind, SourceStatus};
use chiploom_core::doctor::{Report, Status};
use chiploom_core::version::BuildInfo;

use crate::cli::ColorChoice;

/// ANSI styling, disabled as a whole when the terminal cannot use it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Palette {
    enabled: bool,
}

impl Palette {
    /// Decides whether to colour, honouring `--color`, `NO_COLOR` and whether
    /// stdout is a terminal.
    #[must_use]
    pub(crate) fn resolve(
        choice: ColorChoice,
        stdout_is_terminal: bool,
        no_color_set: bool,
    ) -> Self {
        let enabled = match choice {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            // https://no-color.org: any value, even empty, disables colour.
            ColorChoice::Auto => stdout_is_terminal && !no_color_set,
        };
        Self { enabled }
    }

    /// A palette that emits no escape sequences, so rendering tests can assert
    /// on exactly the text a piped run produces.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn plain() -> Self {
        Self { enabled: false }
    }

    fn paint(self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\u{1b}[{code}m{text}\u{1b}[0m")
        } else {
            text.to_owned()
        }
    }

    fn bold(self, text: &str) -> String {
        self.paint("1", text)
    }

    fn dim(self, text: &str) -> String {
        self.paint("2", text)
    }

    fn status(self, status: Status) -> String {
        let code = match status {
            Status::Ok => "32",     // green
            Status::Warn => "33",   // yellow
            Status::Error => "31",  // red
            Status::Skipped => "2", // dim
        };
        // Fixed width so the report reads as a column.
        self.paint(code, &format!("{:<4}", status.marker()))
    }
}

/// Writes a diagnostics report as terminal text.
///
/// # Errors
/// Fails only if the output stream fails.
pub(crate) fn doctor_report<W: Write>(
    writer: &mut W,
    report: &Report,
    palette: Palette,
    strict: bool,
) -> std::io::Result<()> {
    writeln!(writer, "{}", palette.bold("Chip Loom diagnostics"))?;
    writeln!(writer)?;

    let width = report
        .checks
        .iter()
        .map(|check| check.title.len())
        .max()
        .unwrap_or(0);

    for check in &report.checks {
        writeln!(
            writer,
            "  {} {:<width$}  {}",
            palette.status(check.status),
            check.title,
            check.detail,
            width = width
        )?;
        if let Some(hint) = &check.hint {
            // Indented under the finding it belongs to, and dimmed, so a clean
            // report stays scannable while a broken one explains itself. Hints are
            // full sentences, so they are wrapped rather than left to run off the
            // edge of the terminal.
            let indent = width + 9;
            for line in wrap(hint, terminal_width().saturating_sub(indent).max(40)) {
                writeln!(writer, "{:indent$}{}", "", palette.dim(&line))?;
            }
        }
    }

    let summary = report.summary();
    writeln!(writer)?;
    writeln!(
        writer,
        "  {} passed, {} warning{}, {} failure{}, {} skipped   {}",
        summary.ok,
        summary.warn,
        plural(summary.warn),
        summary.error,
        plural(summary.error),
        summary.skipped,
        palette.dim(&format_duration(report.duration_ms)),
    )?;

    let verdict = if report.has_errors() {
        "This machine cannot run Chip Loom correctly yet. Fix the failures above."
    } else if strict && report.has_warnings() {
        "No failures, but --strict treats the warnings above as errors."
    } else if report.has_warnings() {
        "Ready. The warnings above are safe to ignore for now."
    } else {
        "Ready."
    };
    writeln!(writer, "  {verdict}")?;

    Ok(())
}

/// Writes build provenance as terminal text.
///
/// # Errors
/// Fails only if the output stream fails.
pub(crate) fn version<W: Write>(
    writer: &mut W,
    info: &BuildInfo,
    verbose: bool,
    palette: Palette,
) -> std::io::Result<()> {
    if !verbose {
        return writeln!(writer, "{}", info.short_line());
    }

    let detailed = info.detailed();
    let width = detailed.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
    for (key, value) in detailed {
        writeln!(
            writer,
            "{:<width$}  {value}",
            palette.bold(key),
            width = width
        )?;
    }
    Ok(())
}

/// Writes the effective configuration as TOML, optionally with provenance.
///
/// TOML rather than an invented layout: the output can be pasted straight into
/// a `chiploom.toml`.
///
/// # Errors
/// Fails if the output stream fails or the configuration cannot be serialized.
pub(crate) fn config_show<W: Write>(
    writer: &mut W,
    loaded: &Loaded,
    with_sources: bool,
    palette: Palette,
) -> Result<(), Box<dyn std::error::Error>> {
    let toml = strip_empty_tables(&toml_of(&loaded.config)?);
    write!(writer, "{toml}")?;

    if with_sources {
        writeln!(writer)?;
        writeln!(
            writer,
            "{}",
            palette.bold("# layers, lowest precedence first")
        )?;
        for source in &loaded.sources {
            writeln!(writer, "#   {}", palette.dim(&describe_source(source)))?;
        }
    }

    Ok(())
}

/// Writes every location configuration is read from.
///
/// # Errors
/// Fails only if the output stream fails.
pub(crate) fn config_path<W: Write>(
    writer: &mut W,
    loaded: &Loaded,
    palette: Palette,
) -> std::io::Result<()> {
    writeln!(
        writer,
        "{}",
        palette.bold("configuration layers, lowest precedence first")
    )?;
    for source in &loaded.sources {
        writeln!(writer, "  {}", describe_source(source))?;
    }
    writeln!(writer)?;
    writeln!(writer, "{}", palette.bold("storage"))?;
    let paths = &loaded.paths;
    let rows = [
        ("config", paths.config_dir().display().to_string()),
        ("data", paths.data_dir().display().to_string()),
        ("toolchains", paths.toolchains_dir().display().to_string()),
        (
            "target packs",
            paths.target_packs_dir().display().to_string(),
        ),
        ("cache", paths.cache_dir().display().to_string()),
        ("logs", paths.log_dir().display().to_string()),
        (
            "project",
            paths
                .project_root()
                .map_or_else(|| "(none)".to_owned(), |root| root.display().to_string()),
        ),
    ];
    let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
    for (label, value) in rows {
        writeln!(writer, "  {label:<width$}  {value}")?;
    }
    Ok(())
}

/// Writes the result of validating configuration.
///
/// # Errors
/// Fails only if the output stream fails.
pub(crate) fn config_check<W: Write>(
    writer: &mut W,
    loaded: &Loaded,
    palette: Palette,
) -> std::io::Result<()> {
    let files: Vec<_> = loaded
        .sources
        .iter()
        .filter(|source| source.path.is_some() && source.status == SourceStatus::Applied)
        .collect();

    if files.is_empty() {
        writeln!(
            writer,
            "No configuration files found; Chip Loom is using its defaults."
        )?;
    } else {
        for source in files {
            writeln!(
                writer,
                "{} {}",
                palette.status(Status::Ok),
                describe_source(source)
            )?;
        }
    }

    if loaded.warnings.is_empty() {
        writeln!(writer, "\nNo problems found.")?;
    } else {
        writeln!(writer)?;
        for warning in &loaded.warnings {
            writeln!(
                writer,
                "{} {}",
                palette.status(Status::Warn),
                warning.message
            )?;
        }
        writeln!(
            writer,
            "\n{} warning{} found. Unrecognised keys have no effect.",
            loaded.warnings.len(),
            plural(loaded.warnings.len())
        )?;
    }
    Ok(())
}

/// Serializes any value as a pretty JSON document with a trailing newline.
///
/// # Errors
/// Fails if the output stream fails or the value cannot be serialized.
pub(crate) fn json<W: Write, T: serde::Serialize>(
    writer: &mut W,
    value: &T,
) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    writeln!(writer, "{text}")
}

fn toml_of<T: serde::Serialize>(value: &T) -> Result<String, Box<dyn std::error::Error>> {
    Ok(toml::to_string_pretty(value)?)
}

/// Drops table headers that ended up with no keys under them.
///
/// A section whose every value is unset -- `[project]` on a machine with no
/// project -- serializes to a bare header. Valid TOML, but it reads as though
/// something is missing, so it is removed before the output is shown.
fn strip_empty_tables(toml: &str) -> String {
    let lines: Vec<&str> = toml.lines().collect();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());

    for (index, line) in lines.iter().enumerate() {
        let is_header = line.trim_start().starts_with('[');
        if is_header {
            // Keep the header only if a key appears before the next header.
            let has_keys = lines[index + 1..]
                .iter()
                .take_while(|next| !next.trim_start().starts_with('['))
                .any(|next| !next.trim().is_empty());
            if !has_keys {
                continue;
            }
        }
        kept.push(line);
    }

    // Collapse the blank lines the removed headers left behind.
    let mut out = String::with_capacity(toml.len());
    let mut previous_blank = true;
    for line in kept {
        if line.trim().is_empty() {
            if previous_blank {
                continue;
            }
            previous_blank = true;
        } else {
            previous_blank = false;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn describe_source(source: &chiploom_core::config::Source) -> String {
    let kind = match source.kind {
        SourceKind::Defaults => "built-in defaults",
        SourceKind::GlobalFile => "global file",
        SourceKind::ProjectFile => "project file",
        SourceKind::ExplicitFile => "explicit file (--config)",
        SourceKind::Environment => "CHIPLOOM_* environment variables",
        SourceKind::CommandLine => "command-line flags",
    };
    let status = match source.status {
        SourceStatus::Applied => "applied",
        SourceStatus::Missing => "not present",
        SourceStatus::Skipped => "skipped",
        SourceStatus::Empty => "present but empty",
    };
    match source.path.as_deref() {
        Some(path) => format!("{kind}: {} [{status}]", path.display()),
        None => format!("{kind} [{status}]"),
    }
}

/// The width to wrap prose at.
///
/// `COLUMNS` is the only width available without a terminal-size dependency, and
/// it is what shells export. 100 matches the width `--help` is laid out for.
fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(100)
        .clamp(40, 160)
}

/// Breaks `text` into lines of at most `width` characters, splitting on spaces.
///
/// A word longer than `width` is left on its own line rather than broken: paths
/// and identifiers read worse hyphenated than run long.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Renders a millisecond count, distinguishing "fast" from "unmeasured".
fn format_duration(millis: u64) -> String {
    if millis == 0 {
        "(under 1 ms)".to_owned()
    } else {
        format!("({millis} ms)")
    }
}

const fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chiploom_core::config::Loader;
    use chiploom_core::doctor;
    use chiploom_core::paths::Paths;

    fn loaded_in(root: &std::path::Path) -> Loaded {
        let paths = Paths::new(root.join("config"), root.join("data"), root.join("cache"));
        Loader::new(root.to_path_buf(), paths).load().expect("load")
    }

    fn render_doctor(root: &std::path::Path, strict: bool) -> String {
        let loaded = loaded_in(root);
        let report = doctor::run(&loaded, doctor::Options::default());
        let mut out = Vec::new();
        doctor_report(&mut out, &report, Palette::plain(), strict).expect("render");
        String::from_utf8(out).expect("utf-8")
    }

    #[test]
    fn a_healthy_doctor_report_says_ready_and_names_every_check() {
        let temp = tempfile::tempdir().expect("temp dir");
        let text = render_doctor(temp.path(), false);

        assert!(text.contains("Chip Loom diagnostics"), "{text}");
        assert!(text.contains("Host platform"), "{text}");
        assert!(text.contains("Data directory"), "{text}");
        assert!(text.trim_end().ends_with("Ready."), "{text}");
        assert!(text.contains("passed,"), "{text}");
    }

    #[test]
    fn a_failing_check_is_marked_and_its_hint_is_shown() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(temp.path().join("data"), b"not a directory").expect("blocker");

        let text = render_doctor(temp.path(), false);
        assert!(text.contains("fail"), "{text}");
        assert!(
            text.contains("CHIPLOOM_DATA_DIR"),
            "the hint must be rendered:\n{text}"
        );
        assert!(text.contains("cannot run Chip Loom correctly"), "{text}");
    }

    #[test]
    fn strict_mode_changes_the_verdict_for_warnings_only() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            temp.path().join(chiploom_core::paths::PROJECT_FILE),
            "[log]\nlvl = \"debug\"\n",
        )
        .expect("write project file");

        assert!(render_doctor(temp.path(), false).contains("safe to ignore"));
        assert!(render_doctor(temp.path(), true).contains("--strict"));
    }

    #[test]
    fn plain_output_contains_no_escape_sequences() {
        let temp = tempfile::tempdir().expect("temp dir");
        let text = render_doctor(temp.path(), false);
        assert!(!text.contains('\u{1b}'), "plain palette leaked ANSI codes");
    }

    #[test]
    fn colour_is_emitted_when_asked_for() {
        let palette = Palette::resolve(ColorChoice::Always, false, true);
        assert!(palette.status(Status::Error).contains('\u{1b}'));
    }

    #[test]
    fn no_color_disables_colour_on_a_terminal() {
        // https://no-color.org: the variable's presence is enough.
        let palette = Palette::resolve(ColorChoice::Auto, true, true);
        assert!(!palette.status(Status::Ok).contains('\u{1b}'));

        let palette = Palette::resolve(ColorChoice::Auto, true, false);
        assert!(palette.status(Status::Ok).contains('\u{1b}'));
    }

    #[test]
    fn a_pipe_gets_no_colour_by_default() {
        let palette = Palette::resolve(ColorChoice::Auto, false, false);
        assert!(!palette.status(Status::Ok).contains('\u{1b}'));
    }

    #[test]
    fn config_show_emits_toml_that_parses_back() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            temp.path().join(chiploom_core::paths::PROJECT_FILE),
            "[project]\nname = \"blinky\"\n[log]\nlevel = \"debug\"\n",
        )
        .expect("write project file");

        let loaded = loaded_in(temp.path());
        let mut out = Vec::new();
        config_show(&mut out, &loaded, false, Palette::plain()).expect("render");
        let text = String::from_utf8(out).expect("utf-8");

        assert!(text.contains("name = \"blinky\""), "{text}");
        assert!(text.contains("level = \"debug\""), "{text}");
        // The point of emitting TOML: it can go straight back into a file.
        let reparsed: toml::Value = toml::from_str(&text).expect("output must be valid TOML");
        assert_eq!(reparsed["project"]["name"].as_str(), Some("blinky"));
        // The network timeout uses the same key name as the file format.
        assert!(reparsed["network"]["timeout_secs"].is_integer(), "{text}");
    }

    #[test]
    fn config_show_omits_sections_that_have_no_values() {
        let temp = tempfile::tempdir().expect("temp dir");
        let loaded = loaded_in(temp.path());
        let mut out = Vec::new();
        config_show(&mut out, &loaded, false, Palette::plain()).expect("render");
        let text = String::from_utf8(out).expect("utf-8");

        // Nothing sets a project name or a path override on a bare machine.
        assert!(!text.contains("[project]"), "{text}");
        assert!(!text.contains("[paths]"), "{text}");
        // The sections that do have values are still there.
        assert!(text.contains("[log]"), "{text}");
        assert!(text.contains("[network]"), "{text}");
        toml::from_str::<toml::Value>(&text).expect("still valid TOML");
    }

    #[test]
    fn stripping_empty_tables_leaves_populated_ones_untouched() {
        let input = "[a]\n\n[b]\nkey = 1\n\n[c]\n";
        let stripped = strip_empty_tables(input);
        assert!(!stripped.contains("[a]"), "{stripped}");
        assert!(!stripped.contains("[c]"), "{stripped}");
        assert!(stripped.contains("[b]"), "{stripped}");
        assert!(stripped.contains("key = 1"), "{stripped}");
    }

    #[test]
    fn config_show_can_append_provenance_as_toml_comments() {
        let temp = tempfile::tempdir().expect("temp dir");
        let loaded = loaded_in(temp.path());
        let mut out = Vec::new();
        config_show(&mut out, &loaded, true, Palette::plain()).expect("render");
        let text = String::from_utf8(out).expect("utf-8");

        assert!(text.contains("# layers"), "{text}");
        assert!(text.contains("built-in defaults"), "{text}");
        // Comments keep the output valid TOML even with provenance attached.
        toml::from_str::<toml::Value>(&text).expect("still valid TOML");
    }

    #[test]
    fn config_path_lists_layers_and_storage() {
        let temp = tempfile::tempdir().expect("temp dir");
        let loaded = loaded_in(temp.path());
        let mut out = Vec::new();
        config_path(&mut out, &loaded, Palette::plain()).expect("render");
        let text = String::from_utf8(out).expect("utf-8");

        assert!(text.contains("global file"), "{text}");
        assert!(text.contains("not present"), "{text}");
        assert!(text.contains("toolchains"), "{text}");
        assert!(
            text.contains("(none)"),
            "no project root should be shown as none:\n{text}"
        );
    }

    #[test]
    fn config_check_reports_unknown_keys() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            temp.path().join(chiploom_core::paths::PROJECT_FILE),
            "[network]\nproxy = \"http://example.invalid\"\n",
        )
        .expect("write project file");

        let loaded = loaded_in(temp.path());
        let mut out = Vec::new();
        config_check(&mut out, &loaded, Palette::plain()).expect("render");
        let text = String::from_utf8(out).expect("utf-8");

        assert!(text.contains("network.proxy"), "{text}");
        assert!(text.contains("1 warning found"), "{text}");
    }

    #[test]
    fn config_check_on_a_clean_machine_says_so() {
        let temp = tempfile::tempdir().expect("temp dir");
        let loaded = loaded_in(temp.path());
        let mut out = Vec::new();
        config_check(&mut out, &loaded, Palette::plain()).expect("render");
        let text = String::from_utf8(out).expect("utf-8");

        assert!(text.contains("using its defaults"), "{text}");
        assert!(text.contains("No problems found"), "{text}");
    }

    #[test]
    fn the_version_line_is_one_line_and_verbose_is_many() {
        let info = chiploom_core::build_info();

        let mut out = Vec::new();
        version(&mut out, &info, false, Palette::plain()).expect("render");
        let short = String::from_utf8(out).expect("utf-8");
        assert_eq!(short.lines().count(), 1, "{short}");
        assert!(short.starts_with("chiploom "), "{short}");

        let mut out = Vec::new();
        version(&mut out, &info, true, Palette::plain()).expect("render");
        let long = String::from_utf8(out).expect("utf-8");
        assert!(long.lines().count() >= 7, "{long}");
        assert!(long.contains("rustc"), "{long}");
    }

    #[test]
    fn json_output_is_pretty_printed_and_newline_terminated() {
        let mut out = Vec::new();
        json(&mut out, &serde_json::json!({"a": 1})).expect("render");
        let text = String::from_utf8(out).expect("utf-8");
        assert!(text.ends_with("}\n"), "{text:?}");
        assert!(text.contains("\n  \"a\""), "{text:?}");
    }

    #[test]
    fn wrapping_breaks_on_spaces_and_respects_the_width() {
        let wrapped = wrap("the quick brown fox jumps over the lazy dog", 15);
        assert!(
            wrapped.iter().all(|line| line.chars().count() <= 15),
            "{wrapped:?}"
        );
        assert_eq!(
            wrapped.join(" "),
            "the quick brown fox jumps over the lazy dog"
        );
    }

    #[test]
    fn wrapping_leaves_an_overlong_word_intact() {
        // A path reads worse hyphenated than running past the margin.
        let wrapped = wrap("see /very/long/path/that/exceeds/the/width now", 10);
        assert!(
            wrapped.contains(&"/very/long/path/that/exceeds/the/width".to_owned()),
            "{wrapped:?}"
        );
    }

    #[test]
    fn wrapping_empty_text_yields_one_empty_line() {
        assert_eq!(wrap("", 10), vec![String::new()]);
    }

    #[test]
    fn a_long_hint_is_wrapped_in_the_report() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(temp.path().join("data"), b"not a directory").expect("blocker");

        let text = render_doctor(temp.path(), false);
        let hint_lines: Vec<&str> = text
            .lines()
            .filter(|line| line.contains("Chip Loom stores") || line.contains("CHIPLOOM_DATA_DIR"))
            .collect();

        assert!(
            hint_lines.len() > 1,
            "the hint should be wrapped, got:\n{text}"
        );
        // Only the hint is wrapped. A `detail` line carries a path, and a path is
        // worse to read broken across lines than running past the margin -- on a
        // machine whose temporary directory is long, it will.
        for line in &hint_lines {
            assert!(line.chars().count() <= 160, "hint line not wrapped: {line}");
        }
    }

    #[test]
    fn a_sub_millisecond_run_is_not_reported_as_zero() {
        assert_eq!(format_duration(0), "(under 1 ms)");
        assert_eq!(format_duration(1), "(1 ms)");
        assert_eq!(format_duration(1234), "(1234 ms)");
    }

    #[test]
    fn plural_matches_english() {
        assert_eq!(plural(0), "s");
        assert_eq!(plural(1), "");
        assert_eq!(plural(2), "s");
    }
}
