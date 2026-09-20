# Chip Loom — handover

Context for whoever picks this up next, human or agent. It covers where the project
stands, the rules that must keep holding, the reasons behind the choices that are not
self-evident from the code, and the traps that have already cost time once.

Read this before the code. `docs/architecture.md` explains the design;
this file explains the *state* and the *pitfalls*.

日本語の要点は [末尾](#日本語) にあります。

---

## 1. Where the project stands

| | |
| --- | --- |
| Phase | **1 of 11 complete** — Phase 0, Foundation (phases are numbered 0–10) |
| Released | [`v0.1.0-pre.0`](https://github.com/Xenoah/chip-loom-mcu-studio/releases/tag/v0.1.0-pre.0), GitHub **Prerelease**, 12 assets |
| Branch | `claude/vigilant-keller-dbqt8j` — also the repository's **default branch** |
| Tag | `refs/tags/v0.1.0-pre.0` → `bab639d` |
| CI | green on `ubuntu-latest`, `macos-latest`, `windows-latest` |
| Size | 73 files · Rust 6,575 lines · TypeScript 1,309 · Markdown 2,964 |
| Tests | **159** — 143 Rust (83 core unit, 34 CLI unit, 25 CLI integration, 1 doctest) + 16 TypeScript |

Next: **Phase 1, Target system**, releasing as `v0.2.0-pre.0`. See §8.

### What works

`chiploom --version`, `--help`, `doctor`, `version -v`,
`config show | path | check`, `serve --stdio`, `completions <shell>`.
The VS Code extension connects to the core and contributes four commands, a status
bar item and an output channel.

### What does not exist yet

No MCU database, no toolchain management, no build, no flash, no monitor, no
debugger. Those are Phases 1–6. Nothing in the repository claims otherwise, and
nothing added should.

---

## 2. Invariants — do not break these

These are not style preferences. Each one is load-bearing, and most are enforced by a
test that will fail if you violate it.

### 2.1 The core never prints and never exits

`chiploom-core` returns `error::Error`. It has no `println!`, no
`std::process::exit`. The CLI decides how to render a failure and which
[`ExitCode`] to hand the shell; the IPC server decides which JSON-RPC code to send.

*Why*: one `doctor` implementation becomes terminal text, a JSON document and a
JSON-RPC result, with no duplicated logic and no way for the three to disagree.

*Enforced by*: `clippy::print_stdout` is a workspace warning. Only
`crates/chiploom-cli/src/main.rs` allows it, with a comment saying why.

### 2.2 stdout carries only protocol frames

`chiploom serve --stdio` writes nothing to stdout but newline-delimited JSON-RPC.
Every diagnostic goes to stderr, at every verbosity.

*Enforced by*: `serve_keeps_stdout_free_of_logs_even_at_maximum_verbosity` in
`crates/chiploom-cli/tests/cli.rs` runs the server at `-vvv` and parses **every**
stdout line as JSON. Do not work around it.

### 2.3 Every capability reaches both front ends

A new core feature gets a CLI command *and* an IPC method, and the method name goes
in `ipc::protocol::METHODS`. Otherwise the editor and the terminal drift apart, which
is the failure this structure exists to prevent.

### 2.4 Configuration layers merge as patches

Layers are deserialized into `ConfigPatch` — every field `Option` — merged in
precedence order, and resolved against the defaults only at the end.

*Why*: if each file were deserialized into a fully-defaulted `Config`, a project file
that says nothing about `log.level` would still overwrite the global value with the
default. The patch form makes "says nothing" distinguishable from "says the default".

Unknown keys are **collected, not rejected**, so a project pinned to an older Chip
Loom can still read a `chiploom.toml` written by a newer one.
`chiploom config check` is where the user hears about a key that did nothing.

### 2.5 Wire field naming

camelCase everywhere on the wire, with **one deliberate exception**: the `config`
object mirrors `chiploom.toml` key-for-key (`timeout_secs`, `data_dir`) so a
serialized `Config` can be written straight back into a file. Everything else,
`paths` included, is camelCase. Documented in `docs/ipc-protocol.md`.

### 2.6 No placeholders

`clippy::todo`, `clippy::unimplemented` and `clippy::dbg_macro` are **denied** at the
workspace level; `unsafe_code` is **forbidden**. `v1.0.0` must contain no dummy
behaviour and nothing marked supported that has not run on real hardware.

### 2.7 Tests never touch the developer's real configuration

Integration tests isolate themselves with `CHIPLOOM_CONFIG_DIR`,
`CHIPLOOM_DATA_DIR` and `CHIPLOOM_CACHE_DIR` — **not** with `HOME` or `%APPDATA%`.
See §6.1 for why that distinction is not optional. Use the `chiploom()` helper in
`tests/cli.rs`; unit tests construct `Paths` explicitly.

---

## 3. Technology, and why

| Choice | Reason |
| --- | --- |
| Rust 2024, MSRV 1.88 | Let-chains are used. Raise the MSRV deliberately, not by accident. |
| `clap` 4 derive | `Cli::command().debug_assert()` catches flag conflicts at test time. |
| `toml` 1 + `serde` | `config show` emits TOML that parses back, so its output is pasteable. |
| `tracing` + `tracing-subscriber` | Structured fields, an env filter, and a JSON file sink — all on stderr. |
| `directories` 6 | Platform locations. **Read §6.1 before assuming how it behaves.** |
| `sysinfo` **0.38** | 0.39 requires rustc 1.95. Pinned deliberately; do not bump without raising the MSRV. |
| `which` 8 | `git` / `code` discovery in `doctor`. |
| JSON-RPC 2.0 over newline-delimited stdio | JSON escapes newlines, so line framing is sufficient and shell-debuggable. |
| TypeScript, `tsc` only, no bundler | Three dev dependencies. `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes` all on. |
| `node:test` | No test framework dependency. **Read §6.2 — it has a sharp edge.** |

### Why a protocol session rather than shelling out per command

Phase 0 does not need one. It exists now because Phases 3, 5 and 6 need streaming
output, long-lived state (a halted core, breakpoints, a symbol table) and
cancellation. Retrofitting a session onto a subprocess-per-call design later would
mean rewriting both front ends.

---

## 4. Code map

```text
crates/chiploom-core/          Every decision. Never prints, never exits.
  build.rs                     Captures version, commit, dirty, target, rustc, timestamp
  src/config/
    model.rs                   Resolved Config — always fully populated
    patch.rs                   ConfigPatch (all-Option) + merge + resolve
    mod.rs                     Loader, layer precedence, Source provenance
  src/doctor/
    mod.rs                     Report, Status, Summary, the runner
    checks.rs                  The 11 individual checks
  src/ipc/
    protocol.rs                Wire types, METHODS, error codes
    server.rs                  Read-dispatch-write loop, generic over BufRead/Write
  src/error.rs                 Error, ExitCode (values asserted by test)
  src/logging.rs               tracing setup — stderr only
  src/paths.rs                 Platform locations, project discovery, write probe
  src/version.rs               BuildInfo, PROTOCOL_VERSION

crates/chiploom-cli/           The chiploom binary. All human formatting.
  src/cli.rs                   clap declarations
  src/render.rs                Every table, colour and wrap decision
  src/commands/                One module per command
  tests/cli.rs                 Runs the real binary; asserts exit codes and streams

extension/                     VS Code extension
  src/core/protocol.ts         TypeScript mirror of ipc/protocol.rs
  src/core/client.ts           JSON-RPC client — no vscode import, so it is testable
  src/core/locate.ts           Binary discovery — no vscode import
  src/session.ts               Core lifetime: start, restart, stop
  src/extension.ts             Activation and commands
  test/protocol.test.ts        Drives the real binary over the real protocol
```

`client.ts` and `locate.ts` **must not** import `vscode`. That is what lets
`protocol.test.ts` drive them in plain Node with no editor.

---

## 5. Verifying a change

Exactly what CI runs. Run all of it before pushing.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- --deny warnings   # pedantic is on
cargo test --workspace

cargo build --bin chiploom        # the extension tests need a real binary
cd extension && npm ci && npm run compile && npm test
```

CI additionally runs the three acceptance commands directly on each platform, so a
change that breaks `doctor` fails even if no test covered it.

To drive the protocol by hand:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientName":"shell","protocolVersion":1}}' \
  '{"jsonrpc":"2.0","id":2,"method":"core/doctor"}' \
  '{"jsonrpc":"2.0","method":"exit"}' \
  | CHIPLOOM_LOG=chiploom_core::ipc=trace ./target/debug/chiploom serve --stdio
```

---

## 6. Traps — every one of these has already cost time

### 6.1 `%APPDATA%` does **not** relocate storage on Windows

`directories` uses the Known Folder API on Windows, not environment variables.
Overriding `%APPDATA%` or `%LOCALAPPDATA%` achieves nothing. Four integration tests
were silently reading — and writing to — the CI runner's real user profile because of
this.

Worse, setting `USERPROFILE` to a temporary directory makes discovery **fail
outright**, which turned 4 failures into 21.

The fix, and the rule now: `CHIPLOOM_CONFIG_DIR`, `CHIPLOOM_DATA_DIR` and
`CHIPLOOM_CACHE_DIR` are the only portable way to relocate storage. Setting all three
also makes Chip Loom work on a host with no discoverable home directory at all
(`Paths::from_platform_or_overrides`); with only some set it stops and names the ones
missing. `CHIPLOOM_CONFIG_DIR` is environment-only by necessity — it decides which
file to read, so it cannot come from a file.

### 6.2 `node --test` and globs

Node 20 does not expand glob patterns for `--test`, and CI installs Node 20.
Worse, **passing it a directory exits 0 having run nothing** — a green build with zero
tests.

So each test file is named explicitly in the `test` script, and
`npm run check:tests` fails the run if a compiled `*.test.js` is not in that list.
When you add a test file, add it to both.

### 6.3 macOS path symlinks

`/var` is a symlink to `/private/var`, and `find_project_root` canonicalizes. Compare
canonicalized paths on both sides, or the comparison passes on Linux and fails on
macOS. See `relative_path_overrides_anchor_to_the_project_root`.

### 6.4 macOS temporary paths are long

A `doctor` *detail* line carries a path and is deliberately **not** wrapped — a path
reads worse broken across lines than running past the margin. Only the *hint* is
wrapped. Do not assert a width limit on every line of a report.

### 6.5 `doctor` warns on a dirty working tree, so tests must not pin the verdict

`core.build` reports [`Status::Warn`] when the binary was built from a tree with
uncommitted tracked changes — deliberately, because such a binary matches no released
version. That changes the verdict line from `Ready.` to
`Ready. The warnings above are safe to ignore for now.`

A test asserting the exact verdict from a *real* `doctor` run therefore passes in CI
(clean checkout) and fails for anyone mid-edit. Assert verdict wording against a
`Report` the test builds itself — `report_of()` in `render.rs` — and keep assertions
on real runs to things that do not depend on the tree.

### 6.6 `clap` prefixes the binary name to the version

`Command::version()` output is `"<bin> <value>"`. Pass
`build_info().version_line()` (no name) and not `short_line()` (with name), or you
get `chiploom chiploom 0.1.0-pre.0`.

### 6.7 A global counted flag collides with a per-command boolean

`-v` is a global `ArgAction::Count`. A subcommand declaring its own `--verbose` bool
panics at parse time with "Mismatch between definition and access". `chiploom
version` therefore takes its detail from `global.verbose > 0`.
`Cli::command().debug_assert()` catches this class of bug — keep that test.

### 6.8 `unreachable_pub` in a binary crate

Nothing in a binary crate is reachable from outside it, so `pub` items in private
modules warn. Everything in `chiploom-cli` is `pub(crate)`.

### 6.9 CI runner labels

`ubuntu-24.04-arm` and `macos-13` were tried and replaced: availability varies and a
release must not fail for want of a runner image. The release workflow now uses only
`ubuntu-latest`, `macos-latest` and `windows-latest`, cross-compiling
`aarch64-unknown-linux-gnu` and `x86_64-apple-darwin`. Those two cannot be executed
where they are built; the workflow warns for each and the release notes say so.

---

## 7. Releasing, and the environment limits on it

Full procedure: `docs/release-process.md`. **Pushing is not the end of a phase** —
the phase is complete when the release exists on GitHub with notes and artifacts.

### 7.1 What this environment cannot do

Discovered by trying. Do not re-litigate; use the path that works.

| Operation | Result |
| --- | --- |
| `git push origin refs/tags/...` | **403** from the egress proxy |
| `POST /repos/.../git/refs` | **403** "Write access to this GitHub API path is not permitted" |
| `POST /repos/.../releases` | **403** "Creating, editing, or deleting releases is not permitted for this session type" |
| `POST /actions/workflows/.../dispatches` | **403** "Resource not accessible by integration" |
| `POST /repos/.../dispatches` | **204 — works** |
| `git push` to the branch | works |

So the release is created **from inside GitHub Actions**, triggered by a
`repository_dispatch`. The workflow accepts that as a first-class trigger and creates
the tag as it publishes:

```bash
curl -X POST \
  -H "Authorization: Bearer $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github+json" \
  -H "Content-Type: application/json" \
  -d '{"event_type":"release","client_payload":{"tag":"v0.2.0-pre.0"}}' \
  https://api.github.com/repos/Xenoah/chip-loom-mcu-studio/dispatches
```

`repository_dispatch` only fires workflows on the **default branch** — currently
`claude/vigilant-keller-dbqt8j`. If the default branch changes, the workflow file
must be there too.

### 7.2 The release workflow refuses to ship something broken

`.github/workflows/release.yml` validates before building: the tag must match
`vMAJOR.MINOR.PATCH[-pre.N]`, `docs/releases/<tag>.md` must exist, and the versions
in `Cargo.toml` **and** `extension/package.json` must both equal the tag. It then
runs `--version`, `--help` and `doctor` on every binary it can execute, verifies
every checksum, and checks all 12 expected assets are present.

When bumping a version, change all of these in one commit: root `Cargo.toml`,
`extension/package.json`, `Cargo.lock`, `extension/package-lock.json`,
`CHANGELOG.md`, `docs/roadmap.md`, `README.md` and `README.ja.md`.

### 7.3 Release notes

Bilingual, ten sections, listed in `docs/release-process.md`. Links must be
**absolute and pinned to the tag** — the file is rendered as the release body, where
relative paths do not resolve. A section that does not apply yet says so explicitly
rather than being omitted, so a reader can tell "not yet" from "forgotten".

---

## 8. Phase 1 — the next piece of work

Target: `v0.2.0-pre.0`. **Completion condition**: supported MCUs are described in one
format, and a new MCU of an existing family can be added by adding a target pack,
*without significant changes to the core*. That last clause is the real test — design
the pack format so the core stays generic.

Deliverables: MCU database; target pack and board profile formats; MCU search and
information display; memory maps; CPU architecture detection; toolchain requirement
resolution; SVD and other metadata; boards kept separate from MCUs.

```bash
chiploom target list
chiploom target search STM32G4
chiploom target info STM32G431CBT6
```

Where it lands, following §2.3: a `targets` module in the core, `chiploom target` in
the CLI, `target/list`, `target/search` and `target/info` added to
`ipc::protocol::METHODS` and to `extension/src/core/protocol.ts`, plus reference
documentation in `docs/`.

Packs install under `Paths::target_packs_dir()` (`<data>/targets/`), which already
exists and is write-probed by `doctor`.

### Feasibility note for Phases 2 and 3

Checked, because it changes how Phase 2 must be designed:

| Host | Reachable |
| --- | --- |
| `objects.githubusercontent.com` (GitHub Releases assets) | **yes** |
| `archive.ubuntu.com` | **yes** |
| `developer.arm.com` | no |
| `dl.espressif.com` | no |
| `download.01.org` | no |

ARM, AVR, RISC-V, LLVM and Xtensa toolchains all have builds distributed through
GitHub Releases (xpack-dev-tools, espressif/crosstool-NG, llvm/llvm-project). Sourcing
from there makes Phase 2's completion condition demonstrable in a sandboxed
environment, and it is a defensible choice regardless — those artifacts are
checksummed and versioned.

### The hardware wall

Phases 4, 5, 6, 7 and 10 have physical completion conditions: writing to real
silicon, surviving a USB unplug, driving a CMSIS-DAP probe, running on five OS and
architecture combinations. Their code can be written and unit-tested anywhere. Their
support tables **cannot be filled in** without the hardware. Per §2.6, do not mark a
target supported on the strength of code review — leave it listed as unverified and
say so in the release notes.

---

## 9. Open issues

Also in the `v0.1.0-pre.0` release notes, which is the user-facing record.

1. **The extension has never been loaded in a live VS Code window.** Its protocol
   layer — discovery, handshake, every method, error handling, shutdown — is verified
   by automated tests against the real binary. Activation in an Extension Development
   Host, and the status bar and output channel as VS Code renders them, are not.
2. **Two artifacts were never executed**: `x86_64-apple-darwin` and
   `aarch64-unknown-linux-gnu` are cross-compiled. They compile and link; nothing more
   is claimed.
3. **The `.vsix` is not on the Marketplace.** Install from the release asset.
4. **The IPC server is single-threaded and synchronous**, with no cancellation.
   Sufficient for methods that return in milliseconds; Phase 3 and Phase 5 will need
   streaming and cancellation, and that is a real change to `ipc/server.rs`.
5. **`doctor --online` tests TCP reachability only** — no proxy, TLS-interception or
   authentication check. Phase 2 should extend it.
6. **macOS artifacts are unsigned**, so Gatekeeper quarantines them. Users need
   `xattr -d com.apple.quarantine`. Signing needs an Apple Developer identity.
7. **`commit` reads `unknown` in a build from a source archive** with no `.git`. The
   release workflow uses `fetch-depth: 0`, so published binaries carry it.

---

## 10. Conventions

* **Commits**: Conventional Commits with the area as scope —
  `feat(build): add dependency graph`. Commit at meaningful units, not once per phase.
* **Comments** explain *why*, not *what*. If a line needs a comment to say what it
  does, rewrite the line.
* **Tests**: every change needs a test that would have failed before it. Test names
  are sentences describing the guarantee, not the function under test.
* **Docs**: `docs/cli.md`, `docs/configuration.md` and `docs/ipc-protocol.md` are
  reference documents and are expected to be **complete**. Something that exists and
  is not documented there is a bug. Update them in the same commit as the behaviour.
* **Bilingual**: `README.md` / `README.ja.md` stay in step. `CONTRIBUTING.md`,
  `SECURITY.md` and `CODE_OF_CONDUCT.md` are English with a Japanese section.

---

## 日本語

### 現状

* **全11フェーズ中 Phase 0（基盤）のみ完了。** `v0.1.0-pre.0` を GitHub Prerelease として
  公開済み（成果物12点）。CI は Windows／macOS／Linux で green。
* 作業ブランチ `claude/vigilant-keller-dbqt8j`（リポジトリの既定ブランチ）。
* テスト 157 件（Rust 141、TypeScript 16）。
* 次は **Phase 1（Target 管理）**、`v0.2.0-pre.0`。詳細は §8。
* ビルド・書き込み・モニタ・デバッグはまだ存在しません。存在するかのような記述も
  追加しないでください。

### 壊してはならない不変条件（§2）

1. **Core は出力も終了もしない。** `Error` を返すだけ。表示と終了コードは CLI の責務。
2. **`serve --stdio` の stdout はプロトコル専用。** 診断は必ず stderr。`-vvv` でも
   stdout 全行が JSON であることをテストが検証しています。
3. **機能は必ず CLI と IPC の両方に出す。** 片方だけに足すとエディタと端末が乖離します。
4. **設定は「全項目 Option のパッチ」として合成する。** 既定値入りの構造体に
   デシリアライズすると、言及していない項目を既定値で上書きしてしまいます。
5. **ワイヤ形式は camelCase。** ただし `config` オブジェクトだけは `chiploom.toml` の
   キー名（`timeout_secs` 等）を保ちます。ファイルに書き戻せるようにするためです。
6. **ダミー実装を残さない。** `todo!()`／`unimplemented!()`／`dbg!()` は deny、
   `unsafe` は forbid。実機未検証の対象を「対応済み」と書かないこと。
7. **テストは開発者の実設定を読まない。** 隔離は `CHIPLOOM_CONFIG_DIR`／
   `CHIPLOOM_DATA_DIR`／`CHIPLOOM_CACHE_DIR` で行います（`HOME` や `%APPDATA%` では**なく**）。

### 一度痛い目を見た罠（§6）

* **Windows で `%APPDATA%` を書き換えても保存先は移動しません。** `directories` は
  Known Folder API を使います。これが原因で統合テスト4件が CI ランナーの実ユーザー
  プロファイルを読み書きしていました。さらに `USERPROFILE` を一時ディレクトリに
  向けると discovery 自体が失敗し、失敗が4件から21件に増えました。
* **Node 20 は `--test` の glob を展開しません。** ディレクトリを渡すと「0 件実行して
  成功終了」します。テストファイルは明示列挙し、`npm run check:tests` が列挙漏れを
  検出します。テストファイルを追加したら両方を更新してください。
* **macOS の `/var` は `/private/var` へのシンボリックリンク。** パス比較は両辺を
  canonicalize すること。Linux では通り macOS で落ちます。
* **`doctor` は「未コミットの変更があるツリーでビルドされたバイナリ」を warn します。**
  意図的な挙動（そのバイナリはどのリリースにも一致しないため）ですが、判定行が
  `Ready.` から `Ready. The warnings above are safe to ignore...` に変わります。
  実際の `doctor` 実行に対して判定行を厳密に照合するテストは、CI（クリーンな checkout）では
  通り、編集中の開発者の手元では落ちます。判定文言はテスト内で組み立てた `Report`
  （`render.rs` の `report_of()`）に対して検証してください。
* **`clap` はバージョン文字列の前にバイナリ名を付けます。** `version_line()`（名前なし）を
  渡すこと。`short_line()` を渡すと `chiploom chiploom 0.1.0-pre.0` になります。
* **グローバルな `-v`（カウント）とサブコマンドの `--verbose`（bool）は衝突して panic します。**

### リリース手順と、この環境の制約（§7）

* **push だけではフェーズ完了になりません。** Tag・Release Notes・成果物添付・
  Prerelease 公開までが完了作業です。
* この環境では **tag の push・Releases API・workflow_dispatch がいずれも 403** です。
  唯一通るのは **`repository_dispatch`（204）** で、リリースワークフローはこれを
  正式なトリガーとして受け付け、公開時に tag を作成します。コマンドは §7.1 に。
* `repository_dispatch` は**既定ブランチ上のワークフローしか起動しません**。
* リリースワークフローは、tag 形式・Release Notes の存在・`Cargo.toml` と
  `extension/package.json` のバージョン一致を**ビルド前に**検証し、実行可能な成果物に
  対して `--version`／`--help`／`doctor` を走らせ、チェックサムと12点の成果物を
  確認してから公開します。
* Release Notes のリンクは**絶対 URL かつ tag 固定**にしてください。リリース本文として
  描画されるため、相対パスは解決されません。

### 既知の課題（§9）

1. Extension を**実際の VS Code ウィンドウで読み込んだことはありません**。プロトコル層は
   実バイナリに対する自動テストで検証済みですが、activate 以降の描画は未検証です。
2. `x86_64-apple-darwin` と `aarch64-unknown-linux-gnu` は**クロスコンパイルで一度も
   実行していません**。
3. `.vsix` は Marketplace 未公開。4. IPC サーバは単一スレッド・同期でキャンセル機構なし
   （Phase 3／5 で要改修）。5. `doctor --online` は TCP 到達性のみ。
6. macOS 版は未署名（`xattr -d com.apple.quarantine` が必要）。
7. `.git` の無いソースアーカイブからのビルドでは `commit` が `unknown`。

### Phase 2 以降の実現可能性

`developer.arm.com`・`dl.espressif.com` は遮断されていますが、
**`objects.githubusercontent.com`（GitHub Releases）と `archive.ubuntu.com` は到達可能**です。
ARM／AVR／RISC-V／LLVM／Xtensa はいずれも GitHub Releases 経由の配布版があるため、
取得元をそこに向ければ Phase 2 の完了条件をこの環境で実証できます。

Phase 4・5・6・7・10 の完了条件は物理的（実チップ書き込み、USB 抜き差し耐性、
デバッグプローブ駆動、5 プラットフォーム）です。コードは書けますが**対応表は実機なしに
埋められません**。コードレビューだけで「対応済み」にしないでください。
