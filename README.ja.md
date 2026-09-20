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

## ドキュメント

| 文書 | 内容 |
| --- | --- |
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
