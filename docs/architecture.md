# Architecture

Chip Loom is one core with two front ends.

```text
        ┌──────────────────────┐        ┌──────────────────────┐
        │  VS Code extension   │        │      chiploom CLI    │
        │     (TypeScript)     │        │        (Rust)        │
        │  extension/          │        │  crates/chiploom-cli │
        └──────────┬───────────┘        └──────────┬───────────┘
                   │ JSON-RPC 2.0 over stdio       │ direct calls
                   └───────────────┬───────────────┘
                                   ▼
                        ┌─────────────────────────┐
                        │      chiploom-core      │
                        │  crates/chiploom-core   │
                        │                         │
                        │  config   · paths       │
                        │  logging  · error       │
                        │  doctor   · ipc         │
                        └─────────────────────────┘
```

## The division of responsibility

### `chiploom-core` decides

The core answers questions. What does this configuration mean? Is this machine
able to run Chip Loom? Is this protocol frame valid? It holds no terminal, no
editor and no assumptions about who is asking.

Two rules make that real:

* **It never prints.** Nothing in `chiploom-core` writes to stdout. Diagnostics
  go through `tracing`, which the CLI routes to stderr.
* **It never exits.** Every failure comes back as `error::Error`. The core
  suggests an exit code through `Error::exit_code`, but the decision to terminate
  belongs to the front end.

The payoff is concrete: one `doctor` implementation becomes terminal text, a JSON
document, and a JSON-RPC result, with no duplicated logic and no possibility of
the three disagreeing.

### `chiploom-cli` presents

The CLI parses arguments, loads configuration, installs logging, calls the core,
renders the result, and chooses the process exit code. All human-facing
formatting -- column alignment, colour, pluralisation -- lives in
`crates/chiploom-cli/src/render.rs` and nowhere else.

### `extension/` presents, in an editor

The extension is a client of the same protocol any other editor integration would
use. It has no privileged path into the core. That is deliberate: it is the only
way to guarantee the editor cannot do something `chiploom` on the command line
cannot.

## Why a protocol instead of linking

The extension could have shelled out to `chiploom --format json` for each
operation. A long-lived protocol session was chosen because of what the later
phases need:

* **Streaming.** A UART monitor (Phase 5) and a build (Phase 3) produce output
  over time. A request/response subprocess per operation cannot stream.
* **State.** A debug session (Phase 6) holds breakpoints, a halted core and a
  symbol table across many operations.
* **Cancellation.** A build or a flash has to be interruptible.
* **Cost.** Re-reading the configuration and re-scanning target packs on every
  keystroke-driven operation is wasted work.

Phase 0 does not need any of that. It establishes the protocol anyway, because
retrofitting a session onto a subprocess-per-call design later would mean
rewriting both front ends.

## Rules the code enforces

Some of these conventions are checked rather than trusted:

| Rule | How it is enforced |
| --- | --- |
| stdout carries only protocol frames | `crates/chiploom-cli/tests/cli.rs` runs `serve --stdio` at `-vvv` and parses every stdout line as JSON. |
| No placeholders in committed code | The workspace lints deny `todo!()`, `unimplemented!()` and `dbg!()`. |
| No `unsafe` | `unsafe_code = "forbid"` at the workspace level. |
| The core does not print | `print_stdout` is a workspace warning; only the CLI allows it, with a comment saying why. |
| Configuration layering is correct | Patch-based merging (below), covered by tests in `crates/chiploom-core/src/config/`. |
| Exit codes are stable | `crates/chiploom-core/src/error.rs` asserts each numeric value. |

## Configuration layering

Layers are merged as *patches* -- structures in which every field is optional --
and resolved against the defaults only at the very end.

```text
defaults  ←  global config.toml  ←  project chiploom.toml  ←  CHIPLOOM_*  ←  flags
   (lowest precedence)                                          (highest precedence)
```

This is not incidental. If each file were deserialized into a fully-defaulted
`Config`, a project file that says nothing about `log.level` would still overwrite
the global value with the default. The patch representation makes "says nothing"
distinguishable from "says the default", which is the whole problem.

Unknown keys are collected rather than rejected, so a project pinned to an older
Chip Loom can still open a `chiploom.toml` written by a newer one.
`chiploom config check` is where the user hears about a key that did nothing.

Every layer that was consulted is reported in `Loaded::sources`, including the
ones that were missing. That is what lets `chiploom config show --sources` answer
"why is this value what it is?" instead of merely stating the value.

## Error handling

One error type, `error::Error`, with a variant per failure class, each mapping to
a documented exit code. Three properties matter:

* **Failures name their subject.** An I/O error carries the path and the
  operation, because "permission denied" on its own is not actionable.
* **Failures suggest a next step.** `Error::hint` returns remediation text where
  one applies, and the CLI prints it under the message.
* **Failures are machine-readable.** `Error::kind` is a stable string, carried in
  JSON output and in the `data` field of a JSON-RPC error, so tooling can branch
  on the kind without parsing prose.

## What each phase adds, and where

The structure is sized for what is coming. See [roadmap.md](roadmap.md) for the
full plan; the shape of the additions:

| Phase | Where it lands |
| --- | --- |
| 1 — MCU and target management | A `targets` module in the core, `chiploom target` in the CLI, `target/*` IPC methods. |
| 2 — Toolchain manager | A `toolchain` module; downloads under `Paths::downloads_dir`, installs under `Paths::toolchains_dir`. |
| 3 — Build engine | A `build` module, with the dependency graph and cache under the project's `.chiploom/`. |
| 4 — Flash and device engine | A `device` module with a driver trait per programming method. |
| 5 — UART and USB monitor | Streaming notifications over the existing IPC session. |
| 6 — Hardware debugger | A Debug Adapter Protocol server in the extension, backed by core IPC methods. |

Nothing above exists yet. Each arrives with its own CLI commands, IPC methods and
documentation, under the rules on this page.
