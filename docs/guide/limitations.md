# Limitations

TotalFloat is an early release. Read this before you rely on it.

## How far to trust its dates

TotalFloat calculates schedules with its own engine, following the settings each file holds. Its results can differ from the dates another scheduler stored in the same file. Treat them as a strong starting point, and confirm dates in your scheduling tool of record before making commitments on them. `totalfloat schedule` lists where its dates differ from the ones stored in your file, so you can check your own schedules.

## What it doesn't do

- **Resource leveling.** Leveling isn't there, so a leveled schedule's dates will differ. `schedule` shows this as differences that start at activities waiting on a shared resource, with no logic behind them. Resources can have limits, and `query --usage` and the app's Usage panel mark periods over them, but nothing moves work to meet them.
- **Resource-dependent work on several resource calendars.** An activity whose resources use different calendars is refused.
- **The Actual Dates out-of-sequence mode.** Retained Logic and Progress Override work; a project set to Actual Dates is refused.
- **Calendars that declare no working week.** Work on them isn't calculated.
- **Level of effort and WBS summary** are calculated as spans, and how some links on them behave is a first version.
- **Baselines.** An XER export doesn't hold them, so baseline checks say `n/a`.
- **Multiple float paths** follow TotalFloat's own [rule](../rules/TF-002-multiple-float-paths.md), so their numbers can differ from P6's.
- **MS Project XML** export writes costs in US dollars, whatever the schedule's currency. A material resource's work isn't carried in either direction, and links and assignments on summary tasks are left out with a note.
- **The browser app** is a preview. It adds activities, WBS nodes and links, but can't delete a WBS node or edit an activity's codes, notes or resources yet, and editing gets slow on schedules of tens of thousands of activities.

[What TotalFloat supports](supported.md) lists everything it does.

## What it refuses

Rather than guess, TotalFloat refuses a project when it lost rows reading the file, or when it needs something on the list above. The error says why. Other projects in the file can usually be scheduled alone.

## Not yet tested

- Files TotalFloat writes pass XER Tools' file-integrity checks ([record](../validation/xertools-2.2b.md)). Importing them into other tools hasn't been verified, so check an import before relying on it.

Please [open an issue](https://github.com/ijd5004/totalfloat/issues) if a result looks wrong.
