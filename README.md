# Chip Loom MCU Studio

[![CI](https://github.com/Xenoah/chip-loom-mcu-studio/actions/workflows/ci.yml/badge.svg)](https://github.com/Xenoah/chip-loom-mcu-studio/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

An integrated microcontroller development toolchain: build, flash, monitor and
debug firmware from one tool, on Windows, macOS and Linux.

日本語版は [README.ja.md](README.ja.md) を参照してください。

> **Status: Phase 0 of 11.** Chip Loom is being built in public, one phase at a
> time. Every published version before `v1.0.0` is a **prerelease** and exists as
> development history, not as a finished product.
>
> **This version (`v0.1.0-pre.0`) does not build, flash, monitor or debug
> anything yet.** It delivers the foundation those phases are built on: the
> workspace, the core library, the `chiploom` command, the VS Code extension and
> the protocol between them. See [docs/roadmap.md](docs/roadmap.md) for what each
> phase adds and [CHANGELOG.md](CHANGELOG.md) for what is in this one.

## What works today

```console
$ chiploom --version
chiploom 0.1.0-pre.0

$ chiploom doctor
Chip Loom diagnostics

  ok   Chip Loom build          chiploom 0.1.0-pre.0 for x86_64-unknown-linux-gnu (release build), protocol v1
  ok   Host platform            Ubuntu 24.04 (kernel 6.8.0-51-generic), linux on x86_64
  ok   Host resources           4 usable CPU threads, 16095 MiB RAM
  ok   Configuration            layers applied: defaults; effective log level info (compact)
  ok   Configuration directory  /home/you/.config/chiploom is writable
  ok   Data directory           /home/you/.local/share/chiploom is writable
  ok   Cache directory          /home/you/.cache/chiploom is writable
  skip Project                  no chiploom.toml found in this directory or any parent
  ok   git                      found at /usr/bin/git
  skip VS Code CLI              no `code` launcher found on PATH
  skip Network                  skipped: pass `--online` to test connectivity

  8 passed, 0 warnings, 0 failures, 3 skipped   (under 1 ms)
  Ready.
```

| Command | What it does |
| --- | --- |
| `chiploom --version` | The version, and the commit it was built from. |
| `chiploom --help` | Every command and option. |
| `chiploom doctor` | Checks whether this machine can run Chip Loom, and says what to fix. |
| `chiploom version -v` | Full build provenance, for bug reports. |
| `chiploom config show` | The effective configuration, as pasteable TOML. |
| `chiploom config path` | Every location configuration and data is read from. |
| `chiploom config check` | Validates configuration files and reports ignored keys. |
| `chiploom serve --stdio` | Runs the Core IPC server that editors talk to. |
| `chiploom completions <shell>` | A completion script for bash, zsh, fish, PowerShell or Elvish. |

The VS Code extension connects to the same core over the same protocol and
contributes: **Show Core Version**, **Run Diagnostics**, **Restart Core** and
**Show Output**, plus a status bar item reporting the connected core.

## Install

### From a release

Download the archive for your platform from
[Releases](https://github.com/Xenoah/chip-loom-mcu-studio/releases), verify it,
unpack it and put `chiploom` on your `PATH`:

```bash
# Linux and macOS
sha256sum -c chiploom-v0.1.0-pre.0-x86_64-unknown-linux-gnu.tar.gz.sha256
tar xzf chiploom-v0.1.0-pre.0-x86_64-unknown-linux-gnu.tar.gz
sudo install -m755 chiploom-v0.1.0-pre.0-x86_64-unknown-linux-gnu/chiploom /usr/local/bin/
chiploom doctor
```

On Windows, unpack the `.zip` and add the folder to `Path`.

### From source

Chip Loom needs a stable Rust toolchain (1.88 or newer) and, for the extension,
Node.js 20 or newer.

```bash
git clone https://github.com/Xenoah/chip-loom-mcu-studio.git
cd chip-loom-mcu-studio
cargo build --release
./target/release/chiploom doctor
```

The VS Code extension finds a binary in `target/release` or `target/debug`
automatically when the repository is the open workspace, so a contributor always
runs the core they just built.

```bash
cd extension
npm install
npm run compile
npm test          # drives the real binary over the real protocol
```

## How it fits together

Chip Loom is one core with two front ends. Nothing the editor can do is
unavailable from the command line, because both go through the same core.

```text
        ┌──────────────────────┐        ┌──────────────────────┐
        │  VS Code extension   │        │      chiploom CLI    │
        │     (TypeScript)     │        │        (Rust)        │
        └──────────┬───────────┘        └──────────┬───────────┘
                   │ JSON-RPC 2.0 over stdio       │ direct calls
                   └───────────────┬───────────────┘
                                   ▼
                        ┌─────────────────────┐
                        │    chiploom-core    │
                        │ config · logging ·  │
                        │ diagnostics · IPC   │
                        └─────────────────────┘
```

* `crates/chiploom-core` makes every decision. It never prints and never exits.
* `crates/chiploom-cli` renders those decisions for a person and picks the exit
  code.
* `extension/` speaks the protocol in `docs/ipc-protocol.md` and renders the same
  values in the editor.

[docs/architecture.md](docs/architecture.md) explains why, and what that buys.

## Configuration

Chip Loom reads, in increasing order of precedence: built-in defaults, the
user-global `config.toml`, the project's `chiploom.toml`, `CHIPLOOM_*`
environment variables, then command-line flags. A layer that says nothing about a
value leaves the layer below it alone.

```toml
# chiploom.toml, at the root of your project
[project]
name = "blinky"

[log]
level = "debug"

[network]
offline = true       # never open a network connection
```

`chiploom config path` prints where every layer lives on your machine, and
`chiploom config show --sources` prints the result with its provenance.
[docs/configuration.md](docs/configuration.md) documents every key.

## Documentation

| Document | Contents |
| --- | --- |
| [docs/architecture.md](docs/architecture.md) | How the core, CLI and extension divide the work, and why. |
| [docs/cli.md](docs/cli.md) | Every command, flag and exit code. |
| [docs/configuration.md](docs/configuration.md) | Every configuration key and environment variable. |
| [docs/ipc-protocol.md](docs/ipc-protocol.md) | The Core IPC protocol, version 1. |
| [docs/development.md](docs/development.md) | Building, testing and the repository layout. |
| [docs/roadmap.md](docs/roadmap.md) | The eleven phases and what each delivers. |
| [docs/release-process.md](docs/release-process.md) | How a phase becomes a prerelease. |
| [docs/releases/](docs/releases/) | Release notes for every published version. |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). In short: `cargo fmt`, `cargo clippy`,
`cargo test` and `npm test` all have to pass, and a change that touches hardware
behaviour needs to say which hardware it was tested on.

## License

Apache License 2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
