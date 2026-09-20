# Core IPC protocol, version 1

The protocol between the Chip Loom core and its clients. The VS Code extension is
one client; so is any other editor integration, and so is a script driving
`chiploom serve --stdio`.

Authoritative implementation: `crates/chiploom-core/src/ipc/`. TypeScript mirror:
`extension/src/core/protocol.ts`. Both are exercised against each other by
`extension/test/protocol.test.ts`, which drives the real binary.

## Transport

JSON-RPC 2.0 carried as **newline-delimited JSON** over stdio. One complete JSON
object per line; no `Content-Length` headers.

```bash
chiploom serve --stdio
```

* The client writes requests to the child's **stdin**, one per line.
* The core writes responses and notifications to its **stdout**, one per line.
* The core writes diagnostics to its **stderr**. A client should surface these —
  the VS Code extension puts them in its output channel verbatim.

Two invariants hold, and both are enforced by tests:

1. **stdout carries only protocol frames.** No banner, no summary, no log line, at
   any verbosity. `crates/chiploom-cli/tests/cli.rs` runs `serve --stdio` at
   `-vvv` and parses every stdout line as JSON.
2. **No frame contains a raw newline.** JSON string escaping guarantees this for
   any payload, including binary data carried as base64 in later phases.

Blank lines in the input are ignored; some clients flush an empty write.

### Why newline-delimited

Framing headers exist to carry payloads that may contain the delimiter. JSON
cannot: a newline inside a string is escaped. Line framing is therefore sufficient
and is trivially debuggable — you can drive the core from a shell pipeline, which
is how the examples below work.

## Session lifecycle

```text
        ┌────────────────┐
        │ Uninitialized  │  serves: initialize, $/ping, exit
        └───────┬────────┘
                │ initialize (versions agree)
                ▼
        ┌────────────────┐
        │     Ready      │  serves: everything
        └───────┬────────┘
                │ shutdown
                ▼
        ┌────────────────┐
        │  ShuttingDown  │  awaits: exit
        └────────────────┘
```

1. The client sends `initialize` with the protocol version it speaks.
2. The core replies with its capabilities, or refuses the version.
3. The client sends the `initialized` notification. The core answers with
   `core/ready`.
4. Requests flow.
5. The client sends `shutdown`, then the `exit` notification.

Any request other than `initialize` or `$/ping` before step 2 is answered with
`-32002`. `$/ping` is served from the start so a client can tell a hung core from
a slow one.

An `exit` without a preceding `shutdown` is served, but noted in the log: it
usually means the client crashed. A closed transport ends the session cleanly — an
editor being killed is not an error in the core.

## Methods

| Method | Direction | Result |
| --- | --- | --- |
| `initialize` | client → core | [`InitializeResult`](#initialize) |
| `initialized` | client → core (notification) | none; triggers `core/ready` |
| `shutdown` | client → core | `null` |
| `exit` | client → core (notification) | none; ends the session |
| `$/ping` | client → core | `{ uptimeMs, initialized }` |
| `core/version` | client → core | build provenance |
| `core/doctor` | client → core | a diagnostics report |
| `core/config` | client → core | effective configuration and provenance |
| `core/ready` | core → client (notification) | `{ sessionId, version }` |
| `core/log` | core → client (notification) | reserved; not sent by this version |

The authoritative list for a running core is `capabilities.methods` from the
handshake. **Clients should feature-detect on that, not compare version strings.**

### `initialize`

```json
{ "jsonrpc": "2.0", "id": 1, "method": "initialize",
  "params": {
    "clientName": "chiploom-vscode",
    "clientVersion": "0.1.0-pre.0",
    "protocolVersion": 1,
    "workspaceRoots": ["/home/you/firmware"]
  }
}
```

`clientName` and `protocolVersion` are required; the rest are optional.

```json
{ "jsonrpc": "2.0", "id": 1,
  "result": {
    "serverName": "chiploom-core",
    "serverVersion": "0.1.0-pre.0",
    "protocolVersion": 1,
    "capabilities": {
      "methods": ["initialize", "initialized", "shutdown", "exit", "$/ping",
                  "core/version", "core/doctor", "core/config"],
      "notifications": ["core/ready", "core/log"]
    },
    "sessionId": "1b97-18d6f7cae062f9e6",
    "pid": 7063
  }
}
```

`sessionId` appears in the core's own log lines, so a client can correlate its
report with the core's.

A version this core does not implement is refused with `-32000` and both versions
in `data`:

```json
{ "jsonrpc": "2.0", "id": 1,
  "error": { "code": -32000,
    "message": "protocol version 9999 is not supported (this build speaks version 1)",
    "data": { "requested": 9999, "supported": 1,
              "hint": "Update the Chip Loom extension and the chiploom binary together." } } }
```

Omitting `protocolVersion` is `-32602` naming the missing field. Sending
`initialize` twice is `-32600`.

### `core/version`

No params. Returns build provenance — the same values as `chiploom version -v`:

```json
{ "version": "0.1.0-pre.0", "protocolVersion": 1, "commit": "a1b2c3d4e5f6",
  "dirty": false, "target": "x86_64-unknown-linux-gnu",
  "host": "x86_64-unknown-linux-gnu", "profile": "release",
  "rustc": "rustc 1.88.0 (…)", "builtAt": "2026-09-20T07:37:08Z" }
```

### `core/doctor`

```json
{ "jsonrpc": "2.0", "id": 2, "method": "core/doctor",
  "params": { "online": false, "writeProbe": true } }
```

Both params are optional. `online` defaults to `false` and `writeProbe` to `true`,
matching the CLI. The result is identical to `chiploom doctor --format json`; see
[cli.md](cli.md#the-checks) for the check list and their meanings.

### `core/config`

No params. Returns the effective configuration, the resolved storage paths, the
layers that were consulted, and any key no layer understood:

```json
{ "config": { "project": { "name": "blinky" },
              "log": { "level": "debug", "format": "compact",
                       "file": null, "timestamps": false },
              "network": { "offline": false, "timeout_secs": 30, "retries": 3 } },
  "paths": { "configDir": "…", "dataDir": "…", "toolchainsDir": "…",
             "cacheDir": "…", "logDir": "…", "projectRoot": "…" },
  "sources": [ { "kind": "defaults", "path": null, "status": "applied" } ],
  "warnings": [] }
```

## Field naming

Protocol fields are **camelCase**, with one deliberate exception: the `config`
object mirrors `chiploom.toml` key-for-key — `timeout_secs`, `data_dir` — so it can
be serialized back into a configuration file unchanged. Everything else,
`paths` included, is camelCase.

## Errors

Standard JSON-RPC codes, plus three of Chip Loom's own:

| Code | Meaning |
| --- | --- |
| `-32700` | Parse error: the line was not valid JSON. The response's `id` is `null`. |
| `-32600` | Invalid request: wrong `jsonrpc` version, or `initialize` sent twice. |
| `-32601` | Unknown method. `data.supported` lists the methods this core serves. |
| `-32602` | Invalid params. |
| `-32603` | Internal error. Always a bug in Chip Loom. |
| `-32002` | A request arrived before `initialize` completed. |
| `-32001` | The request was understood and attempted, but failed. `data.kind` gives the failure class. |
| `-32000` | The requested protocol version is not supported. |

`data.hint` carries remediation text when one applies — the same text the CLI
prints under a failure. A client should show it rather than inventing its own.

**A failed request never ends the session.** A malformed frame, an unknown method
and invalid params are all answered and the session continues; one stray call must
not take down an editor's core.

## Driving it from a shell

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientName":"shell","protocolVersion":1}}' \
  '{"jsonrpc":"2.0","id":2,"method":"core/version"}' \
  '{"jsonrpc":"2.0","id":3,"method":"shutdown"}' \
  '{"jsonrpc":"2.0","method":"exit"}' \
  | chiploom serve --stdio 2>/dev/null
```

```text
{"jsonrpc":"2.0","id":1,"result":{"serverName":"chiploom-core",…}}
{"jsonrpc":"2.0","id":2,"result":{"version":"0.1.0-pre.0",…}}
{"jsonrpc":"2.0","id":3,"result":null}
```

Drop the `2>/dev/null` to watch the core's log alongside the protocol.

## Versioning

`protocolVersion` is a single integer, incremented when a change would break an
existing client. Adding a method or a notification does **not** increment it —
that is what `capabilities` is for.

A client and a core that disagree refuse to proceed rather than guessing, because
a partially-understood debug session is worse than no session.

## Planned extensions

Not implemented in `v0.1.0-pre.0`. Listed so the shape of the protocol is clear,
not as a claim that anything works yet:

| Phase | Additions |
| --- | --- |
| 1 | `target/list`, `target/search`, `target/info`. |
| 2 | `toolchain/list`, `toolchain/install`, with progress notifications. |
| 3 | `build/start`, `build/cancel`, `build/output` notifications. |
| 4 | `device/list`, `flash/start`, `flash/progress` notifications. |
| 5 | `monitor/open`, `monitor/data` notifications, `monitor/write`. |

Every one of them arrives with documentation on this page and tests in
`extension/test/`.
