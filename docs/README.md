# TotalFloat

**Open-source, agent-native CPM scheduling.** TotalFloat is an open-source CPM scheduler that reads and writes Primavera P6 XER files and Microsoft Project XML files, calculates schedules with its own deterministic engine and documented rules, and exposes everything through a CLI and an MCP server so humans and AI agents can work on real project schedules together.

<div class="tf-downloads">
<a class="tf-button" href="https://github.com/ijd5004/totalfloat/releases/latest/download/totalfloat-x86_64-pc-windows-msvc.zip">Download for Windows</a>
<a class="tf-button" href="https://github.com/ijd5004/totalfloat/releases/latest/download/totalfloat-x86_64-unknown-linux-gnu.tar.gz">Download for Linux</a>
<span class="tf-button tf-button-soon" aria-disabled="true">macOS installer coming soon</span>
</div>

The buttons download the latest release for x86-64: a `.zip` file for Windows and a `.tar.gz` file for Linux. The quickest way to install is [one command](guide/install.md#install-with-one-command), which downloads and checks the release and puts `totalfloat` on your `PATH`. To install from the download instead, follow [Install by hand](guide/install.md#install-by-hand).

On a Mac, you can [build TotalFloat from source](guide/install.md#build-from-source) today: install [Rust](https://rustup.rs) and run `cargo install --path crates/cli` from a copy of the repository.

TotalFloat v0.1 is an early release. It's usable on real schedules, but read the [limitations](guide/limitations.md) before you rely on its dates.

## Where to go next

- [Start here](guide/start-here.md) if you schedule projects and don't write code.
- [Install](guide/install.md) TotalFloat on Windows or Linux.
- [User guide](guide/README.md): every command, with examples.
- [Using TotalFloat from an agent](agents.md): connect an AI agent over MCP.
- [Rules](rules/README.md): TotalFloat's calculation rules and their sources.
- [Source code on GitHub](https://github.com/ijd5004/totalfloat)
