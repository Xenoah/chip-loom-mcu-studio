# Chip Loom documentation

| Document | Contents |
| --- | --- |
| [../AGENTS.md](../AGENTS.md) | **Handover**: current state, invariants, traps. Read first when picking the project up. |
| [architecture.md](architecture.md) | How the core, the CLI and the extension divide the work, and why. |
| [cli.md](cli.md) | Reference: every command, flag and exit code. |
| [configuration.md](configuration.md) | Reference: every configuration key and environment variable. |
| [ipc-protocol.md](ipc-protocol.md) | Reference: the Core IPC protocol, version 1. |
| [development.md](development.md) | Building, testing, and the repository layout. |
| [roadmap.md](roadmap.md) | The eleven phases and what each one delivers. |
| [release-process.md](release-process.md) | How a finished phase becomes a published prerelease. |
| [releases/](releases/) | Release notes for every published version. |

## Which document answers which question

* *How do I run this?* — the [README](../README.md).
* *What does this flag do?* — [cli.md](cli.md).
* *Why is this value what it is?* — [configuration.md](configuration.md), and
  `chiploom config show --sources`.
* *How does the extension talk to the core?* — [ipc-protocol.md](ipc-protocol.md).
* *Where does my change belong?* — [architecture.md](architecture.md).
* *What has already gone wrong here?* — [../AGENTS.md](../AGENTS.md) §6.
* *When will Chip Loom do X?* — [roadmap.md](roadmap.md).

## Conventions

Reference documents (`cli.md`, `configuration.md`, `ipc-protocol.md`) are expected
to be **complete**: if something exists and is not documented there, that is a
bug. Anything not yet implemented is marked as such and appears in
[roadmap.md](roadmap.md) under the phase that will deliver it -- never as though
it already worked.
