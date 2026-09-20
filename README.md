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

## Roadmap

Chip Loom is built in eleven phases. Each one ends with a working, testable artifact
published as a **GitHub prerelease**, so the development history *is* the release
history. `v1.0.0` is the first finished product; everything before it is development
history, useful for following along and evaluating, not a release to depend on.

| Version | Phase | Delivers | Status |
| --- | --- | --- | --- |
| [`v0.1.0-pre.0`](https://github.com/Xenoah/chip-loom-mcu-studio/releases/tag/v0.1.0-pre.0) | 0 — Foundation | Workspace, core, CLI, extension, Core IPC protocol, CI, docs | **Released** |
| `v0.2.0-pre.0` | 1 — Target system | MCU database, target packs, board profiles, memory maps, `chiploom target` | Not started |
| `v0.3.0-pre.0` | 2 — Toolchain manager | Automatic download, SHA-256 verification, caching, version pinning, offline import | Not started |
| `v0.4.0-pre.0` | 3 — Build engine | C/C++/asm, dependency graph, parallel and incremental builds, `compile_commands.json` | Not started |
| `v0.5.0-pre.0` | 4 — Flash engine | Device detection, erase/program/verify/reset, UF2 · DFU · UART · AVR ISP · SWD · ESP ROM | Not started |
| `v0.6.0-pre.0` | 5 — UART / USB monitor | ASCII/hex/binary, CDC · HID · bulk, telemetry, graphs, MCU-side debug library | Not started |
| `v0.7.0-pre.0` | 6 — Debugger | Debug Adapter Protocol, CMSIS-DAP, breakpoints, DWARF, SVD peripheral viewer | Not started |
| `v0.8.0-pre.0` | 7 — Framework integration | Bare metal, CMSIS, STM32 HAL, Arduino Core, Pico SDK, ESP-IDF, FreeRTOS | Not started |
| `v0.9.0-pre.0` | 8 — Library / test / analysis | Library manager with lockfile, host and on-device tests, clang-tidy | Not started |
| `v0.10.0-pre.0` | 9 — Remote / OTA | Remote build · flash · monitor · debug, device sharing, OTA with rollback | Not started |
| `v1.0.0-rc.x` | 10 — Final verification | Five platforms, stress and endurance tests, no new features | Not started |
| `v1.0.0` | — | First stable release | Not started |

A phase may publish more than one prerelease (`v0.4.0-pre.1`, `-pre.2`) whenever
there is a verifiable milestone inside it.

### Progress

**1 of 11 phases complete.** By effort rather than phase count that is closer to
3–5%: Phase 0 is the foundation everything else sits on, and the heavyweight build,
flash and debug engines are all still ahead.

### What `v1.0.0` will not contain

The rule that shapes every phase, and the reason each one's completion condition is
written in terms of something you can run:

* no unimplemented buttons or commands;
* no dummy or placeholder behaviour;
* no major feature missing behind a `TODO`;
* no major feature that depends on "we will add this later";
* **nothing listed as supported that has not been verified on real hardware.**

That last point is a hard constraint on how this roadmap can advance. Phases 4, 5, 6,
7 and 10 have completion conditions that are physical — writing to real silicon,
surviving a USB unplug, driving a debug probe, running on five OS and architecture
combinations. Their code can be written and unit-tested anywhere; their support
tables cannot be filled in without the hardware in front of someone.

[docs/roadmap.md](docs/roadmap.md) has the full description of each phase.

## Documentation

| Document | Contents |
| --- | --- |
| [AGENTS.md](AGENTS.md) | **Handover**: current state, the invariants that must hold, and the traps that have already cost time. Read this first if you are picking the project up. |
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
