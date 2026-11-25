# Change a schedule

`edit` makes changes, recalculates, and tells you what the changes did. Your file stays as it was.

```sh
totalfloat edit plant.xer --duration A1=15d
```

```text
A1 duration: original 8h to 120h, remaining 8h to 120h
Project rel-fs-positive (1)
  finish: 2026-02-02 17:00 (was 2026-01-13 17:00)
  critical: 2 (was 2)
Nothing written; pass -o to write a new file
```

Without `-o` nothing is saved, so every run is a what-if. `--dry-run` says so outright. On a [history store](history.md), edits are recorded there instead, with who, why and when. Add `-o` to write a new file:

```sh
totalfloat edit plant.xer --duration A1=15d -o plant-longer.xer
```

The report compares against your file calculated as it stands, so old results in the file don't count as the edit's effect. If a critical date moves, it says which milestones moved, what became critical or stopped being critical, and anything `diff` would flag.

Every edit applies or none do. A refused edit says why and what to try, and every refused edit in the run is listed at once, each with the flag it came from (or `edits.json item 3`, or a status table's `line 4`), so one correction fixes them all. An edit that needed one refused earlier says so, rather than giving a second, unrelated reason. When a name is close to one the file holds, the hint asks `did you mean A1010?`. With `--json`, the error document (`totalfloat.error/1`, in `docs/schemas/`) lists them under `problems`; one refusal keeps its own code, and several have the code `edits-refused`. If the engine refuses the file as it stands, an edit that repairs it still runs, and a warning says the comparison is with the file's stored dates.

## Apply what can be applied

For bulk input whose parts don't depend on each other, such as a 500-row status table with three typos, `--skip-failed` applies every edit that can be applied and skips the rest, together with every edit that needed a skipped one. The skipped edits are listed first, each with its reason, then the impact of what did apply. The command exits `3` when anything was skipped, so a script notices. With `--json` they are under `skipped`; over MCP, set `skip_failed`. Without `-o` it still writes nothing.

## Check before you save

`--check` compares the quality checks from [Check schedule quality](check.md) before and after the edits. It says which newly fail, which gained offenders and which now pass. If one newly fails, nothing is written and the command exits `3`. Leave out `--check` to save anyway.

```sh
totalfloat edit plant.xer --duration A1=15d --check -o plant-checked.xer
```

## What you can change

| Flag | Changes |
|---|---|
| `--duration A1=15d` | Original duration. On work not started, remaining follows it |
| `--remaining A1=5d` | Remaining duration |
| `--constraint "A1=start-on-or-after@2026-03-02 08:00"` | Primary constraint |
| `--secondary-constraint "A1=finish-on-or-before@2026-04-01 17:00"` | Secondary constraint |
| `--clear-constraint A1`, `--clear-secondary-constraint A1` | Removes a constraint |
| `--calendar "A1=Synthetic weekdays"` | The activity's calendar |
| `--name "A1=Pour slab"` | The activity's name |
| `--link A1>A2`, `--link "A1>A2:SS+2d"` | Adds a relationship |
| `--unlink A1>A2`, `--unlink "A1>A2:SS"` | Removes a relationship |
| `--relink "A1>A2+2d"`, `--relink "A1>A2:FF-4h"` | Changes the only relationship between two activities |
| `--add "A3;Pour slab;ROOT;5d"` | Adds an activity, as code, name, WBS and duration, on the project's calendar. In a file with several projects, name one with `--project` |
| `--delete A2` | Deletes an activity, with its links and assignments, and says what that leaves open |
| `--add-wbs "CIV;Civil Works;ROOT"` | Adds a WBS node under a parent |
| `--rename-wbs "CIV=Site Civil"`, `--move-wbs "CIV=PHASE2"` | Renames a WBS node, or moves it under another |
| `--wbs "A1=CIV"` | Moves an activity to a WBS node |
| `--udf "A1=Risk=2"`, `--clear-udf "A1=Risk"` | Gives an activity a user-defined field value, or takes it off. The value is checked against the field's type: text on one line, a number, a whole number, a cost, a date as `YYYY-MM-DD HH:MM`, or an indicator, `0` to `4` or `UDF_G0` to `UDF_G4`. JSON `set_udf` and `clear_udf` take `wbs`, `resource`, or `activity` and `resource` for an assignment, in place of `activity`. Code-list fields can't be set yet |
| `--add-udf-type "Risk;indicator"`, `--add-udf-type "Owner;text;wbs"` | Adds a user-defined field, for activities unless `wbs`, `resource` or `assignment` follows. Types: `text`, `number`, `integer`, `cost`, `start-date`, `finish-date`, `indicator`. Fields belong to the whole file |
| `--add-step "A1=Excavate"`, `--add-step "A1=Pour;2"`, `--step "A1=Pour=50"`, `--complete-step "A1=Pour"`, `--delete-step "A1=Pour"` | Adds a step (weight 1 unless given), sets a step's percent complete, marks one complete or deletes one. Steps are found by name or `proc_id`. Where the project measures physical percent by steps and the activity's type is physical, its physical percent follows ([TF-042](../rules/TF-042-activity-steps.md)). JSON `set_step` also renames and reweights |
| `--add-note "A1=Basis of schedule=Two crews"`, `--set-note "A1=Risks=Late steel"`, `--delete-note "A1=Risks"` | Adds, replaces or deletes an activity's note under a notebook topic the file has for activities. The text becomes simple HTML, a paragraph a line. Notes on WBS nodes take JSON edits with `wbs` in place of `activity` |
| `--add-obs Contractor`, `--add-obs "Electrical;Contractor"` | Adds an OBS node, at the top or under another. The OBS belongs to the whole file |
| `--wbs-obs "CIV=Contractor"` | Makes an OBS node, by name or `obs_id`, a WBS node's responsible manager. A new WBS node takes the project root's |
| `--assign "A1=CREW-1"`, `--assign "A1=CREW-1:40"`, `--unassign "A1=CREW-1"` | Puts a resource on an activity, at its default units per hour or with the units given (hours for labor), or takes it off |
| `--code "A1=Phase=DESIGN"`, `--clear-code "A1=Phase"` | Gives an activity a code value, or takes it off |
| `--add-code-type Phase`, `--add-code-type "Area;global"` | Adds an activity code type, for this project or for every project |
| `--add-code-value "Phase=BUILD;Construction"` | Adds a value to a code type |
| `--project-code "Region=WEST"`, `--clear-project-code Region` | Gives the project a project code value, or takes it off. In a file with several projects, name one with `--project` |
| `--add-project-code-type Region`, `--add-project-code-value "Region=WEST;Western region"` | Adds a project code type, or a value to one. Project codes belong to the whole file, so every project in the file gets them |
| `--add-resource "ELEC;Electricians"`, `--add-resource "CRANE;Tower crane;nonlabor"` | Adds a resource, labor unless a type is given, on the default global calendar at 1 unit an hour. JSON edits can give `units_per_hour`, `calendar` and `max_units_per_hour`, its limit |
| `--resource-limit "ELEC=2"`, `--resource-limit "ELEC=3@2027-01-04"`, `--resource-limit "ELEC="` | Sets a resource's limit, the most units an hour it can work, so 2 is two people at once; the Usage panel and `query --usage` measure each period against it. Without a day, every rate the resource has takes the limit, and a resource with no rate gets one, with no prices, starting on the project's planned start (or its data date). With a day, a rate starts that day with the prices in force then, and later rates keep their own limits; a rate that already starts that day takes the limit instead. If no rate starts before that day, a rate from the project's start keeps what held before it, so the limit doesn't reach back. With no units it clears the limit. JSON: `set_resource_limit` with `resource`, `max_units_per_hour` (a number, or null to clear) and `from`. Before a resource's first rate starts, that rate is used; a rate a file holds with no start date is in force until the first dated rate starts |
| `--add-role "FOREMAN;Site foreman"`, `--add-role "FOREMAN;Site foreman;SUPER"` | Adds a role, under another role if one is given |
| `--role-limit "ENG=2"`, `--role-limit "ENG=3@2027-01-04"`, `--role-limit "ENG="` | Sets a role's limit, the most units an hour of it the work can use, as `--resource-limit` does for a resource; `portfolio capacity` measures the role against it. `add_role` takes `max_units_per_hour` in JSON. |
| `--assign-role "A1=FOREMAN"`, `--assign-role "A1=FOREMAN:16"`, `--unassign-role "A1=FOREMAN"` | Staffs an activity by role with no resource yet, with no units or the hours given, or takes that off. The role's first price in force when the work starts prices it, as a resource's does |
| `--curve "A1=ELEC;Bell Shaped"`, `--spread-evenly "A1=ELEC"` | Spreads a resource's units on an activity by one of the file's resource curves, ignoring case, or evenly over its work with none. Either clears units typed by hand, and is refused where actual units were typed by period |
| `--spread "A1=ELEC;day=2026-03-02;12,4,0,12,12"` | Types a resource's remaining units on an activity for each `day`, `week` or `month` (from the first of a month) in turn, from the date given, in place of a curve. The periods must cover the activity's remaining dates, as they stand before this command recalculates, and add up to the remaining units: the edit never changes units. A period with no work takes no units. The file holds the result as XER files hold it, in stretches of work time ([TF-007](../rules/TF-007-choosing-a-spread.md), [TF-040](../rules/TF-040-typed-spreads.md)) |
| `--add-expense "A1=Building permit;1500"`, `--add-expense "A1=Crane;2400;SITE;Fees;end"` | Adds an expense: one unit at its budgeted cost (negative for a credit), all of it still to spend, with a cost account, an expense category and an accrual (`start`, `end` or `uniform`, the default) if given. Leave one empty to skip it, as `A1=Crane;2400;;Fees` |
| `--set-expense "A1=Building permit;actual=500"` | Sets an expense's `budget`, `actual` or `remaining` cost, or its `accrual`. A new budget keeps the units and moves the price. Without a remaining cost, the cost at completion holds, so an actual cost comes out of what remains; while nothing was spent, a new budget is the new cost at completion |
| `--delete-expense "A1=Building permit"` | Removes an expense |
| `--set-cost-account "A1=resource:CREW;SITE"`, `"A1=role:RIGGER;SITE"`, `"A1=expense:Building permit;SITE.PERMIT"` | Books the costs of a resource's assignment, a role's assignment with no resource, or an expense to a cost account, by its path (short names from the top of the tree down, joined by dots, as `SITE.PERMIT`), or by short name or name where only one account has it. Short names can repeat under different parents, and the path tells them apart. Leave the account empty, as `"A1=expense:Building permit;"`, to book them to none. A resource on the activity more than once has each assignment booked |
| `--holiday "Synthetic weekdays=2026-02-02"`, `--no-holiday "..."` | Makes a date a holiday, or puts it back to the normal week |
| `--actual-start "A1=2026-02-02 08:00"` | Starts work. Actuals can't be after the data date |
| `--actual-finish "A1=2026-02-06 17:00"` | Finishes work, and remaining goes to zero |
| `--percent A1=50` | Physical percent complete. On started work measured by duration percent, with no `--remaining`, it sets remaining duration too: half done leaves half the original ([TF-023](../rules/TF-023-duration-and-physical-percent-complete.md)) |
| `--clear-progress A1` | Takes recorded progress away, so work is not started again |
| `--suspend "A1=2026-01-12 17:00/2026-01-19 08:00"` | Suspends work in progress: it stopped on the first date, and its remaining work restarts on the second, the resume date. The activity must be in progress, suspended on or after its actual start, and resumed after it was suspended |
| `--clear-suspend A1` | Takes an activity's suspend and resume dates away |
| `--data-date "2026-02-02 08:00"` | Moves the data date too |
| `--data-date "2026-02-02 08:00" --as-planned` | Moves the data date and records the progress the schedule planned before it |

Durations and lags are in hours, days, weeks, months or years, such as `120h`, `15d`, `2w`, `1m` or `1y`; `m` is a month, as P6 writes it. Each unit is the calendar's hours in it (P6's hours per day, week, month and year), so `2w` on a calendar of 40 hours a week is 80 hours. Durations use the activity's calendar, and lags use the calendar lag runs on; on 24 hours a day, a week is 168 hours and months and years have no fixed length. A calendar without hours for a unit refuses it, and resource-dependent work needs hours.

`--hours-per "Standard 5-Day=week=37.5"` sets a calendar's hours in a `day`, `week`, `month` or `year`, which changes what spans in that unit mean on it.

Quote anything holding `>`, or the shell treats it as a redirect.

The order is fixed, so flags don't depend on how you write them: edits from a file, then WBS nodes, code values, holidays, cleared progress, new activities, calendars, removed constraints, new constraints, durations, actuals, names, links, codes and deletions. A new activity can be linked in the same command, an expense is deleted before one is added and added before one is set, costs are booked to accounts after that, and a resource or role is unassigned before one is assigned, so `--unassign-role A1=SUPER --assign-role A1=SUPER:24` replaces the units. An assignment's curve and spread come after assignments, so a new one can be shaped in the same command. A duration in days uses the new calendar.

A change to a calendar reaches every project that uses it, and they all recalculate. Name a project with `--project` to limit that, and a warning says the others weren't recalculated.

For many changes, or for a type, an expected finish, a new milestone or a new activity on another calendar, put the edits in a JSON file. Each `op` is an edit:

```json
[
  {"op": "set_duration", "activity": "A1", "original": "15d"},
  {"op": "set_type", "activity": "A2", "activity_type": "finish-milestone"},
  {"op": "set_expected_finish", "activity": "A1", "date": "2026-03-02 17:00"},
  {"op": "add_link", "predecessor": "A1", "successor": "A2", "link_type": "SS", "lag": "2d"},
  {"op": "add_activity", "code": "A9", "name": "Turnover", "wbs": "ROOT", "activity_type": "finish-milestone", "calendar": "Synthetic weekdays"},
  {"op": "delete_activity", "activity": "A9"},
  {"op": "add_calendar", "name": "Winter weekdays", "copy_of": "Synthetic weekdays"},
  {"op": "add_calendar_exception", "calendar": "Synthetic weekdays", "date": "2026-02-07", "shifts": ["08:00-12:00"]},
  {"op": "set_work_week", "calendar": "Synthetic weekdays", "day": "saturday", "shifts": ["08:00-12:00"]},
  {"op": "set_hours_per_day", "calendar": "Winter weekdays", "hours": 7.5},
  {"op": "set_hours_per_period", "calendar": "Winter weekdays", "period": "week", "hours": 37.5}
]
```

```sh
totalfloat edit plant.xer --edits my-edits.json
```

That list can also be a versioned document, `totalfloat.command/1`, which holds the request's settings too. Its schema is `docs/schemas/command-1.schema.json`, and a document from a later version is refused by name rather than misread. `--emit-commands` saves the edits a run applied, with its settings, in this form, so a command line can be kept, reviewed and run again with `--edits`:

```sh
totalfloat edit plant.xer --duration A1=15d --data-date "2026-02-02 08:00" --emit-commands saved-edits.json
totalfloat edit plant.xer --edits saved-edits.json -o plant-replayed.xer
```

```json
{
  "schema": "totalfloat.command/1",
  "edits": [{"op": "set_duration", "activity": "A1", "original": "15d"}],
  "options": {"data_date": "2026-02-02 08:00"}
}
```

Settings in the document and on the command line combine; if both give a project or a data date, they must agree. Edits skipped with `--skip-failed` aren't saved, so running the document again can report fewer projects.

## Change many activities at once

`--where` and `--set` make a global change: one change to every activity a filter finds. The filter is `query`'s, written as `KEY=VALUE` pairs separated by `;`, with the names of `query`'s flags: `project`, `wbs`, `code`, `resource`, `status`, `type`, `float-min`, `float-max`, `critical`, `constrained`, `start-from`, `start-to`, `finish-from`, `finish-to` and `text`. `status` and `type` can come more than once, and `critical` and `constrained` take no value. It finds the same activities `query` does, in the file as it stands before any edit. A filter such as `wbs=CIV;code=Phase=CON` narrows it to a WBS node's construction work.

```sh
totalfloat edit plant.xer --where "type=task;status=not-started" --set "duration+=2d" --dry-run
```

| `--set` | Does, to each activity found |
|---|---|
| `duration=5d` | Sets the original duration. Milestones, level of effort and WBS summaries have none of their own, so they're skipped |
| `duration+=2d`, `duration-=1d` | Adds to or takes from the original duration, in days of each activity's own calendar |
| `calendar=NAME` | Moves it to a calendar |
| `constraint=KIND@DATE` | Sets the primary constraint, as `--constraint` does |
| `code=TYPE=VALUE` | Gives it a code value |
| `udf=FIELD=VALUE` | Gives it a user-defined field value |
| `prefix=TEXT`, `suffix=TEXT` | Puts text before or after its name |
| `duration=EXPRESSION`, `remaining_duration=EXPRESSION` | Sets the original or remaining duration to what an expression gives, such as `remaining_duration=original_duration*0.5`. Skipped where there's no duration of its own, as for `duration=5d` |
| `constraint_date=EXPRESSION` | Moves the primary constraint's date, keeping its kind, such as `constraint_date=early_start+5d` |
| `udf:FIELD=EXPRESSION` | Sets a user-defined field that holds a number, a cost, an integer or a date, such as `udf:Crew hours=original_duration/1h*1.5` |

Each change becomes an ordinary edit, one per activity, and they apply together or not at all, like any edits in one run. Several `--set` build on one another, so `duration=5d` then `duration+=1d` gives 6d. They come before the run's other edits, since `duration+=` and the name changes are worked out from the file as it stands, so a code value or calendar they name must be in the file already; `--apply` can't join them. `--dry-run` lists every one. The filter must find activities in one project; add `project=ID` if it finds them in several. Over MCP, `global_change` takes `filter` (`query`'s filters) and `set`, expressions included.

### Expressions

An expression is worked out for each activity from its own values. It takes numbers, spans such as `5d`, `4h` or `2w` (in the units of each activity's calendar, as `duration=5d` is), `+ - * /` and parentheses, and these fields:

| Field | Kind |
|---|---|
| `duration` or `original_duration`, `remaining_duration` | Duration |
| `total_float`, `free_float` | Duration, as the file stores them |
| `early_start`, `early_finish`, `late_start`, `late_finish` | Date, as the file stores them |
| `actual_start`, `actual_finish`, `planned_start`, `planned_finish` | Date |
| `constraint_date` | Date: the primary constraint's |
| `data_date` | Date: the project's |

Each expression has a kind: a number, a duration or a date. `duration/1h` is a number of hours. Durations add to and take from durations; a duration times or divided by a number is a duration, and a duration divided by a duration is a number. A date plus or minus a duration moves through the activity's calendar's working time, as scheduling does, so `early_start+5d` is five working days later, in working hours. A date less a date is the working time between them. Anything else, such as a date times a number, is refused with the field named before anything changes, and so is an expression that gives the wrong kind for its field: a duration for `duration` and `remaining_duration`, a date for `constraint_date`, and for a user field a number or a date, as its type says. A duration is rounded to the minute; a user field's number to 2 places, or to a whole number for an integer field.

`remaining_duration` and `constraint_date` skip finished work, which has no remaining duration and no use for a constraint. An activity without a value the expression reads, such as an actual start before work starts, or a `constraint_date` change on an activity with no dated primary constraint, stops the whole change, naming the activity. A negative duration does too. Narrow the filter, with `constrained` for example, to leave such activities out.

```sh
totalfloat edit plant.xer --where "type=task" --set "remaining_duration=original_duration*0.5" --dry-run
```

## Edit in a spreadsheet

Export activities with `query --xlsx` or `--csv`, change them in a spreadsheet, and read the changes back with `--from-sheet`. Each cell that differs from the schedule becomes an ordinary edit, so the saved file changes in those cells and nothing else, as the same `edit` run would.

```sh
totalfloat query plant.xer --columns code,name,original_duration_hours,remaining_duration_hours,calendar,constraint,constraint_date --xlsx activities.xlsx
```

Then, after changing cells in `activities.xlsx`:

```text
totalfloat edit plant.xer --from-sheet activities.xlsx -o plant-edited.xer
```

`--from-sheet` reads the `.xlsx` or `.csv` that `query` wrote, or a folder of CSV files (`activities.csv`, and if wanted `relationships.csv`). The Activities sheet is keyed by `code`, the Activity ID, and these columns become edits:

| Column | Edit |
|---|---|
| `name` | Renames it |
| `original_duration_hours`, `remaining_duration_hours` | Sets its durations, in hours |
| `calendar` | Moves it to a calendar, by name |
| `constraint`, `constraint_date` | Sets its primary constraint, or clears it when both are blank; `as-late-as-possible` drops the date it had |
| `code:TYPE` | Sets its value of an activity code type, or takes it off when blank |
| `udf:FIELD` | Sets its user-defined field value, or takes it off when blank |

Other columns `query` gives, such as dates, floats and costs, are worked out by the schedule, so they're left as they are. Spaces around a value and the kind of line break don't count as changes. Rows can be left out or put in any order; an Activity ID the schedule doesn't have is refused, since re-import doesn't add activities (`import` and `edit --add-activity` do).

A Relationships sheet, with `predecessor`, `successor`, and if wanted `link_type` (`FS`, `SS`, `FF` or `SF`) and `lag` (`2d`) or `link_lag_hours`, lists the links among the Activities sheet's activities: links it adds are added, a changed lag is changed, and links between those activities that it leaves out are removed. A blank or missing type or lag keeps the link's own; a new link without them is FS with no lag. A lag can be written `+2d`, as `--add-link` takes it.

Every problem is reported at its sheet, row and column, as `Activities row 3 column original_duration_hours`, and nothing changes unless every row is good. The edits come before the run's others, and `--from-sheet` can't be used with `--where` or `--apply`. A file with several projects needs `--project`.

## Apply timesheets

Team members' hours and progress come back on the timesheets `timesheet` writes, and `--from-timesheet` applies them: hours add to each assignment's actual units, and progress goes on each activity. See [Collect timesheets](timesheets.md).

## Record an update

A monthly update moves the data date and records what happened, in one command:

```sh
totalfloat edit plant.xer --data-date "2026-01-19 08:00" --actual-start "A1=2026-01-12 08:00" --actual-finish "A1=2026-01-12 17:00" --actual-start "A2=2026-01-13 08:00" --remaining A2=1h -o plant-update.xer
```

When most work went as planned, record that first and correct the exceptions in the same command. `--as-planned` takes each activity's calculated dates before the move. Work due to start before the new data date starts then. Work due to finish by it finishes then, and work still underway keeps the working time left to its planned finish. Your own edits come after, as corrections:

```sh
totalfloat edit plant.xer --data-date "2026-01-13 12:00" --as-planned --remaining A2=8h -o plant-planned.xer
```

An activity your own progress edits name is left to them, so give its progress in full: for work planned to finish that's still going, its actual start and remaining duration. Finished work, level of effort and WBS summary activities are left as they are, and the report says how many started and finished, and how many had no calculated dates. The data date has to move later, and the file has to calculate as it stands.

Actuals you give are checked against the new data date, so a finish in the future is refused. Moving the data date back past actuals already in the file gives a warning. Level of effort and WBS summary activities take their progress from the work they span, so they can't be set. Percent is stored as given. It doesn't change remaining duration, which you give yourself.

## Take a status table

Progress often comes back as a table. Save it as CSV with a header, one activity a row: `activity`, and any of `actual_start`, `actual_finish`, `remaining` and `percent`. A blank cell leaves that value as it is.

```text
activity,actual_start,actual_finish,remaining,percent
A1,2026-01-12 08:00,2026-01-12 17:00,,
A2,2026-01-13 08:00,,4h,50
```

```sh
totalfloat edit plant.xer --data-date "2026-01-13 12:00" --progress-table status.csv -o plant-status.xer
```

Each row becomes a progress edit, applied before any other edits in the command. If any row can't be used, nothing is changed, and the error names every such row by its line, such as a date that isn't a date or an activity the file doesn't hold. Cells can be quoted, so a value can hold a comma. Without `-o`, you see the impact and nothing is written.

## Links between projects

Name an activity in another project as `PROJECT:CODE`, where `PROJECT` is the project's short name, on any link edit:

```sh
totalfloat edit two-projects.xer --link "SYN1:A1>SYN2:B1:FS+8h" -o linked.xer
```

If only one end names its project, the other has to be in just one project in the file. A link needs at least one project name to leave its project, so a typo can't join two projects by accident.

Both projects recalculate, each using the other's new dates until they settle, so these edits can't be limited with `--project`. A link that would close a loop through the projects is refused. A project linked to one you edit also recalculates, unless you name one with `--project`, when a warning says so.

## Units and costs follow durations

When an edit changes a duration, each of the activity's assignments moves with it, by the activity's duration type ([TF-038](../rules/TF-038-units-and-costs-follow-durations.md)):

- **Fixed units per time** and **fixed duration and units per time** keep the units per hour, so the units change.
- **Fixed units** and **fixed duration and units** keep the units, so the units per hour change.

Work not started keeps its remaining units and cost equal to its budget while its two durations agree. A milestone takes no resources, so an activity with them can't become one, and finished work takes no new ones. Each activity's labor and nonlabor unit totals move with its assignments. A resource whose name ends in a colon and a number is assigned through a JSON edit, since `CREW:40` reads as 40 units. A cost linked to its units moves with them at the price the file already used, which can differ from today's price. Actual units and costs never change. The edit's line names each assignment that moved and by how much, for example `CREW: remaining units 40 to 48`.

## What it won't change

- **Actual units and costs.** A duration edit moves units and costs (see above), but progress doesn't record actual units or costs on the assignments, and a warning says so. A new assignment's price comes from P6, so its cost starts at zero. An assignment with actual units or cost can't be removed here.
- **Deleting WBS nodes, and the project's own node.** Both are left to P6.
- **A global code type only another project uses**, which this project can't set. Nor can a code type be changed or deleted here, or a resource deleted. Of a resource, only its limit and user-defined field values change here. The rest is left to P6.
- **Deleting an activity linked to another project.** Remove that link first.
