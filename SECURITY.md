# Security policy

## Supported versions

Chip Loom has not reached `v1.0.0`. Until it does, only the most recent
prerelease receives fixes. Please reproduce a problem on the latest release
before reporting it.

## Reporting a vulnerability

Please report privately rather than in a public issue, using GitHub's
[private vulnerability reporting](https://github.com/Xenoah/chip-loom-mcu-studio/security/advisories/new)
for this repository.

Include:

* what an attacker can achieve, and what access they need to start;
* the output of `chiploom version -v`;
* a reproduction, ideally as a sequence of `chiploom` commands.

You will get an acknowledgement, and an assessment once the report has been
reproduced. If a fix is needed, it ships in the next prerelease and the advisory
is published alongside it.

## What is in scope

Chip Loom downloads and executes compiler toolchains, talks to USB devices and,
from Phase 9, exposes remote operations. The areas most worth your attention:

* **Toolchain and package integrity** — anything that lets an unverified archive
  be installed, or that weakens the SHA-256 verification of a download.
* **Path handling** — an archive entry or a configuration value that writes
  outside the intended directory.
* **Configuration and project files** — a `chiploom.toml` in a repository you
  cloned must not be able to execute code or reach files outside the project.
* **Remote and OTA operations** (Phase 9 onwards) — authentication, transport
  encryption, and anything that lets one device's session reach another's.

## What is not a vulnerability

* Chip Loom running a compiler you configured it to run.
* A missing feature, or a target that is not yet supported.
* Anything that requires an attacker to already have write access to your
  machine's Chip Loom data directory.

## 日本語

セキュリティ上の問題は公開 Issue ではなく、GitHub の非公開脆弱性報告機能から
お知らせください。攻撃者が何を達成できるか、その前提となる権限、
`chiploom version -v` の出力、再現手順を添えてください。
