# Roadmap

Chip Loom is built in eleven phases. Every phase ends with a working, testable
artifact published as a **GitHub prerelease**, so the development history is the
release history.

`v1.0.0` is the first finished product. Everything before it is development
history — useful for evaluation and for following along, not a release to depend
on.

## Status

| Version | Phase | State |
| --- | --- | --- |
| [`v0.1.0-pre.0`](releases/v0.1.0-pre.0.md) | 0 — Foundation | **Released** |
| `v0.2.0-pre.0` | 1 — Target system | Not started |
| `v0.3.0-pre.0` | 2 — Toolchain manager | Not started |
| `v0.4.0-pre.0` | 3 — Build engine | Not started |
| `v0.5.0-pre.0` | 4 — Flash engine | Not started |
| `v0.6.0-pre.0` | 5 — UART and USB monitor | Not started |
| `v0.7.0-pre.0` | 6 — Debugger | Not started |
| `v0.8.0-pre.0` | 7 — Framework integration | Not started |
| `v0.9.0-pre.0` | 8 — Library, test, analysis | Not started |
| `v0.10.0-pre.0` | 9 — Remote and OTA | Not started |
| `v1.0.0-rc.x` | 10 — Final verification | Not started |
| `v1.0.0` | — | Not started |

A phase may publish more than one prerelease (`v0.4.0-pre.1`, `-pre.2`) whenever
there is a verifiable milestone inside it.

## Phase 0 — Foundation ✅ `v0.1.0-pre.0`

Repository, Rust workspace, VS Code extension skeleton, CLI skeleton, the
Core/CLI/extension separation, configuration loading, logging, error handling, CI,
cross-platform builds, license, README, CONTRIBUTING, documentation structure.

**Done when** `chiploom --version`, `chiploom --help` and `chiploom doctor` work,
and the extension starts and communicates with the Rust core.

## Phase 1 — Target system `v0.2.0-pre.0`

MCU database; target pack and board profile formats; MCU search and information
display; memory maps; CPU architecture detection; toolchain requirement
resolution; SVD and other metadata; boards kept separate from MCUs.

```bash
chiploom target list
chiploom target search STM32G4
chiploom target info STM32G431CBT6
```

**Done when** supported MCUs are described in one format, and a new MCU of an
existing family can be added by adding a target pack, without significant changes
to the core.

## Phase 2 — Toolchain manager `v0.3.0-pre.0`

Automatic toolchain selection; OS and CPU detection; download; SHA-256
verification; extraction; caching; version pinning; several versions side by side;
a package manifest; offline import and export; SDK and framework pack management.

Targets ARM GCC, AVR GCC, RISC-V GCC, Xtensa GCC and LLVM/Clang.

**Done when** `chiploom build` on a clean machine acquires every toolchain it
needs, with no prior GCC installation required of the user.

## Phase 3 — Build engine `v0.4.0-pre.0`

C, C++ and assembly; compiler and linker invocation; ELF output and conversion to
BIN, HEX and UF2; a dependency graph; parallel and incremental builds; a build
cache; header dependency detection; debug and release profiles; flash and RAM
usage analysis; `compile_commands.json` and clangd integration.

**Done when** clean, incremental and no-change builds of the same project all
behave correctly, and no dependency change is ever missed.

## Phase 4 — Flash engine `v0.5.0-pre.0`

Device detection by USB VID/PID, serial port and bootloader; a common flash driver
API with erase, program, verify and reset; recovery from a failed write;
reconnection; firmware format handling.

Methods: UF2, USB DFU, UART bootloader, AVR ISP, SWD, ESP ROM bootloader.

**Done when** `chiploom flash` writes and verifies on every target in the support
table — on real hardware.

## Phase 5 — UART and USB monitor `v0.6.0-pre.0`

A primary feature, not an accessory.

UART in ASCII, hex and binary; baud rate; RX and TX; timestamps; search; filter;
logging to file; reconnection. USB CDC, HID, bulk and vendor interfaces.
Structured logs, variable inspection, multi-series graphs, CSV export, a
command/response channel, and an MCU-side Chip Loom debug library.

**Done when** it survives an endurance test: long sessions, high data rates, USB
unplugging, UART reconnection.

## Phase 6 — Debugger `v0.7.0-pre.0`

Debug Adapter Protocol integration, CMSIS-DAP first and more probes after. SWD and
JTAG; halt, resume, step, reset; software and hardware breakpoints; watchpoints;
register and memory access; call stacks; source mapping; ELF and DWARF parsing;
disassembly; variables; expressions; an SVD peripheral viewer.

**Done when** run, pause, step in/over/out, breakpoints, watches, registers, memory
and peripherals all work from VS Code's standard debug UI.

## Phase 7 — Framework integration `v0.8.0-pre.0`

Bare metal, CMSIS, STM32 HAL, Arduino Core, Pico SDK, ESP-IDF and FreeRTOS usable
directly from Chip Loom.

**Done when** representative samples exercise each framework's major features —
GPIO, UART, SPI, I²C, ADC, PWM, timers, interrupts, USB, RTOS, networking — not
merely a blink.

## Phase 8 — Library, test, analysis `v0.9.0-pre.0`

A library manager with registry, Git, GitHub, local and archive sources; version
constraints; dependency resolution; a lockfile. Host and on-device tests, a result
viewer, CLI and CI output. clang-tidy integration with diagnostics in VS Code's
Problems view.

## Phase 9 — Remote and OTA `v0.10.0-pre.0`

Remote build, flash, monitor, debug and test; device sharing; authentication;
encryption. OTA firmware upload, verification, rollback and failure recovery where
the framework and MCU allow it.

## Phase 10 — Final verification `v1.0.0-rc.x`

No new features. Verification on Windows, macOS (Intel and Apple Silicon) and
Linux (x86-64 and ARM64): clean installs, build and flash stress tests, long USB
and UART sessions, debugger endurance, corrupted packages, network and USB
disconnection, permission errors, recovery from abnormal termination, upgrade and
rollback, documentation, every sample building, security and license review.

Each significant fix ships as a new release candidate.

## `v1.0.0` — the first stable release

Published only when every completion condition is met. It will contain:

* no unimplemented buttons or commands;
* no dummy or placeholder behaviour;
* no major feature missing behind a `TODO`;
* no major feature that depends on "we will add this later";
* nothing listed as supported that has not been verified on real hardware.

## 日本語

Chip Loom は全11フェーズで構築します。各フェーズの完了時に、その時点で動作する成果物を
**GitHub Prerelease** として公開します。したがって開発履歴がそのままリリース履歴になります。

`v1.0.0` が最初の完成製品です。それ以前のものはすべて開発履歴・検証履歴であり、
評価や進捗の追跡のためのものです。

`v1.0.0` には、未実装のボタン、ダミー処理、`TODO` による主要機能の欠落、
「将来対応」を前提とした主要機能、実機未検証の「対応済み」表示のいずれも残しません。
