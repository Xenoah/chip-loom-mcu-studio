# Release process

Every phase ends with a published prerelease. Pushing commits is **not** the end of
a phase; the phase is complete when the release exists on GitHub with notes and
artifacts attached.

## Versions

| Tag shape | What it is |
| --- | --- |
| `v0.N.0-pre.M` | Phase N−1 complete (or a verifiable milestone inside it). Published as a **prerelease**. |
| `v1.0.0-rc.M` | A Phase 10 release candidate. Published as a **prerelease**. |
| `v1.0.0` | The first finished product. Published as a **stable release**. |

Only a tag with no pre-release suffix is published as stable. The release workflow
derives this from the tag itself — `prerelease: contains(tag, '-')` — so it cannot
be got wrong by hand.

The workspace version in `Cargo.toml` matches the tag exactly, including the
suffix, so `chiploom --version` and the tag are the same string.

## Completing a phase

### 1. Verify

Everything CI runs, on every platform you have access to:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- --deny warnings
cargo test --workspace

cargo build --release
cd extension && npm ci && npm run compile && npm test
```

Then the phase's own completion conditions from [roadmap.md](roadmap.md), by hand,
on the artifact you are about to publish. For Phase 0 that means running
`chiploom --version`, `chiploom --help` and `chiploom doctor` from the built
release binary, and confirming the extension connects to it.

Write down what you actually verified, and on what. It goes in the release notes,
and a claim you cannot support does not.

### 2. Set the version

Update, in one commit:

* `version` in the root `Cargo.toml` (the workspace inherits it);
* `version` in `extension/package.json`;
* `Cargo.lock`, via `cargo build`;
* `extension/package-lock.json`, via `npm install`;
* `CHANGELOG.md`;
* the status table in [roadmap.md](roadmap.md).

### 3. Write the release notes

Create `docs/releases/<tag>.md`. The release workflow refuses to publish without
it, deliberately: a release with no notes is not a release.

Every prerelease's notes contain, **in both Japanese and English**:

1. what this phase added;
2. supported MCUs and boards;
3. supported operating systems;
4. supported toolchains;
5. what was actually verified, and on what hardware;
6. known issues;
7. changes since the previous version;
8. how to install;
9. what the build artifacts are;
10. screenshots or terminal output.

"Updated." is not release notes. If a section does not apply yet — there are no
supported MCUs before Phase 1 — say so explicitly rather than omitting it. A
reader has to be able to tell "not yet" from "forgot to mention".

### 4. Commit and push

```bash
git status
git add .
git commit -m "feat: complete phase N — <what it delivers>"
git push -u origin <branch>
```

Inside a large phase, commit at meaningful units rather than once at the end:

```text
feat(build): add dependency graph
feat(build): add parallel compilation
feat(build): add incremental cache
fix(build): detect changed headers correctly
test(build): add cross-platform build tests
```

### 5. Tag

Confirm GitHub has the commit you tested, then:

```bash
git tag -a v0.N.0-pre.M -m "Phase N: <name>"
git push origin v0.N.0-pre.M
```

### 6. Publish

The tag triggers `.github/workflows/release.yml`, which:

1. builds `chiploom` for all five target triples;
2. runs `--version` and `doctor` on each native binary — a broken artifact is never
   published;
3. packages each build with `LICENSE`, `NOTICE`, both READMEs, `CHANGELOG.md` and
   `docs/`, as `.tar.gz` or `.zip`;
4. writes a `.sha256` next to every archive;
5. builds the `.vsix`;
6. creates the release from `docs/releases/<tag>.md`, marked prerelease unless the
   tag is a bare `vX.Y.Z`.

### 7. Confirm

Check the published release yourself:

* every expected artifact is attached, with its checksum;
* the notes render correctly, in both languages;
* it is marked prerelease (or, for `v1.0.0`, is not);
* download one archive, verify its checksum, unpack it and run
  `chiploom doctor` from it.

The phase is complete when that last step passes.

## Target triples

| Triple | Runner | Archive | Smoke-tested on the runner |
| --- | --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `.tar.gz` | Yes |
| `aarch64-unknown-linux-gnu` | `ubuntu-latest` (cross) | `.tar.gz` | No — cross-compiled |
| `aarch64-apple-darwin` | `macos-latest` | `.tar.gz` | Yes |
| `x86_64-apple-darwin` | `macos-latest` (cross) | `.tar.gz` | No — no Rosetta on the runner |
| `x86_64-pc-windows-msvc` | `windows-latest` | `.zip` | Yes |

Only the three universally-available runner labels are used, so a release never
fails for want of a runner image. The two cross-compiled targets cannot be executed
where they are built; the workflow emits a warning for each, and the release notes
must say which artifacts were run and which were not.

## Publishing by hand

When the workflow cannot run, the same result has to be produced by hand — the
release is what matters, not the automation:

```bash
cargo build --release --locked --bin chiploom
./target/release/chiploom --version
./target/release/chiploom doctor

version=v0.N.0-pre.M
target=$(rustc -vV | sed -n 's/^host: //p')
name="chiploom-${version}-${target}"
mkdir -p "dist/${name}"
cp target/release/chiploom LICENSE NOTICE README.md README.ja.md CHANGELOG.md "dist/${name}/"
cp -r docs "dist/${name}/docs"
( cd dist && tar czf "${name}.tar.gz" "${name}" \
  && sha256sum "${name}.tar.gz" > "${name}.tar.gz.sha256" )
```

Then create the release from `docs/releases/<tag>.md`, attach the archive and its
checksum, and mark it as a prerelease. Say in the notes which platforms were built
by hand and which were not — an artifact that does not exist must not be implied.

## 日本語

* push だけではフェーズ完了とみなしません。Tag・Release Notes・Build 成果物の添付・
  Prerelease 公開までがフェーズの完了作業です。
* バージョンは `Cargo.toml`・`extension/package.json`・両ロックファイル・
  `CHANGELOG.md`・`docs/roadmap.md` を同一コミットで更新します。
* Release Notes は日本語と英語の両方で、追加機能／対応MCU・Board／対応OS／
  対応Toolchain／動作確認内容／既知の問題／前バージョンからの変更／インストール方法／
  Build 成果物／スクリーンショットを記載します。「更新しました」の一文では足りません。
* 該当がない項目は省略せず「まだ無い」と明記します。読者が「未対応」と「書き忘れ」を
  区別できる必要があります。
* Prerelease は `-` を含むタグから自動判定されます。`vX.Y.Z` の形だけが正式リリースです。
