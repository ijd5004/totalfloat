# Collect timesheets

Team members report the hours they worked and how far their activities got. `timesheet` writes each one a sheet for a period, and `edit --from-timesheet` applies what they fill in.

## Write the sheets

The examples use `crew.xer`, a small schedule with a crew at work in January.

```sh
totalfloat timesheet crew.xer --from 2026-01-05 --to 2026-01-11 --xlsx week-2.xlsx
```

`--xlsx` writes one workbook with a sheet for each resource. `--csv DIR` writes a new folder with a CSV file for each resource instead. `--resource` limits it to one resource, by short name or name, ignoring case; repeat it for several. A file with several projects needs `--project`.

A resource's sheet lists its assignments on unfinished activities whose dates overlap the period, from the start (the actual start once started, otherwise the early start) to the early finish. Labor and nonlabor resources get sheets. Material is counted in its own units, so it has none.

| Column | What it holds |
|---|---|
| `activity`, `name` | The Activity ID and name |
| `resource` | The resource's short name |
| `planned_start`, `planned_finish` | The dates the overlap was worked out from |
| `budget_hours` | The assignment's budgeted units |
| `actual_hours` | The actual units the file recorded when the sheet was written. Leave it as it is |
| `hours` | To fill in: hours worked in the period |
| `remaining_hours` | To fill in: hours still to do |
| `actual_start`, `actual_finish` | To fill in, as `YYYY-MM-DD HH:MM` |
| `percent` | To fill in: the activity's physical percent complete, 0 to 100 |

Each sheet is named after its resource's short name, changed where Excel or a file system won't take it: letters a sheet or file name can't hold become `_`, and a name already used gets ` (2)`.

## Apply what comes back

```sh
totalfloat edit crew.xer --from-timesheet crew.csv -o crew-week-2.xer
```

Give `--from-timesheet` once for each file returned: the workbook, a CSV file, or a folder of CSV files. Every sheet in them is read, and a row's `activity` and `resource` say which assignment it's about, whatever the sheet is called. A sheet with none of the timesheet's columns, such as one added for notes, is skipped. Rows with nothing filled in are skipped, and a blank cell changes nothing.

- `hours` is added to the assignment's actual units.
- `remaining_hours` sets its remaining units.
- `actual_start`, `actual_finish` and `percent` set the activity's progress, once for each activity, before the units. Two team members on the same activity can both report its progress, as long as they agree.
- An activity whose percent complete is by units takes its percent from the hours, so `percent` stays blank for it.

The workbook's `percent` column is formatted as text, so `50` and `50%` both mean half done. Excel stores a percent typed with `%` in a number cell as a fraction, so a `percent` below 1 there is refused.

The run is all or nothing. Nothing is written if any of these is true:

- a cell isn't a date, a number of hours or a percent;
- a `percent` is given for an activity whose percent complete is by units;
- an activity or resource is unknown, or isn't assigned;
- two rows give the same assignment;
- two rows give the same activity different progress;
- a row's `actual_hours` no longer matches the file. That means the sheet was written from another version of the schedule, or it's already been applied, so applying it again would count the hours twice.

Each problem is named at its file, sheet, row and column, as `crew.csv crew row 2 column hours`.

The activity's work totals follow its assignments' units, so the saved file changes in the assignments' actual and remaining units, the activity's progress, and those totals. When progress is given for an activity and some of its assignments get no new hours, the run warns that their actual units are still to record. The edits come before the run's others. `--from-timesheet` can't be used with `--where` or `--apply`.
