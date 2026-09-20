# Contributing to Chip Loom

Thank you for considering it. This document covers what you need to know before
your first change.

日本語の要点は末尾にあります。

## Ground rules

Chip Loom is a tool people trust with their hardware. Two rules follow from that
and are not negotiable:

1. **Nothing is "done" until it has been run.** A feature that compiles is not a
   feature. If your change affects hardware behaviour, say which MCU, board and
   probe you tested it on.
2. **`v1.0.0` will contain no placeholders.** No buttons that do nothing, no
   stubbed handlers, no "support" for a target nobody has verified on real
   silicon. If something is not ready, it is not listed as supported. This is why
   the workspace denies `todo!()` and `unimplemented!()` in committed code.

## Before you start

Read [AGENTS.md](AGENTS.md). It records the state of the project, the invariants that
have to keep holding, and the platform traps that have already cost time once —
Windows storage locations, `node --test` globs, macOS path symlinks. It will save you
rediscovering them.

Then run the diagnostics. It tells you whether your machine is set up, and it is the
first thing anyone will ask you for if something goes wrong:

```bash
cargo build
./target/debug/chiploom doctor
```

## The checks your change has to pass

These are exactly what CI runs, on Windows, macOS and Linux:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- --deny warnings
cargo test --workspace

cd extension
npm ci
npm run compile
npm test
```

`extension/npm test` spawns the real `chiploom` binary and drives the real
protocol, so build the core first (`cargo build`) or the tests will tell you to.

## Where code goes

The division of responsibility is the most important convention in this
repository. See [docs/architecture.md](docs/architecture.md) for the reasoning;
the rules are:

| Put it in | When it is |
| --- | --- |
| `crates/chiploom-core` | A decision: what a value means, whether something is valid, what a diagnostic found. |
| `crates/chiploom-cli` | Presentation for a terminal, or the mapping from a failure to an exit code. |
| `extension/` | Presentation inside VS Code. |

Three consequences worth stating plainly:

* **The core never prints and never exits.** It returns `Result<_, Error>`. If you
  find yourself reaching for `println!` or `std::process::exit` in
  `chiploom-core`, the logic belongs one layer up.
* **Every capability reaches both front ends.** A new core feature gets a CLI
  command *and* an IPC method. Otherwise the editor and the terminal drift apart,
  which is the failure mode this structure exists to prevent.
* **stdout belongs to the protocol.** `chiploom serve --stdio` writes nothing to
  stdout but JSON-RPC frames. All diagnostics go to stderr. There is a test that
  enforces this at `-vvv`; please do not work around it.

## Commits

Conventional Commits, with the affected area as the scope:

```text
feat(build): add incremental dependency graph
fix(flash): retry UF2 enumeration after a bus reset
docs(cli): document the doctor exit codes
test(config): cover relative path overrides
```

Commit at meaningful units of work rather than once per phase. The history is
meant to be readable.

## Tests

Every change needs a test that would have failed before it. Beyond that:

* **Unit tests** live next to the code, in a `mod tests`. They cover decisions.
* **Integration tests** in `crates/chiploom-cli/tests/` run the real binary and
  assert on exit codes and on what reaches stdout versus stderr.
* **Protocol tests** in `extension/test/` drive the real binary over the real
  protocol. This is how Core↔extension communication is verified without an
  editor.

Tests must not read the developer's real home directory. Both suites redirect
`HOME`, the XDG variables and `APPDATA` at a temporary directory; follow the
existing helpers (`chiploom()` in `tests/cli.rs`) rather than inventing a new way.

## Documentation

If you change behaviour, change the document that describes it in the same
commit. `docs/cli.md`, `docs/configuration.md` and `docs/ipc-protocol.md` are
reference documents and are expected to be complete.

## Reporting problems

Open an issue with the output of `chiploom doctor --format json` and
`chiploom version -v`. Those two together identify your platform, your
configuration, your storage locations and the exact binary you ran.

## Security

Please do not open a public issue for a security problem. See
[SECURITY.md](SECURITY.md).

---

## 日本語

* **動かしていないものは完成とみなしません。** 実機の挙動に関わる変更は、検証した
  MCU・ボード・書き込み器を明記してください。
* **`v1.0.0` にダミー実装は残しません。** 動作しないボタン、未実装のハンドラ、
  実機未検証の「対応済み」表示はいずれも認められません。このため Workspace は
  `todo!()` と `unimplemented!()` をコミット対象コードで拒否します。
* **責務分離を守ってください。** 判断は `chiploom-core`、端末向けの表示は
  `chiploom-cli`、エディタ向けの表示は `extension/`。Core は出力も終了もしません。
* **`chiploom serve --stdio` の stdout はプロトコル専用です。** 診断出力は
  すべて stderr へ。`-vvv` でもこれが守られることをテストで検証しています。
* CI と同じ検査（`cargo fmt --all --check`／`cargo clippy ... --deny warnings`／
  `cargo test --workspace`／`extension` の `npm test`）をローカルで通してから
  提出してください。
* コミットメッセージは Conventional Commits 形式で、範囲をスコープに記載してください。
* 不具合報告には `chiploom doctor --format json` と `chiploom version -v` の
  出力を添付してください。
