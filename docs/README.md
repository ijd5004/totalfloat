# TotalFloat

**Open-source, agent-native CPM scheduling.** TotalFloat is an open-source CPM scheduler that reads and writes Primavera P6 XER files and Microsoft Project XML files, calculates schedules with its own deterministic engine and documented rules, and exposes everything through a CLI and an MCP server so humans and AI agents can work on real project schedules together.

<div class="tf-downloads">
<a class="tf-button" href="https://github.com/ijd5004/totalfloat/releases/latest">Download for Windows</a>
<a class="tf-button" href="https://github.com/ijd5004/totalfloat/releases/latest">Download for Linux</a>
<span class="tf-button tf-button-soon" aria-disabled="true">macOS installer coming soon</span>
</div>

The Windows and Linux buttons open the latest release. Windows takes the `.zip` file and Linux the `.tar.gz` file, both for x86-64. The [install guide](guide/install.md) shows how to unpack it and put `totalfloat` on your `PATH`.

On a Mac, you can [build TotalFloat from source](guide/install.md#build-from-source) today: install [Rust](https://rustup.rs) and run `cargo install --path crates/cli` from a copy of the repository.

TotalFloat v0.1 is an early release. It's usable on real schedules, but read the [limitations](guide/limitations.md) before you rely on its dates.

## Where to go next

- [Start here](guide/start-here.md) if you schedule projects and don't write code.
- [Install](guide/install.md) TotalFloat on Windows or Linux.
- [User guide](guide/README.md): every command, with examples.
- [Using TotalFloat from an agent](agents.md): connect an AI agent over MCP.
- [Rules](rules/README.md): TotalFloat's calculation rules and their sources.
- [Source code on GitHub](https://github.com/ijd5004/totalfloat)
