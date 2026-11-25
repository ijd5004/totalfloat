# TotalFloat user guide

TotalFloat works with Primavera P6 `.xer` files from the command line or through an AI agent. It schedules a file, tells you why dates are what they are, and never changes your original.

It doesn't need P6 installed. It's not P6 either, so read [Limitations](limitations.md) before you rely on its dates.

1. [Install](install.md)
2. [See what's in a file](inspect.md)
3. [Schedule a file](schedule.md)
4. [Change a schedule](edit.md)
5. [Collect timesheets](timesheets.md)
6. [Start a new schedule](new.md)
7. [Move schedules to and from Microsoft Project](import.md)
8. [Find activities](query.md)
9. [Find out why a date is what it is](explain.md)
10. [Check schedule quality](check.md)
11. [Write a report](report.md)
12. [Compare two updates](diff.md)
13. [Keep projects in an EPS](portfolio.md)
14. [Run a schedule risk analysis](risk.md)
15. [Keep a schedule's history](history.md)
16. [Keep schedules in git](git.md)
17. [Schedules as text](text.md)
18. [Work with an agent](agent-recipes.md)
19. [Open the local app](serve.md)
20. [Limitations](limitations.md)
21. [What TotalFloat supports](supported.md)

Every command in this guide is run by a test, so the examples can't drift from what the program does. The examples use a small two-activity schedule, `plant.xer`.

## Things that work the same everywhere

- **Your file is safe.** Commands only read it. Anything written goes to a new file, and an existing file is never replaced.
- **`--json`** gives output for scripts and agents.
- **Exit codes:** `0` is fine, `1` is an error, `2` is a mistyped command, and `3` means it finished and found something to look at.
- **Errors** say what went wrong and what to try next.
