# Schedules as text

A schedule can be written as a text file, `plan.tfs`: one activity per line, with its predecessors on the same line. People read it top to bottom, agents write it with ordinary file tools, and a change to one activity is a change to one line in a diff. The format is set out in [ARCHITECTURE.md, Schedule as text](../ARCHITECTURE.md#schedule-as-text).

```text
totalfloat-schedule 1

project PLANT "New Plant"
  data-date 2026-03-02 08:00

wbs PLANT.CIV "Civil works"

activity A1010 "Excavate" 5d
activity A1020 "Pour foundations" 10d after A1010
activity A1030 "Cure" 7d after A1020 ss+2d
milestone M100 "Civil complete" after A1030
  finish-on-or-before 2026-06-30 17:00
```

The file never holds calculated dates or float. Pin a date with a constraint.

## Format a file

`fmt` rewrites a file in the canonical layout: one space between words, two spaces before each detail, and a blank line before each project, calendar, code type and WBS line, before a run of resources, and before activities that follow them. Comments (`#` to the end of a line) are kept.

```sh
totalfloat fmt plan.tfs
totalfloat fmt plan.tfs --check
```

```text
Formatted plan.tfs
Every file is formatted
```

With `--check`, nothing changes, and the command exits 3 if any file would, as other commands do for findings. A file with mistakes is left as it is, and the others are still formatted.

Names go in double quotes. Inside one, write `\"` for a quote, `\\` for a backslash and `\n` for a line break.

## Mistakes

Every mistake in a file is reported in one pass, with the line and column, the text underlined, and a fix:

```text
error[unknown-keyword]: `activty` isn't a statement
 --> plan.tfs:5:1
  |
5 | activty A1010 "Excavate" 5d
  | ^^^^^^^
  = hint: did you mean `activity`?
```

## Build a schedule from text

`import` builds a new schedule from a text file, the same way [`new --edits`](new.md) does from a JSON list. Nothing is built unless the whole file is right.

```sh
totalfloat import plan.tfs -o built.xer
```

```text
WBS CIV added
A1010 added, 40h long, with no links yet
A1020 added, 80h long, with no links yet
A1010 to A1020 FS added, lag 0h
Project PLANT "New Plant": data date 2026-03-02 08:00, 2 activities, 1 relationships
finish: 2026-03-20 17:00
Wrote built.xer
```

How the file becomes a schedule:

- **The project** line gives the code and name, and its `data-date` is required. A WBS path starts with the project code, so `PLANT.CIV` is the node `CIV` under the project. Activities belong to the `wbs` line above them, or to the project's own node.
- **The project's calendar** is the one the schedule starts with, `Standard 5-Day` unless the project names another and the file defines it. A calendar with `week` lines works only the days they list. One with `copy-of` keeps the days its lines don't mention.
- **A day** in a span is the calendar's hours per day. An `hours-per-day` line sets them, such as `hours-per-day 10`. Without one, a calendar with no `copy-of` whose working days are all the same length counts a day as that length, and any other calendar keeps the hours per day it was copied with when its week changes ([TF-033](../rules/TF-033-calendar-hours-and-duration-units.md)): a copy of `Standard 5-Day` given ten-hour days still counts `5d` as 40 hours, and the build's report says so.
- **Every problem is reported in one run**, each at its line: names that aren't in the file, with a suggestion, and every edit the file's commands would make that's refused, such as a constraint without a date or a link that closes a loop.
- **Roles** are defined before the activities that use them too: a `role` line, with a `parent` line for a role under another defined above it, and a `max-units-per-hour` line for its limit, as for a resource; `portfolio capacity` measures the role against it. An activity's `role` line staffs it by role with no resource yet, with `units` in hours if given.
- **An activity's `expense` lines** add its expenses: `expense "Building permit" 1500` is one unit at that budgeted cost, with nothing spent. `actual` and `remaining` give what's spent and what's left; the remaining cost is the rest of the budget unless given. A negative cost is a credit. `accrual start` or `accrual end` puts the cost at the activity's start or finish, rather than spread over it.
- **Code types and resources** are defined before the activities that use them. A `code-type` line and its `value` lines add the type, for this project, and its values. A `resource` line adds a resource, labor unless the line says `nonlabor` or `material`, with `calendar` and `units-per-hour` lines if it doesn't take the defaults: the project's calendar, and 1 unit an hour. A `max-units-per-hour` line gives its limit, the most units an hour it can work, so `2` is two people at once; the Usage panel and `query --usage` measure each period against it. A resource can only use a global calendar, so in a schedule built from text, that's the project's calendar.
- **Fields** are user-defined fields for activities: a `field` line gives the name and type (`text`, `number`, `integer`, `cost`, `start-date`, `finish-date` or `indicator`), and an activity's `udf NAME=VALUE` or `udf "Name" VALUE` line gives its value, checked against the type. An indicator is `UDF_G0` to `UDF_G4`, a date `"2026-03-09 17:00"`. Fields of code-list types, and values on other records, aren't written; they're listed as notes.

```text
code-type Phase
  value CON "Construction"

resource ELEC "Electricians" labor
  max-units-per-hour 2
resource CRANE "Tower crane" nonlabor
  units-per-hour 0.5

role SUPER "Superintendent"
role FOREMAN "Site foreman"
  parent SUPER

activity A1010 "Pull cable" 5d
  code Phase=CON
  resource ELEC
  role FOREMAN units 16
```

Over MCP, `new` takes the same text as `text`.

## Write a schedule out as text

`export --to tfs` writes one project of a schedule as text: the project, the calendars it, its activities and its resources use, its code types and resources, then each WBS node in WBS order with its activities by ID. The same file always gives the same text, so two exports diff cleanly. A file with several projects needs `--project`.

```sh
totalfloat export plant.xer --to tfs -o plant.tfs
```

```text
Wrote plant.tfs
```

Spans are written in whole or half days of their calendar's day, otherwise in hours, so they mean the same whichever way the text is used. A calendar whose hours per day differ from its working day gets an `hours-per-day` line. Calculated dates and float are never written. What the format can't hold is listed as notes, as for MS Project, with `--json` giving each kind and the first few activities. That includes user-defined fields, costs, resources' prices, limits that change over time and hierarchy (a limit that is the same on every rate is written as `max-units-per-hour`), resources with no calendar, roles' prices and limits that change over time (a limit that is the same on every rate is written as `max-units-per-hour`), the roles resources fill and one parent in a loop of parent roles, expenses' cost accounts, categories, units, prices and vendors, suspend and resume dates, duration and percent complete types, level of effort and WBS summary activities (written as tasks), and scheduling options that differ from the defaults. A schedule built back from the text uses the defaults.

## Apply a text file to a schedule

Write a schedule out, edit the text, and apply it: `edit --apply` works out what changed and turns it into ordinary edits. Everything else about `edit` works the same. The batch is all or nothing unless you pass `--skip-failed`. `--dry-run` shows the impact without writing. On a history store the change is recorded with `-m` and its author, and `--propose` leaves it for review.

```sh
totalfloat edit plant.xer --apply plant.tfs --dry-run
```

```text
plant.tfs matches the schedule; nothing to change
```

Change a duration in the text, add a predecessor, and apply it again. `--dry-run` lists each edit and what it did to the dates. `--emit-commands` saves the edits as JSON.

How the two are compared:

- **Activities** match by ID, **WBS nodes** by path (by code, if one has moved), **calendars** and **code types** by name, **resources** and **roles** by short name, and **links** by their two activities.
- **Spans** are compared in minutes on the activity's calendar. `5d` and `40h` on an 8-hour calendar are the same. A changed `hours-per-day` line sets the calendar's hours per day.
- **What the text leaves out:** an activity, link or WBS node the schedule has but the text doesn't is refused and listed, until you choose `--delete-missing` or `--keep-missing`. A partial file is safe to apply, and nothing is deleted by accident. WBS nodes can't be deleted yet. If a deleted and an added activity look the same apart from their ID, you're warned: there's no command to change an activity's ID.
- **A new data date** in the text moves the data date, as `--data-date` does. The project's code, name, must-finish date and calendar can't be changed this way yet.
- **New code types, resources and roles** are added, and an activity's `role` lines are assigned or taken off as its `resource` lines are. **Expenses** match by item on their activity: a new line adds one, a changed line sets its costs and accrual, and a line taken out deletes it. Its cost account, category and other details stay as they are. A resource's or role's `max-units-per-hour` line sets its limit on every rate; with no such line, its limit stays as it is. Otherwise a resource or role the schedule has can't be changed or deleted this way yet; a line that changes one is refused.
- Mistakes in the text, and edits that are refused, come back at their lines.

Over MCP, `apply_text` takes the text on an open handle, with `missing` set to `refuse`, `keep` or `delete`.
