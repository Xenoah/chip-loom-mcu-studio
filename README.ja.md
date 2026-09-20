# Chip Loom MCU Studio

[![CI](https://github.com/Xenoah/chip-loom-mcu-studio/actions/workflows/ci.yml/badge.svg)](https://github.com/Xenoah/chip-loom-mcu-studio/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

マイコン開発を一つのツールで完結させるための統合ツールチェーンです。ビルド・書き込み・
モニタ・デバッグを Windows／macOS／Linux で扱えるようにすることを目標としています。

English version: [README.md](README.md)

> **現在の状態：全11フェーズのうち Phase 0。** Chip Loom は開発過程を公開しながら
> フェーズ単位で構築しています。`v1.0.0` より前に公開されるものはすべて
> **Prerelease** であり、開発履歴・検証履歴として扱われます。完成した製品ではありません。
>
> **このバージョン（`v0.1.0-pre.0`）は、まだビルド・書き込み・モニタ・デバッグを
> 行いません。** 提供するのは、以降のフェーズが載る土台です。すなわち Workspace 構成、
> Core ライブラリ、`chiploom` コマンド、VS Code Extension、そして両者を結ぶ通信プロトコルです。
> 各フェーズの内容は [docs/roadmap.md](docs/roadmap.md)、
> 今回の変更点は [CHANGELOG.md](CHANGELOG.md) を参照してください。

## 現時点で動作するもの

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

| コマンド | 内容 |
| --- | --- |
| `chiploom --version` | バージョンと、ビルド元のコミット。 |
| `chiploom --help` | 全コマンドとオプション。 |
| `chiploom doctor` | この環境で Chip Loom が動作するかを検査し、対処方法を示します。 |
| `chiploom version -v` | Build 情報の全項目。不具合報告に添付してください。 |
| `chiploom config show` | 有効な設定を、そのまま貼り付けられる TOML として出力します。 |
| `chiploom config path` | 設定とデータの読み込み場所をすべて表示します。 |
| `chiploom config check` | 設定ファイルを検証し、無視されたキーを報告します。 |
| `chiploom serve --stdio` | エディタが接続する Core IPC サーバを起動します。 |
| `chiploom completions <shell>` | bash／zsh／fish／PowerShell／Elvish 用の補完スクリプト。 |

VS Code Extension は同じ Core に同じプロトコルで接続し、
**Show Core Version**／**Run Diagnostics**／**Restart Core**／**Show Output** の
4コマンドと、接続中の Core を示すステータスバー表示を提供します。

## インストール

### リリースから

[Releases](https://github.com/Xenoah/chip-loom-mcu-studio/releases) から対象
プラットフォームのアーカイブを取得し、検証・展開して `chiploom` を `PATH` に置きます。

```bash
# Linux／macOS
sha256sum -c chiploom-v0.1.0-pre.0-x86_64-unknown-linux-gnu.tar.gz.sha256
tar xzf chiploom-v0.1.0-pre.0-x86_64-unknown-linux-gnu.tar.gz
sudo install -m755 chiploom-v0.1.0-pre.0-x86_64-unknown-linux-gnu/chiploom /usr/local/bin/
chiploom doctor
```

Windows では `.zip` を展開し、そのフォルダを `Path` に追加してください。

### ソースから

Rust stable 1.88 以降が必要です。Extension をビルドする場合は Node.js 20 以降も必要です。

```bash
git clone https://github.com/Xenoah/chip-loom-mcu-studio.git
cd chip-loom-mcu-studio
cargo build --release
./target/release/chiploom doctor
```

このリポジトリを VS Code で開いている場合、Extension は `target/release` または
`target/debug` のバイナリを自動的に見つけます。開発者が常に「たった今ビルドした Core」
を動かせるようにするためです。

```bash
cd extension
npm install
npm run compile
npm test          # 実際のバイナリに対して実際のプロトコルで通信します
```

## 構成

Chip Loom は「1つの Core と 2つのフロントエンド」という構成です。CLI から使えない機能が
エディタ側にだけ存在することはありません。どちらも同じ Core を経由します。

```text
        ┌──────────────────────┐        ┌──────────────────────┐
        │  VS Code extension   │        │      chiploom CLI    │
        │     (TypeScript)     │        │        (Rust)        │
        └──────────┬───────────┘        └──────────┬───────────┘
                   │ JSON-RPC 2.0 over stdio       │ 直接呼び出し
                   └───────────────┬───────────────┘
                                   ▼
                        ┌─────────────────────┐
                        │    chiploom-core    │
                        │ 設定 · Logging ·    │
                        │ 診断 · IPC          │
                        └─────────────────────┘
```

* `crates/chiploom-core` がすべての判断を行います。出力も終了もしません。
* `crates/chiploom-cli` はその結果を人向けに整形し、終了コードを決めます。
* `extension/` は [docs/ipc-protocol.md](docs/ipc-protocol.md) のプロトコルを話し、
  同じ値をエディタ上に表示します。

設計理由は [docs/architecture.md](docs/architecture.md) に記載しています。

## 設定

設定は優先度の低い順に、組み込み既定値 → ユーザ全体の `config.toml` →
プロジェクトの `chiploom.toml` → `CHIPLOOM_*` 環境変数 → コマンドライン引数の順で
重ねられます。ある層が値に言及しなければ、その下の層の値がそのまま残ります。

```toml
# プロジェクト直下の chiploom.toml
[project]
name = "blinky"

[log]
level = "debug"

[network]
offline = true       # ネットワークに一切接続しない
```

`chiploom config path` で各層の場所を、`chiploom config show --sources` で
結果とその出所を確認できます。全キーの一覧は
[docs/configuration.md](docs/configuration.md) にあります。

## ロードマップ

Chip Loom は全11フェーズで構築します。各フェーズの完了時に、その時点で動作する成果物を
**GitHub Prerelease** として公開するため、開発履歴がそのままリリース履歴になります。
`v1.0.0` が最初の完成製品であり、それ以前のものはすべて開発履歴・検証履歴です。
評価や進捗の追跡のためのもので、依存して使うためのものではありません。

| バージョン | Phase | 提供するもの | 状態 |
| --- | --- | --- | --- |
| [`v0.1.0-pre.0`](https://github.com/Xenoah/chip-loom-mcu-studio/releases/tag/v0.1.0-pre.0) | 0 — 基盤 | Workspace、Core、CLI、Extension、Core IPC、CI、ドキュメント | **公開済み** |
| `v0.2.0-pre.0` | 1 — Target 管理 | MCU Database、Target Pack、Board Profile、メモリマップ、`chiploom target` | 未着手 |
| `v0.3.0-pre.0` | 2 — Toolchain 管理 | 自動ダウンロード、SHA-256 検証、キャッシュ、バージョン固定、オフライン取込 | 未着手 |
| `v0.4.0-pre.0` | 3 — Build Engine | C／C++／アセンブリ、依存グラフ、並列・増分ビルド、`compile_commands.json` | 未着手 |
| `v0.5.0-pre.0` | 4 — Flash Engine | Device 検出、Erase／Program／Verify／Reset、UF2・DFU・UART・AVR ISP・SWD・ESP ROM | 未着手 |
| `v0.6.0-pre.0` | 5 — UART／USB Monitor | ASCII／HEX／Binary、CDC・HID・Bulk、Telemetry、グラフ、MCU 側デバッグライブラリ | 未着手 |
| `v0.7.0-pre.0` | 6 — Debugger | Debug Adapter Protocol、CMSIS-DAP、Breakpoint、DWARF、SVD Peripheral Viewer | 未着手 |
| `v0.8.0-pre.0` | 7 — Framework 統合 | Bare Metal、CMSIS、STM32 HAL、Arduino Core、Pico SDK、ESP-IDF、FreeRTOS | 未着手 |
| `v0.9.0-pre.0` | 8 — Library／Test／解析 | Lockfile 付き Library Manager、Host／MCU テスト、clang-tidy | 未着手 |
| `v0.10.0-pre.0` | 9 — Remote／OTA | Remote Build・Flash・Monitor・Debug、Device 共有、Rollback 付き OTA | 未着手 |
| `v1.0.0-rc.x` | 10 — 完成版検証 | 5 プラットフォーム、ストレス・耐久試験。新機能は追加しない | 未着手 |
| `v1.0.0` | — | 最初の正式リリース | 未着手 |

大きなフェーズでは、検証可能な区切りごとに `v0.4.0-pre.1`、`-pre.2` のように
同一フェーズ内で Prerelease を追加します。

### 進捗

**全11フェーズ中 1 フェーズ完了。** フェーズ数では約 9% ですが、工数ベースでは
3〜5% 程度です。Phase 0 は以降のすべてが乗る土台であり、重量級の Build Engine・
Flash Engine・Debugger はいずれもこの先にあります。

### `v1.0.0` に残さないもの

全フェーズの設計を規定している原則であり、各フェーズの完了条件を「実行して確かめられる
こと」として書いている理由です。

* 未実装のボタンやコマンド
* ダミー処理・placeholder
* `TODO` による主要機能の欠落
* 「将来対応」を前提とした主要機能
* **実機未検証のまま「対応済み」と表示する対象**

最後の項目は、このロードマップの進め方に対する厳しい制約になります。Phase 4・5・6・7・10
の完了条件は物理的なものです。実チップへの書き込み、USB 抜き差しへの耐性、デバッグ
プローブの駆動、5 つの OS とアーキテクチャの組み合わせでの動作。コードの記述と単体
テストはどこでも可能ですが、対応表を埋めるには実機が目の前に必要です。

各フェーズの詳細は [docs/roadmap.md](docs/roadmap.md) にあります。

## ドキュメント

| 文書 | 内容 |
| --- | --- |
| [AGENTS.md](AGENTS.md) | **引継ぎ資料**：現状、壊してはならない不変条件、一度痛い目を見た罠。開発を引き継ぐ場合はまずこれを。 |
| [docs/architecture.md](docs/architecture.md) | Core／CLI／Extension の責務分離とその理由。 |
| [docs/cli.md](docs/cli.md) | 全コマンド・全オプション・終了コード。 |
| [docs/configuration.md](docs/configuration.md) | 全設定キーと環境変数。 |
| [docs/ipc-protocol.md](docs/ipc-protocol.md) | Core IPC プロトコル version 1 の仕様。 |
| [docs/development.md](docs/development.md) | ビルド方法・テスト方法・リポジトリ構成。 |
| [docs/roadmap.md](docs/roadmap.md) | 全11フェーズと各フェーズの成果物。 |
| [docs/release-process.md](docs/release-process.md) | フェーズを Prerelease として公開する手順。 |
| [docs/releases/](docs/releases/) | 公開済み全バージョンのリリースノート。 |

## 開発に参加する

[CONTRIBUTING.md](CONTRIBUTING.md) を参照してください。要点は、`cargo fmt`・
`cargo clippy`・`cargo test`・`npm test` がすべて通ること、そして実機動作に関わる
変更には「どのハードウェアで検証したか」を明記することです。

## ライセンス

Apache License 2.0。[LICENSE](LICENSE) および [NOTICE](NOTICE) を参照してください。
