# `chiploom` command reference

Complete as of `v0.1.0-pre.0`. Anything not listed here does not exist yet; see
[roadmap.md](roadmap.md).

```text
chiploom [OPTIONS] <COMMAND>
```

## Global options

Accepted before or after the subcommand, so `chiploom doctor --format json` and
`chiploom --format json doctor` are the same command.

| Option | Description |
| --- | --- |
| `-c`, `--config <FILE>` | Use this configuration file instead of searching. Replaces both the user-global file and project discovery. The file must exist. |
| `--no-global-config` | Ignore the user-global file. Use this in CI so a run cannot be influenced by the machine it lands on. |
| `-C`, `--directory <DIR>` | Run as if Chip Loom had been started in `DIR`. Applied before project discovery. |
| `-v`, `--verbose...` | More diagnostics on stderr. Repeatable: `-v` debug, `-vv` trace. |
| `-q`, `--quiet...` | Fewer diagnostics. Repeatable. Cannot be combined with `-v`. |
| `--log-format <FORMAT>` | `compact` (default), `pretty` or `json`. Affects stderr only. |
| `--log-file <FILE>` | Append every diagnostic to `FILE` as JSON, in addition to stderr. Parent directories are created. |
| `--format <FORMAT>` | How command *results* are printed on stdout: `text` (default) or `json`. |
| `--color <WHEN>` | `auto` (default), `always` or `never`. `auto` colours only when stdout is a terminal and `NO_COLOR` is unset. |
| `--offline` | Forbid all network access for this run. |
| `-h`, `--help` | Help. `-h` is a summary, `--help` is the full text. |
| `-V`, `--version` | One line: `chiploom <version>`, plus the commit when known. |

`--log-format` and `--format` are separate on purpose: the first governs
diagnostics on stderr, the second governs results on stdout. A script can ask for
JSON results while leaving logs human-readable, or the reverse.

## `chiploom doctor`

Reports whether this machine can run Chip Loom.

| Option | Description |
| --- | --- |
| `--online` | Also test that the hosts Chip Loom downloads from are reachable. Off by default so `doctor` is fast and safe on an air-gapped machine. |
| `--no-write-probe` | Do not write probe files. Makes the run read-only, at the cost of not proving the directories are writable. |
| `--strict` | Exit non-zero on warnings as well as failures. |

Exits `0` when no check failed, `7` when one did — or, with `--strict`, when any
check warned.

### The checks

| Id | What it looks at |
| --- | --- |
| `core.build` | This binary's version, target, profile and commit. Warns when built from a dirty working tree. |
| `host.platform` | Operating system name, version, kernel and architecture. |
| `host.resources` | Usable CPU threads and total RAM. Warns under 1 GiB. |
| `config.layers` | Which configuration layers applied, the effective log level, and any key no layer understood. |
| `paths.config` | The configuration directory exists and is writable. |
| `paths.data` | The data directory (toolchains, target packs) exists and is writable. |
| `paths.cache` | The cache directory (downloads, build caches, logs) exists and is writable. |
| `project.root` | Whether this directory is inside a Chip Loom project. Skipped when it is not; that is normal. |
| `tools.git` | `git` on `PATH`. Warns if absent: needed only for libraries fetched from Git, from Phase 8. |
| `tools.vscode` | A `code`, `code-insiders` or `codium` launcher on `PATH`. Skipped if absent; the extension works without it. |
| `network.reachability` | TCP reachability of `github.com:443` and `crates.io:443`. Skipped unless `--online`, and always skipped when `network.offline` is set. |

A directory check does more than test for existence: it creates the directory,
writes a probe file, reads it back and deletes it. A bare existence check passes
on read-only mounts and on Windows directories that deny writes, which is exactly
the case `doctor` exists to catch.

Warnings never fail the command unless you ask with `--strict`. A machine without
`git` or VS Code builds firmware perfectly well, and CI that went red for it would
be unusable.

### JSON output

```bash
chiploom doctor --format json
```

```json
{
  "build": { "version": "0.1.0-pre.0", "protocolVersion": 1, "target": "…" },
  "checks": [
    { "id": "core.build", "title": "Chip Loom build", "status": "ok",
      "detail": "chiploom 0.1.0-pre.0 for …", "hint": null }
  ],
  "durationMs": 3
}
```

`status` is one of `ok`, `warn`, `error`, `skipped`. `id` is stable across
releases and safe to grep for in CI.

## `chiploom version`

Prints `chiploom <version>`. With the global `-v`, prints the whole build record —
protocol version, commit, target, host, profile, compiler and build timestamp.
That is what a bug report needs.

`--format json` emits the same fields as a JSON object.

## `chiploom config`

### `chiploom config show`

Prints the effective configuration as TOML, which means the output can be pasted
straight into a `chiploom.toml`. Sections with no values are omitted.

`--sources` appends the consulted layers as TOML comments, so the output stays
valid TOML. `--format json` emits the configuration together with its provenance
and the resolved storage paths.

### `chiploom config path`

Prints every configuration layer, in increasing order of precedence, with whether
each one was applied, missing, skipped or empty — followed by every storage
location. This is the command to run when a value is not what you expected.

### `chiploom config check`

Validates the configuration files and reports keys that no layer understood.

Exits `0` even when keys were ignored: a project pinned to an older Chip Loom must
be able to mention a key a newer one added without failing CI. An invalid file is
a different matter and fails during loading, with exit code `3`, before this
command runs.

## `chiploom serve`

Runs the Core IPC server. Editors invoke this; people rarely need to.

| Option | Description |
| --- | --- |
| `--stdio` | Serve on stdin and stdout. Required today, and named explicitly so another transport can be added without changing what existing clients invoke. |

**stdout carries only protocol frames.** Every diagnostic goes to stderr,
whatever `-v` level is in effect. See [ipc-protocol.md](ipc-protocol.md).

Without a transport, exits `8`.

## `chiploom completions <SHELL>`

Writes a completion script to stdout for `bash`, `zsh`, `fish`, `powershell` or
`elvish`.

```bash
# bash
chiploom completions bash > ~/.local/share/bash-completion/completions/chiploom

# zsh, with ~/.zfunc on your fpath
chiploom completions zsh > ~/.zfunc/_chiploom

# fish
chiploom completions fish > ~/.config/fish/completions/chiploom.fish

# PowerShell, appended to your profile
chiploom completions powershell >> $PROFILE
```

## Exit codes

Stable across releases. Scripts and CI may rely on them.

| Code | Meaning |
| --- | --- |
| `0` | Success. |
| `1` | A general, unclassified failure. |
| `2` | The command line was wrong: unknown command, bad flag, missing argument. |
| `3` | A configuration file or environment variable was missing, unreadable or invalid. |
| `4` | A filesystem operation failed. |
| `5` | The IPC peer violated the protocol. |
| `6` | The host environment cannot support the request. |
| `7` | `doctor` completed and reported at least one failure (or, with `--strict`, a warning). |
| `8` | The request is understood but not supported on this build or platform. |

Codes `9` and above are unused, and are reserved for later phases.

## Failure output

Failures go to stderr, name what they were doing, and suggest what to do:

```console
$ chiploom config show
error: `/home/you/firmware/chiploom.toml` is not valid Chip Loom configuration: invalid table header

hint: Fix `/home/you/firmware/chiploom.toml`, then run `chiploom config check` to confirm Chip Loom reads it.
$ echo $?
3
```
