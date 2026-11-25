# Start here

This page is for people who plan and update project schedules and don't write code. It takes you from nothing to your first answers about a schedule, one step at a time.

## What TotalFloat does with your schedule

TotalFloat opens a Primavera P6 `.xer` file and calculates its dates, float and critical path with its own engine and its own [written rules](../rules/README.md). It tells you why any date is what it is, checks the schedule's quality, and compares one update with the next. It only reads your file: anything it writes goes to a new file, and the original never changes. You can use it by asking an AI agent, in a browser window, or by typing commands.

## Install

You type the install command into a terminal: a window where you type commands and the computer types its answers back. On Windows the terminal is called PowerShell.

**On Windows,** open Start, type `PowerShell`, and choose **Windows PowerShell**. Paste this line into the window and press Enter:

```powershell
irm https://raw.githubusercontent.com/ijd5004/totalfloat/main/install/install.ps1 | iex
```

**On Linux,** open a terminal, paste this line and press Enter:

```sh
curl -fsSL https://raw.githubusercontent.com/ijd5004/totalfloat/main/install/install.sh | sh
```

The command downloads the latest TotalFloat, checks that the download is intact, and installs it. On Windows it also adds TotalFloat's folder to your `Path`, the list of folders the terminal looks in for programs, so typing `totalfloat` works from any folder. On Linux it installs into `~/.local/bin`. If that folder isn't on your `PATH` yet, it prints a line that starts with `export PATH=`: add that line to the end of `~/.bashrc`, the file your terminal reads when it opens.

To check it worked, open a new terminal window and type:

```sh
totalfloat --version
```

It answers with a line such as `totalfloat 0.1.0`. To upgrade later, run the install command again. On a Mac, there's no ready-made download yet: see [Build from source](install.md#build-from-source). [Install](install.md) has the other ways to install, and how to uninstall.

## Three ways to use it

Pick the one that suits you. They all give the same dates, because they all use the same engine.

### With your AI agent

An AI agent such as Claude Desktop or Claude Code can call TotalFloat for you. You ask questions in plain words; the agent asks TotalFloat, and TotalFloat calculates the answers. The agent never works out dates itself.

**Claude Desktop:** open **Settings**, choose **Developer**, then **Edit Config**. That opens a file named `claude_desktop_config.json`. On Windows it's `%APPDATA%\Claude\claude_desktop_config.json`, and on a Mac `~/Library/Application Support/Claude/claude_desktop_config.json`. Put this in the file and save it. If the file already has an `mcpServers` section, add the `totalfloat` entry inside it.

```json
{
  "mcpServers": {
    "totalfloat": {
      "command": "totalfloat",
      "args": ["mcp"]
    }
  }
}
```

Then quit Claude Desktop completely and open it again. On Windows, closing its window can leave it running, so quit it from its icon at the bottom right of the taskbar.

On Windows, if Claude Desktop says it can't start `totalfloat`, give the program's full path instead. The install command puts it in `C:\Users\YOUR-NAME\AppData\Local\Programs\totalfloat\totalfloat.exe`, where `YOUR-NAME` is the folder named after you in `C:\Users`; in PowerShell, `(Get-Command totalfloat).Source` prints the exact path. In this file each backslash is written twice:

```json
"command": "C:\\Users\\YOUR-NAME\\AppData\\Local\\Programs\\totalfloat\\totalfloat.exe"
```

**Claude Code:** run this once in a terminal:

```sh
claude mcp add totalfloat -- totalfloat mcp
```

Then ask about a schedule. Give the file's full path, such as `C:\Schedules\plant.xer` on Windows or `/home/you/schedules/plant.xer` on Linux. For example:

- "Open C:\Schedules\plant.xer and tell me what's in it: the projects, their data dates and how many activities each has."
- "Schedule it and tell me the project finish and which activities are critical."
- "Why does activity A2 finish when it does? Walk me back through the logic."
- "Run the schedule quality checks and list the three problems most worth fixing."
- "Compare C:\Schedules\update-1.xer with C:\Schedules\update-2.xer. What moved, and what's worth a look?"

[Using TotalFloat from an agent](../agents.md) lists everything an agent can ask TotalFloat to do, and other agents it works with.

### In the local app

The local app shows a schedule in your web browser, with a table of activities and a Gantt chart. It runs on your own computer, and nothing leaves it.

In a terminal, go to the folder that holds your schedule with `cd`, such as `cd Downloads`, then run:

```sh
totalfloat serve plant.xer
```

It calculates the schedule, then prints a link that starts with `http://127.0.0.1:`. Copy the link into your browser's address bar, or start the app with `totalfloat serve plant.xer --open` to have it open your browser for you. The link signs that browser in, so keep it to yourself. A `.xer` file opens read-only; [Open the local app](serve.md) shows how to start a history so you can make changes. When you're done, go back to the terminal and press Ctrl-C to stop the app.

### From the command line

You can also type commands and read the answers in the terminal. The examples use a file named `plant.xer`: use your own file's name instead. Go to the folder that holds it with `cd` first.

See what's in the file:

```sh
totalfloat inspect plant.xer
```

Calculate its dates, float and critical path. The answer starts like this:

```sh
totalfloat schedule plant.xer
```

```text
Project rel-fs-positive (1)
  data date: 2026-01-12 08:00
  finish: 2026-01-13 17:00 (was none)
  activities: 2, critical: 2, changed: 2, on dates or float: 2
```

Find out why an activity finishes when it does, using its Activity ID:

```sh
totalfloat explain plant.xer A2 --field early_finish
```

```text
A2 early finish 2026-01-13 17:00: its early start and its duration
A2 early start 2026-01-13 09:00: FS with 1h lag on the predecessor calendar from A1 early finish
A1 early finish 2026-01-12 17:00: its early start and its duration
A1 early start 2026-01-12 08:00: data date
```

Read it from the top: the finish comes from the start, the start from a link to A1, and so on back to the data date.

## Where to go next

- [Limitations](limitations.md): read this before you rely on TotalFloat's dates.
- [The user guide](README.md): every command, with examples, such as [checking schedule quality](check.md) and [comparing two updates](diff.md).
- [Using TotalFloat from an agent](../agents.md): everything your agent can ask for.
- [What TotalFloat supports](supported.md).
