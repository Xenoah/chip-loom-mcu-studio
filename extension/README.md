# Chip Loom for VS Code

Build, flash, monitor and debug microcontroller firmware from VS Code.

> **This is a prerelease from Phase 0 of 11.** It does not build, flash, monitor or
> debug anything yet. What it does is connect to the Chip Loom core and report on
> your environment, which is the foundation the rest is built on. See the
> [roadmap](https://github.com/Xenoah/chip-loom-mcu-studio/blob/main/docs/roadmap.md).

## What this version does

| Command | Effect |
| --- | --- |
| **Chip Loom: Run Diagnostics** | Runs the core's environment checks and shows the report in the output channel. |
| **Chip Loom: Show Core Version** | Reports the connected core's version, target and build. |
| **Chip Loom: Restart Core** | Stops the core and starts it again. |
| **Chip Loom: Show Output** | Opens the Chip Loom output channel. |

A status bar item shows the connected core's version, and the output channel
carries everything the core logs.

## Requirements

The `chiploom` executable. Get it from
[Releases](https://github.com/Xenoah/chip-loom-mcu-studio/releases), or build it
with `cargo build --release`.

The extension looks for it in this order:

1. the `chiploom.corePath` setting;
2. the `CHIPLOOM_BIN` environment variable;
3. `target/release/chiploom`, then `target/debug/chiploom`, in an open workspace
   folder — so a contributor always runs the core they just built;
4. `PATH`.

If none of those has it, the error names every place it looked.

## Settings

| Setting | Default | Meaning |
| --- | --- | --- |
| `chiploom.corePath` | `""` | Absolute path to the `chiploom` executable. Empty means search. |
| `chiploom.autoStart` | `true` | Connect to the core when a window opens. |
| `chiploom.logLevel` | `"info"` | Log level requested from the core. |
| `chiploom.startupTimeoutMs` | `15000` | How long to wait for the handshake. |

## How it talks to the core

The extension spawns `chiploom serve --stdio` and speaks JSON-RPC 2.0 over
newline-delimited stdio. It has no privileged path into the core — it is a client
of the same protocol any other editor integration would use, which is what
guarantees nothing here is unavailable from the command line.

The protocol is specified in
[docs/ipc-protocol.md](https://github.com/Xenoah/chip-loom-mcu-studio/blob/main/docs/ipc-protocol.md).

## License

Apache License 2.0.
