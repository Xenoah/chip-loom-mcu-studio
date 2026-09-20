# Changelog

Notable changes to Chip Loom. Format based on [Keep a Changelog](https://keepachangelog.com/1.1.0/);
versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Every version before `v1.0.0` is a **prerelease**: development history, not a
finished product. Full, bilingual notes for each published version are in
[docs/releases/](docs/releases/).

## [Unreleased]

Nothing yet. Phase 1 (target system) is next; see [docs/roadmap.md](docs/roadmap.md).

## [0.1.0-pre.0] — 2026-09-20

**Phase 0: Foundation.** The structure the rest of Chip Loom is built on. This
version does not build, flash, monitor or debug anything.

### Added

**Workspace and build**

- Cargo workspace with `chiploom-core` (the library) and `chiploom-cli` (the
  `chiploom` binary), on Rust 2024, MSRV 1.88.
- Workspace lints that forbid `unsafe` and deny `todo!()`, `unimplemented!()` and
  `dbg!()` in committed code.
- A build script capturing version, commit, dirty state, target, host, profile,
  compiler and build timestamp into the binary, honouring `SOURCE_DATE_EPOCH`.

**`chiploom-core`**

- A single error type with a variant per failure class, each mapping to a
  documented, test-asserted exit code; every failure names its subject and carries
  a remediation hint where one applies.
- Layered configuration — defaults, user-global file, project file, `CHIPLOOM_*`
  environment, command-line flags — merged as optional patches so a layer that
  says nothing about a value leaves the layer below it alone. Unknown keys are
  collected rather than rejected, for forward compatibility. Every consulted layer
  is reported, including the missing ones.
- `CHIPLOOM_CONFIG_DIR`, `CHIPLOOM_DATA_DIR` and `CHIPLOOM_CACHE_DIR` relocate the
  three storage locations on every platform. On Windows they are the only way to:
  the platform locations come from the Known Folder API, so `%APPDATA%` has no
  effect on them. Setting all three also makes Chip Loom usable on a host where no
  home directory can be determined, since it then never asks the platform; with
  only some set, it stops and names the ones missing.
- Platform path resolution for configuration, data and cache directories, with
  project discovery by walking up for `chiploom.toml`, and a writability probe that
  round-trips a file rather than merely testing for existence.
- `tracing`-based logging with compact, pretty and JSON formats, an optional JSON
  log file, and the `CHIPLOOM_LOG` filter override. Diagnostics go to stderr only,
  so stdout stays available for the IPC protocol.
- A diagnostics engine with eleven checks covering the build, the host, the
  configuration, all three storage locations, the project, `git`, the VS Code CLI
  and — on request — network reachability. A check never aborts the run, and
  anything actionable carries a hint.

**Core IPC**

- JSON-RPC 2.0 over newline-delimited stdio, protocol version 1, with `initialize`,
  `initialized`, `shutdown`, `exit`, `$/ping`, `core/version`, `core/doctor` and
  `core/config`, plus the `core/ready` notification.
- Capability-based feature discovery, so clients branch on the method list rather
  than on a version string.
- Version negotiation that refuses a mismatch and names both versions, rather than
  guessing.

**`chiploom` CLI**

- `doctor` (with `--online`, `--no-write-probe`, `--strict`), `version`,
  `config show | path | check`, `serve --stdio`, and `completions` for bash, zsh,
  fish, PowerShell and Elvish.
- Global options for configuration selection, verbosity, log format and
  destination, result format, colour and offline mode; accepted before or after the
  subcommand.
- `--format json` on every command that produces a result.
- Colour honouring `--color`, `NO_COLOR` and whether stdout is a terminal.

**VS Code extension**

- Connects to the core over the Core IPC protocol, with the handshake, a status bar
  item reporting the connected version, and an output channel carrying the core's
  stderr verbatim.
- Commands: Show Core Version, Run Diagnostics, Restart Core, Show Output.
- Settings: `chiploom.corePath`, `chiploom.autoStart`, `chiploom.logLevel`,
  `chiploom.startupTimeoutMs`.
- Binary discovery preferring the configured path, then `CHIPLOOM_BIN`, then the
  workspace `target/` directory, then `PATH`, so a contributor runs the core they
  just built; a failure lists everywhere it looked.
- Every request times out, and a core that dies rejects in-flight requests with its
  exit status and the tail of its stderr.

**Tests**

- 141 Rust tests: unit tests beside the code, and 25 integration tests that run the
  real binary and assert on exit codes and stream separation.
- 16 TypeScript tests that drive the real binary over the real protocol — no mocks,
  no editor required.
- A test that runs `serve --stdio` at `-vvv` and parses every stdout line as JSON,
  so logging can never corrupt the protocol channel.
- Both suites confine themselves to a temporary directory — the integration tests
  through Chip Loom's own `CHIPLOOM_*_DIR` overrides, which is the only approach
  that works on Windows — so no test can read, or write to, the developer's real
  configuration.
- `npm run check:tests` fails the extension suite if a compiled test file is not
  named in the test script, since Node 20 does not expand globs for `--test` and
  passing it a directory silently runs nothing.

**CI and release**

- CI on Windows, macOS and Linux: formatting, clippy with warnings denied, the Rust
  test suite, the extension type-check and protocol tests, and the three Phase 0
  acceptance commands run directly.
- A release workflow building all five target triples, verifying each native binary
  runs before packaging it, writing a SHA-256 next to every archive, building the
  `.vsix`, and publishing from `docs/releases/<tag>.md` — marked prerelease unless
  the tag is a bare `vX.Y.Z`.

**Documentation**

- Bilingual README, CONTRIBUTING, SECURITY and CODE_OF_CONDUCT.
- `docs/`: architecture, CLI reference, configuration reference, IPC protocol
  specification, development guide, roadmap and release process.
- Issue and pull request templates, and VS Code launch and task configuration.

[Unreleased]: https://github.com/Xenoah/chip-loom-mcu-studio/compare/v0.1.0-pre.0...HEAD
[0.1.0-pre.0]: https://github.com/Xenoah/chip-loom-mcu-studio/releases/tag/v0.1.0-pre.0
