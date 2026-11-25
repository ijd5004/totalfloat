# Find activities

`query` lists activities that match what you ask for. It reads the results stored in the file, so schedule it first if you want fresh dates:

```sh
totalfloat schedule plant.xer -o plant-scheduled.xer
```

```sh
totalfloat query plant-scheduled.xer --critical
```

```text
code  name                  status       early_start       early_finish      total_float_hours  critical
A1    Synthetic activity 1  not-started  2026-01-12 08:00  2026-01-12 17:00  0.0                true
A2    Synthetic activity 2  not-started  2026-01-13 09:00  2026-01-13 17:00  0.0                true
Showing 1-2 of 2
```

## Filters

Filters combine, so an activity has to pass all of them.

| Filter | Finds |
|---|---|
| `--project 1` | One project |
| `--wbs CODE` | A WBS node and everything below it: its code, which takes every node with that code, or its path as the `wbs_path` column shows it, such as `PLANT.2`, which takes that node alone |
| `--code "Phase=DESIGN"` | Activities holding an activity code value. `inspect` lists the file's codes |
| `--resource CREW-1` | Activities assigned a resource, by short name or name |
| `--status not-started` | `not-started`, `in-progress` or `completed` |
| `--type task` | `task`, `resource`, `loe`, `start-milestone`, `finish-milestone` or `wbs-summary` |
| `--float-min 0 --float-max 40` | Total float in hours |
| `--critical` | Critical by the project's own definition |
| `--constrained` | Activities with a primary or secondary constraint |
| `--start-from`, `--start-to` | Early start range |
| `--finish-from`, `--finish-to` | Early finish range |
| `--text pump` | Text in the code or name, ignoring case |

Finished work stores the data date as its early dates, so date filters leave it out. Add `--status completed` to include it, and it's then matched on its actual dates.

```sh
totalfloat query plant-scheduled.xer --status not-started --type task --float-max 40
```

```sh
totalfloat query plant-scheduled.xer --start-from "2026-01-13 00:00" --text activity
```

### Any of several groups

`--any` gives a group of conditions, and an activity has to meet every condition of at least one group, besides the filters above. Each `--any` is one group: the filters' names as in the table but `--project`, without the dashes and with `_` for `-`, as `NAME=VALUE` separated by `;`. A switch is `critical=true`, and a list such as `status` takes commas. `\;` is a semicolon within a value.

```sh
# Not started, and either critical or with no more than 8 hours of float
totalfloat query plant.xer --status not-started --any critical=true --any float_max=8

# Critical work with "pour" in its ID or name, or anything not started
totalfloat query plant.xer --any "text=pour;critical=true" --any status=not-started
```

Over MCP, `any` is a list of groups, each an object of the same filters with their usual types, such as `[{"critical": true}, {"float_max": 8}]`.

## Follow the logic

`--preds` and `--succs` list what's linked to an activity. Use one at a time.

```sh
totalfloat query plant-scheduled.xer --preds A2 --columns code,link_type,link_lag_hours
```

```text
code  link_type  link_lag_hours
A1    FS         1.0
Showing 1-1 of 1
```

`--driving-path-to` traces everything that drives an activity, nearest first. It runs the engine, so it needs a schedule that can be calculated.

```sh
totalfloat query plant-scheduled.xer --driving-path-to A2
```

## Choose what comes back

`--columns` picks columns, or use `all` to see every one. `resources` lists each activity's resources and `roles` the roles its assignments are for, including those with no resource yet. `obs` names the OBS node responsible for the activity's WBS node. `wbs` is the activity's WBS node's code, and `wbs_path` its codes from the top of the tree joined with `.`, which tells apart nodes that share a code. `udf:NAME` gives an activity's value of a user-defined field, by label (any case) or `udf_type_id`, as text, a number or a date. `code:TYPE` gives the value it holds of one activity code type, by the type's name (any case), where `codes` lists them all. `steps` lists the activity's steps with their percent complete, and `steps_percent` gives their percents weighted by step weight, which is the physical percent where the project measures by steps ([TF-042](../rules/TF-042-activity-steps.md)). `notes` gives the activity's notebook notes as plain text, each as `TOPIC: TEXT` in the topics' order. In JSON a line break is `\n` and a blank line separates notes; the table shows them as ` / ` and ` | `. Results come 50 at a time: `--limit` and `--offset` page through them, and the last line always gives the total.

```sh
totalfloat query plant-scheduled.xer --columns all --limit 1 --offset 1
```

## Progress, units and costs

These columns show progress:

- **`percent_complete`** uses the activity's `percent_type`: `duration`, `physical` or `units`.
- **`duration_percent`** is original less remaining duration, over original. Finished work is 100, and work with no duration is 0 until it finishes.
- **`physical_percent`** is as entered.
- **`units_percent`** is actual over actual plus remaining units.

`budgeted_units`, `actual_units` and `remaining_units` add up the activity's labor and nonlabor hours. Material is in its own units, so it isn't added in. `budgeted_cost`, `actual_cost`, `remaining_cost` and `at_completion_cost` add up all its assignments and its expenses. `budgeted_expense_cost`, `actual_expense_cost`, `remaining_expense_cost` and `at_completion_expense_cost` give the expenses alone, and `expenses` lists their items. With no assignments or expenses these columns are empty, not zero, and so is a units percent with no units to measure.

```sh
totalfloat query plant-scheduled.xer --columns code,percent_type,percent_complete,budgeted_units,at_completion_cost
```

## Resource usage

`--usage` gives a resource's units per period instead of activities: actual and remaining, their total, and the resource's limit. `--by week` (the default; weeks start on Monday) or `--by month`. Units are hours for labor and nonlabor, and the resource's own units for material.

This is the synthetic `fixtures/xer/usage.xer`, a crew limited to 1 unit an hour:

```text
$ totalfloat query usage.xer --usage CREW --by week
CREW Synthetic labor by week, in hours
start           actual   remaining       total       limit
2025-12-29          16           0          16          40
2026-01-05           0          24          24          40
2026-01-12           0          60          60          40  over
2026-01-19           0          60          60          40  over
Total 16 actual, 144 remaining over 2 assignments
```

- **Remaining** units are spread over each activity's remaining dates as stored, so schedule the file first. Each assignment's curve or typed spread shapes them, on the calendar its work runs on ([TF-039](../rules/TF-039-resource-curves.md), [TF-040](../rules/TF-040-typed-spreads.md)).
- **Actual** units are spread over the activity's actual dates, from its actual start to its actual finish, or to the data date for work under way.
- **The limit** is the resource's maximum units per hour, in the price in force, over its calendar's work hours in the period; a price taking effect within a period limits the rest of it. A period whose total is above it is marked `over`. The activity filters don't apply to `--usage`.

An assignment with units but no dates to spread them over is listed under `not_spread`, with why. In JSON, `limits` gives the limit over time in units per hour: the first entry, with `from` null, is the limit in force before any change, and each later one starts on its `from`. `--json` gives `totalfloat.usage/1` ([schema](../schemas/usage-1.schema.json)), as does MCP `usage` and `GET /api/1/usage`.

## Export to a spreadsheet

`--csv` prints every matching activity as CSV, with the columns asked for: one header row of column names, then a row per activity. `--xlsx FILE` writes the same rows as an Excel workbook in a new file, one sheet with a header row, numbers as numbers. Both export every match, so `--limit` and `--offset` can't be given with them. Text that a spreadsheet would run as a formula (starting with `=`, `+`, `-` or `@`) gets a leading `'` in CSV; in a workbook it's always stored as text. Edits made in the exported sheet can be read back with `edit --from-sheet` ([Edit in a spreadsheet](edit.md#edit-in-a-spreadsheet)); the `'` is taken off again.

```sh
totalfloat query plant-scheduled.xer --columns code,name,early_start,early_finish,total_float_hours --csv
totalfloat query plant-scheduled.xer --columns all --xlsx activities.xlsx
```

## Non-ASCII text in PowerShell

TotalFloat writes its output as UTF-8. PowerShell on Windows decodes a program's output in the console's output encoding when it captures it. On most Windows installs that's a legacy code page such as 437, and letters beyond ASCII then arrive garbled, "Prüfung" as "Pr├╝fung", wherever the output goes into a variable or through a pipe to a command such as `Select-Object`, `ConvertFrom-Json` or `ConvertFrom-Csv`. This happens in Windows PowerShell 5.1 and in PowerShell 7. Windows PowerShell 5.1 garbles a `>` redirect too, and saves it as UTF-16. Text printed straight to the console, output in cmd, and files written with `-o` or `--xlsx` are unaffected.

Set PowerShell to read UTF-8 before you capture output, in the session or in your profile (`notepad $PROFILE`):

```powershell
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
```

## Costs by cost account

`cost_accounts` lists the cost accounts an activity's assignment and expense costs are booked to, each by its path: short names from the top of the tree down, joined by dots, which `--set-cost-account` takes back. `--by-cost-account` adds up the matching activities' costs by account: the last lines, or `cost_accounts` in JSON, give each account's budgeted, actual, remaining and at-completion cost, with costs booked to no account last. An account's line holds only the costs booked to it, and the accounts under it have their own lines; `parent` in JSON gives the path of the account above, so you can roll them up.

```sh
totalfloat query plant-scheduled.xer --status not-started --by-cost-account --limit 0
```

## Earned value

These columns give earned value from the activity's assignment and expense costs, progress and plan:

- **`budget_at_completion`**
- **`planned_value`**
- **`earned_value`**
- **`estimate_to_complete`** and **`estimate_at_completion`**
- **`schedule_variance`** and **`cost_variance`**
- **`schedule_performance_index`** and **`cost_performance_index`**

`actual_cost` is the progress column. Each WBS node's earned value settings, as the file stores them, say how value is earned and how the estimate to complete is made. A node that estimates by a user performance factor stores the factor but not which way it reads: by default the budget left is divided by it, as public estimate-at-completion formulas divide by an efficiency index, and `--user-factor multiply` multiplies the budget left by it instead.

Planned value comes from the baseline's planned dates and budget with `--baseline`, and otherwise from the activity's own planned dates. When you ask for any of these columns, the last line, or `earned_value_total` in JSON, gives the totals over every matching activity. With `--wbs`, that's the node and everything under it.

```sh
totalfloat query plant-scheduled.xer --wbs rel-fs-positive --columns code,budget_at_completion,earned_value,cost_performance_index
```

An XER file stores no cumulative earned value results. These measures follow TotalFloat's rule, [TF-041](../rules/TF-041-earned-value.md), from the earned value definitions in public guidance.

## Compare with a baseline

`--baseline` reads another XER of the same project, such as last month's or the approved plan, and matches activities by Activity ID. Use `--baseline-project` if that file holds several projects.

```sh
totalfloat edit plant-scheduled.xer --duration A1=16h -o plant-later.xer
totalfloat query plant-later.xer --baseline plant-scheduled.xer
```

The default columns gain `baseline_start`, `baseline_finish` and `finish_variance_hours`. `baseline_duration_hours`, `start_variance_hours` and `duration_variance_hours` are there too:

- Each side's start and finish is its actual date, or its early date if it hasn't happened.
- A variance is the baseline less the current value, in working hours on the activity's calendar, so a negative variance is late. A slip of one working week shows as `-40`, whatever falls between.
- Durations are at completion: the actual working time so far plus the remaining duration. So `baseline_duration_hours` is the baseline's, and duration variance is the baseline's less the current one. A file compared with itself shows no variance.

A file with several projects pairs each with the baseline's project of the same short name. An activity only in this file has empty baseline columns, and a baseline that lost rows when it was read is refused, since a lost row would look like a deletion. The last line counts the activities matched, those added since the baseline and those deleted from it. Over MCP, `baseline` is a file path.
