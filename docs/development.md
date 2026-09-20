# Development

## Prerequisites

| | Version | Needed for |
| --- | --- | --- |
| Rust | stable, 1.88 or newer | The core and the CLI. `rust-toolchain.toml` pins the channel and pulls in `rustfmt` and `clippy`. |
| Node.js | 20 or newer | The VS Code extension. |
| git | any | Recorded in the binary's version string; not required to build. |

## Repository layout

```text
.
├── Cargo.toml                  Workspace: members, shared dependencies, lints
├── rust-toolchain.toml         Pins the toolchain channel and components
├── crates/
│   ├── chiploom-core/          Every decision Chip Loom makes
│   │   ├── build.rs            Captures version, commit, target at build time
│   │   └── src/
│   │       ├── config/         Layered configuration
│   │       │   ├── model.rs    The resolved configuration
│   │       │   ├── patch.rs    The optional form, and how layers merge
│   │       │   └── mod.rs      The loader, and source provenance
│   │       ├── doctor/         Diagnostics
│   │       │   ├── checks.rs   The individual checks
│   │       │   └── mod.rs      Report types and the runner
│   │       ├── ipc/            Core IPC
│   │       │   ├── protocol.rs Wire types
│   │       │   └── server.rs   The read-dispatch-write loop
│   │       ├── error.rs        The error type and the exit codes
│   │       ├── logging.rs      tracing setup
│   │       ├── paths.rs        Platform locations, project discovery
│   │       └── version.rs      Build provenance
│   └── chiploom-cli/           The `chiploom` binary
│       ├── src/
│       │   ├── cli.rs          clap declarations
│       │   ├── commands/       One module per command
│       │   ├── render.rs       All terminal formatting
│       │   └── main.rs         Entry point, failure rendering, exit codes
│       └── tests/cli.rs        End-to-end tests of the real binary
├── extension/                  The VS Code extension
│   ├── src/
│   │   ├── core/
│   │   │   ├── client.ts       JSON-RPC client over the child process
│   │   │   ├── locate.ts       Finding the chiploom executable
│   │   │   └── protocol.ts     TypeScript mirror of the protocol
│   │   ├── ui/                 Status bar and output channel
│   │   ├── session.ts          Core lifetime: start, restart, stop
│   │   └── extension.ts        Activation and commands
│   └── test/protocol.test.ts   Drives the real binary over the real protocol
├── docs/                       This directory
└── .github/workflows/          CI and release
```

## Building

```bash
cargo build                     # debug
cargo build --release           # release
./target/debug/chiploom doctor  # confirm it works
```

```bash
cd extension
npm install
npm run compile                 # tsc, strict
npm run watch                   # recompile on change
```

## Testing

Run what CI runs, in this order:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- --deny warnings
cargo test --workspace

cargo build --bin chiploom      # the extension tests need a real binary
cd extension && npm test
```

### The three layers of tests

| Layer | Location | What it proves |
| --- | --- | --- |
| Unit | `mod tests` next to the code | A decision is correct: layering precedence, level parsing, protocol dispatch. |
| Integration | `crates/chiploom-cli/tests/cli.rs` | The real binary exits with the right code and puts the right things on stdout versus stderr. |
| Protocol | `extension/test/protocol.test.ts` | The extension's client and the Rust core actually talk to each other. Nothing is mocked. |

The protocol tests are how "the extension communicates with the Rust core" is
verified without launching an editor. They spawn `chiploom serve --stdio`, complete
the handshake, call every method, and assert that a malformed request does not kill
the session. Set `CHIPLOOM_BIN` to test a specific executable.

### Test isolation

No test may read the developer's real configuration, or results differ between a
laptop and CI. Both suites redirect `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
`XDG_CACHE_HOME`, `APPDATA` and `LOCALAPPDATA` at a temporary directory and clear
every `CHIPLOOM_*` variable. Use the existing helper — `chiploom()` in
`tests/cli.rs`, `Loader::new` with explicit `Paths` in unit tests — rather than a
new approach.

The environment is captured explicitly rather than read at the point of use:
`Loader::with_env` takes a map. That keeps loading a pure function of its inputs,
which is what makes these tests deterministic in a threaded test runner.

## Running the extension in VS Code

1. Open the repository root in VS Code.
2. `cargo build` so a binary exists in `target/debug`.
3. `cd extension && npm install && npm run compile`.
4. Press <kbd>F5</kbd>, or run the "Run Extension" launch configuration, to open an
   Extension Development Host.
5. In the new window, run **Chip Loom: Run Diagnostics** from the command palette.

The extension prefers `target/release`, then `target/debug`, then `PATH`, so a
contributor always runs the core they just built. Override with the
`chiploom.corePath` setting or the `CHIPLOOM_BIN` environment variable.

## Debugging the protocol

The core logs to stderr, so you can watch a session as it happens:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientName":"shell","protocolVersion":1}}' \
  '{"jsonrpc":"2.0","method":"exit"}' \
  | CHIPLOOM_LOG=chiploom_core::ipc=trace ./target/debug/chiploom serve --stdio
```

In VS Code, **Chip Loom: Show Output** shows the same stderr stream, and
`chiploom.logLevel` sets the level the extension asks for.

## Workspace lints

Enforced for every crate, from the root `Cargo.toml`:

| Lint | Level | Why |
| --- | --- | --- |
| `unsafe_code` | forbid | Nothing in Chip Loom needs it. |
| `clippy::todo`, `clippy::unimplemented` | deny | A placeholder must not reach a commit. |
| `clippy::dbg_macro` | deny | Debug output must not reach a release. |
| `clippy::print_stdout` | warn | stdout belongs to the protocol. The CLI allows it explicitly, with a comment. |
| `clippy::unwrap_used` | warn | A panic is never a good error message for a user. |
| `missing_docs` | warn (core) | The core is a library; its public API is documented. |

`clippy::pedantic` is on for both crates. If a pedantic lint is wrong for a
specific case, allow it at the narrowest scope with a comment saying why.

## Adding a capability

The order matters; skipping the CLI or the IPC side is what lets the two front
ends drift apart.

1. **Core.** Add the module, the types and the logic. Return `Result<_, Error>`.
   Print nothing.
2. **Unit tests.** Cover the decision, including the failure paths.
3. **CLI.** Add the command in `cli.rs`, the implementation in `commands/`, and the
   rendering in `render.rs`. Support `--format json`.
4. **IPC.** Add the method in `ipc/protocol.rs` and `ipc/server.rs`, and add its
   name to `METHODS`.
5. **Extension.** Add the TypeScript types, the `CoreSession` call, and the command.
6. **Integration and protocol tests.** Exit codes in `tests/cli.rs`, the method in
   `extension/test/`.
7. **Documentation.** `cli.md`, `configuration.md` and `ipc-protocol.md` are
   reference documents and are expected to be complete.

## Release

See [release-process.md](release-process.md).
