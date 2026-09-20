# Release notes

One file per published version, named after its tag. The release workflow publishes
the matching file as the GitHub release body and **fails if it is missing** — a
release with no notes is not a release.

| Version | Phase | Notes |
| --- | --- | --- |
| [`v0.1.0-pre.0`](v0.1.0-pre.0.md) | 0 — Foundation | First prerelease. Workspace, core, CLI, extension, Core IPC protocol. |

Every file covers, in both Japanese and English: what the phase added, supported
MCUs and boards, supported operating systems, supported toolchains, what was
actually verified, known issues, changes since the previous version, how to
install, the build artifacts, and the program's real output.

Sections that do not apply yet say so explicitly rather than being left out, so a
reader can tell "not yet" from "forgotten".

See [../release-process.md](../release-process.md) for how one of these is produced.
