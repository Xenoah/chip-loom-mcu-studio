# Configuration reference

Complete as of `v0.1.0-pre.0`. Anything not listed here is not read by this
version; unknown keys are reported by `chiploom config check` and otherwise
ignored.

## Where configuration comes from

Layers are merged in increasing order of precedence. A layer that says nothing
about a value leaves the layer below it untouched — saying nothing is not the same
as saying the default.

| # | Layer | Location |
| --- | --- | --- |
| 1 | Built-in defaults | Compiled in. |
| 2 | User-global file | See the table below. |
| 3 | Project file | The nearest `chiploom.toml` in this directory or any parent. |
| 4 | Environment | `CHIPLOOM_*` variables. |
| 5 | Command line | Flags. |

`--config <FILE>` (or `CHIPLOOM_CONFIG`) replaces layer 2 *and* project discovery
with one named file. That file must exist: silently ignoring a file you asked for
would run with a configuration you did not choose.

Run `chiploom config path` to see all of this for your machine, and
`chiploom config show --sources` to see the result with its provenance.

## Locations per platform

| | Linux | macOS | Windows |
| --- | --- | --- | --- |
| Config | `~/.config/chiploom/` | `~/Library/Application Support/chiploom/` | `%APPDATA%\chiploom\` |
| Data | `~/.local/share/chiploom/` | `~/Library/Application Support/chiploom/` | `%APPDATA%\chiploom\data\` |
| Cache | `~/.cache/chiploom/` | `~/Library/Caches/chiploom/` | `%LOCALAPPDATA%\chiploom\cache\` |

Each of the three can be relocated: `CHIPLOOM_CONFIG_DIR`, and `paths.data_dir` /
`paths.cache_dir` (or `CHIPLOOM_DATA_DIR` / `CHIPLOOM_CACHE_DIR`). On Windows these
are the **only** way to relocate them: the platform locations come from the Known
Folder API, not from `%APPDATA%`, so overriding that variable achieves nothing.

On a host where no home directory can be determined at all — a service account, a
locked-down container, a CI image with no profile — setting **all three** of
`CHIPLOOM_CONFIG_DIR`, `CHIPLOOM_DATA_DIR` and `CHIPLOOM_CACHE_DIR` is enough on its
own; Chip Loom then never asks the platform. With only some of them set, it stops
and names the ones still missing.

The three are kept apart because they have different lifetimes. **Config** is
hand-edited and belongs in your dotfiles. **Data** holds installed toolchains and
target packs: large, reproducible, but expensive to re-fetch. **Cache** holds
downloads, build caches and logs, and is safe to delete at any moment.

Derived locations: `<data>/toolchains/`, `<data>/targets/`, `<cache>/downloads/`,
`<cache>/logs/`, and `<project>/.chiploom/` for per-project state.

## File format

TOML. A complete example, with every key this version reads:

```toml
[project]
# Human-readable project name. Defaults to the project directory's name.
name = "blinky"

[paths]
# Override where Chip Loom stores things. Relative paths are resolved against
# the project root, so a checked-in value means the same thing from any
# subdirectory.
data_dir = "vendor/chiploom/data"
cache_dir = "/var/cache/chiploom"

[log]
level = "info"        # error | warn | info | debug | trace
format = "compact"    # compact | pretty | json
file = "build.log"    # additionally append JSON records here
timestamps = false    # timestamps in human-readable output

[network]
offline = false       # true forbids every network connection
timeout_secs = 30     # per-request timeout; must be greater than 0
retries = 3           # retries per failed request; 0 is allowed
```

### `[project]`

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `name` | string | the project directory's name | Human-readable project name. |

### `[paths]`

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `data_dir` | path | platform data directory | Where toolchains and target packs are installed. |
| `cache_dir` | path | platform cache directory | Where downloads, build caches and logs go. |

A relative path is resolved against the **project root** when there is one, and
against the working directory otherwise. An absolute path is used as given.

There is deliberately no `config_dir` key here. It would be circular — the value
would have to be read from the file whose location it decides — so the
configuration directory is relocated with the `CHIPLOOM_CONFIG_DIR` environment
variable instead.

### `[log]`

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `level` | enum | `info` | Minimum severity reaching the output. `error`, `warn` (`warning` accepted), `info`, `debug`, `trace`. |
| `format` | enum | `compact` | `compact` (one line per record), `pretty` (`full` accepted; multi-line, fields expanded) or `json`. |
| `file` | path | unset | Also append every record here, always as JSON with timestamps, whatever the terminal shows. Parent directories are created. |
| `timestamps` | boolean | `false` | Timestamps in human-readable output. A log *file* always has them. |

Diagnostics go to **stderr**, never stdout. This is not configurable:
`chiploom serve --stdio` uses stdout as the protocol channel, and a stray log line
there would corrupt the session.

### `[network]`

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `offline` | boolean | `false` | When true, Chip Loom must never open a network connection. Commands that would need one fail with an explanation instead. |
| `timeout_secs` | integer | `30` | Per-request timeout, in seconds. `0` is rejected: it would fail every request before it started. |
| `retries` | integer | `3` | Retries per failed request. `0` is a legitimate choice. |

`offline = true` outranks `--online` on `chiploom doctor`: an explicit policy of
never touching the network is not overridden by a flag that asks to test it.

## Environment variables

| Variable | Equivalent | Notes |
| --- | --- | --- |
| `CHIPLOOM_CONFIG` | `--config` | Path to one configuration file, replacing discovery. The flag wins if both are set. |
| `CHIPLOOM_CONFIG_DIR` | — | Directory the user-global `config.toml` is read from. Environment-only: it decides which file to read, so it cannot come from a file. |
| `CHIPLOOM_LOG_LEVEL` | `log.level` | |
| `CHIPLOOM_LOG_FORMAT` | `log.format` | |
| `CHIPLOOM_LOG_FILE` | `log.file` | |
| `CHIPLOOM_DATA_DIR` | `paths.data_dir` | |
| `CHIPLOOM_CACHE_DIR` | `paths.cache_dir` | |
| `CHIPLOOM_OFFLINE` | `network.offline` | `1`, `true`, `yes`, `on` are true; `0`, `false`, `no`, `off`, empty are false. Case-insensitive. |
| `CHIPLOOM_NETWORK_TIMEOUT` | `network.timeout_secs` | |
| `CHIPLOOM_NETWORK_RETRIES` | `network.retries` | |

Two more are read outside the configuration system:

| Variable | Effect |
| --- | --- |
| `CHIPLOOM_LOG` | Overrides the computed log filter using the full `tracing` grammar, e.g. `chiploom_core::ipc=trace,info`. Takes precedence over `log.level`. |
| `NO_COLOR` | Any value, including empty, disables colour when `--color` is `auto`. See [no-color.org](https://no-color.org). |

A variable that is set but unparsable **fails the command** with exit code `3`.
Silently ignoring `CHIPLOOM_OFFLINE=yse` would be worse than stopping.

## Unknown keys

Unknown keys are collected, not rejected, so a project pinned to an older Chip
Loom can still open a `chiploom.toml` written by a newer one.

```console
$ chiploom config check
ok   project file: /home/you/firmware/chiploom.toml [applied]

warn `/home/you/firmware/chiploom.toml` sets `log.lvl`, which this version of Chip Loom does not use

1 warning found. Unrecognised keys have no effect.
```

The same warnings appear as the `config.layers` check in `chiploom doctor`, and in
`Loaded::warnings` over IPC. `chiploom config check` exits `0` regardless; use
`chiploom doctor --strict` if you want a typo to fail CI.

## Recipes

**Make CI independent of the machine it runs on.** Layer 2 is whatever the runner
image happens to have:

```bash
chiploom --no-global-config doctor --strict
```

**Confine a run entirely to one directory**, on any platform — which is how Chip
Loom's own integration tests isolate themselves:

```bash
export CHIPLOOM_CONFIG_DIR=$PWD/.ci/config
export CHIPLOOM_DATA_DIR=$PWD/.ci/data
export CHIPLOOM_CACHE_DIR=$PWD/.ci/cache
chiploom doctor
```

**Vendor toolchains into the repository** so every clone shares one copy:

```toml
[paths]
data_dir = "vendor/chiploom/data"
```

**Work entirely offline**, failing fast rather than hanging on a timeout:

```toml
[network]
offline = true
```

**Capture a support log** without changing what you see on screen:

```bash
chiploom -vv --log-file chiploom.log doctor --online
```
