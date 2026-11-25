# Compare two updates

`diff` shows what changed between two versions of a schedule. Give the earlier file first.

```sh
totalfloat diff update-1.xer update-2.xer
```

```text
Project rel-fs-negative: finish none
  activities: 0 added, 0 deleted, 2 changed
  relationships: 0 added, 0 deleted, 1 changed lag
  critical: 0 joined, 0 left; milestones moved: 0; flagged: 0
```

It compares activities, durations, progress, constraints, calendars, codes and relationships, along with milestone and finish movement and who joined or left the critical path. Both files are read as stored, so schedule them first if you need fresh dates.

Activities match by Activity ID, then by internal ID, so a renumbered activity shows as a change.

Some changes are flagged for a look, and `diff` then exits `3`:

- a critical activity got shorter by more than progress and the time between updates can explain
- logic was removed from critical work
- an activity gained a constraint
- critical work moved to another calendar
- an actual date already recorded reads differently
- a project is only in the earlier file
- a file lost a project's rows when it was read, so that project wasn't compared

For the full list of changes, use `--markdown` for a report or `--json` for scripts:

```sh
totalfloat diff update-1.xer update-2.xer --markdown
```

```sh
totalfloat diff update-1.xer update-2.xer --json
```

## Measure against a baseline

`variance` measures an update against a baseline, such as the approved plan or last month's update, and gives the facts a monthly narrative needs:

```sh
totalfloat variance update-2.xer --baseline update-1.xer
```

For each project it gives:

- **The finish:** the project finish and its variance.
- **Milestones:** each milestone and its variance, latest first.
- **Slips and float:** the largest finish slips and the largest losses of total float. `--top` says how many, and the default is 10.
- **The critical path:** what joined it and what left it.
- **Counts:** activities added and deleted, and how many relationships, durations and constraints changed.

A variance is the baseline less the current date, in working hours on the activity's own calendar, or the project's calendar for its finish, so a negative variance is late. Activities match by Activity ID, and projects pair as `diff` pairs them. `--json` gives schema `totalfloat.variance/1` ([docs/schemas/variance-1.schema.json](../schemas/variance-1.schema.json)), and MCP has the same as `variance`.

For a report to keep or send on, `--markdown` prints the same facts as Markdown tables, and `--html` writes them as one HTML page, with late variances marked in red:

```sh
totalfloat variance update-2.xer --baseline update-1.xer --markdown
```

```sh
totalfloat variance update-2.xer --baseline update-1.xer --html -o variance.html
```

Each project's section starts with its code and name, its data date, and the baseline with its data date. `-o FILE` writes a new file and refuses one that already exists. The page has its style inline and nothing else: no scripts, no links to other files and no time of writing, so the same files always give the same page.
