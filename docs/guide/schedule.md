# Schedule a file

`schedule` calculates a file's dates, float and critical path.

```sh
totalfloat schedule plant.xer
```

```text
Project rel-fs-positive (1)
  data date: 2026-01-12 08:00
  finish: 2026-01-13 17:00 (was none)
  activities: 2, critical: 2, changed: 2, on dates or float: 2
  changed fields: early_finish 2, early_start 2, free_float 2, late_finish 2, late_start 2, longest_path 2, planned_finish 2, planned_start 2, remaining_early_finish 2, remaining_early_start 2, remaining_late_finish 2, remaining_late_start 2, total_float 2
  differences start at:
    A1 early_start: none to 2026-01-12 08:00
    A2 late_start: none to 2026-01-13 09:00
Nothing written; pass -o to write a new file
```

`finish` shows the new project finish and the one stored in the file. A `note` follows if the file's project finish disagrees with the dates saved on its unfinished work, which is a sign the file wasn't saved straight after a calculation.

`changed` counts activities whose stored results differ from the calculation, and `on dates or float` counts the unfinished ones whose early or late dates or total float differ. The rest differ only on fields such as remaining dates or float paths, or are finished work, whose stored dates follow the data date. On a file P6 has just scheduled, a high number is worth a look: see [Limitations](limitations.md).

When something changed, `changed fields` counts each one, and `differences start at` names where they begin, which is the place to look first. For early dates that's a changed activity with no changed predecessor, and for late dates or float, one with no changed successor. Finished work isn't listed, and level of effort and WBS summary activities can't be a starting place, because their dates follow other activities. `--json` gives the old and new dates and float for up to 25 activities, under `changes`.

Add `-o` to write the results to a new file:

```sh
totalfloat schedule plant.xer -o plant-scheduled.xer
```

Only result fields change, and every other byte stays as it was. Results go into the file's own result fields, so other tools can read them.

Move the data date first with `--data-date`, or pick one project in a multi-project file with `--project`:

```sh
totalfloat schedule plant.xer --data-date "2026-02-02 08:00"
```

```sh
totalfloat schedule plant.xer --project 1
```

If a project can't be calculated, the command stops before writing anything and says why. Scheduling a project alone usually gets past a problem in another one.

## Several projects together

The projects calculated are every project in the file, or the ones named with `--project`. A relationship between two of them uses the other project's newly calculated dates in both passes, so a change ripples through any number of links, either way. Each project reads the others' latest results until nothing changes, so running `schedule` twice gives the same file. A project that has never been calculated, with no dates stored on its linked activities, is calculated along with the rest.

Each project keeps its own data date and its own schedule options. A project not chosen with `--project` isn't recalculated: its stored dates stand in.

The file's setting `sched_use_project_end_date_for_float` decides which finish late dates count back from:

- **each project** (`sched_use_project_end_date_for_float` `Y`, the usual setting): the project's own finish;
- **opened projects** (`N`): the latest finish of the projects open together. That finish is also stored as the project's finish. Each project's longest path still runs back from its own last activity.

Free float counts successors in other projects. Relationships between projects that make a loop are refused before anything is written, with the activities in the loop named, as `PROJECT: ACTIVITY`.

The MCP `schedule` tool and the app's recalculation work the same way. `explain`, `query` and `check` recalculate one project with every project in the file open, so after `schedule --project`, their late dates for an **opened projects** project can differ from the ones written.

## The scheduling log

`--log` also writes a log of the run to a new file:

```sh
totalfloat schedule plant.xer --log schedule.log
```

For each project calculated, the log gives:

- **Settings:** every scheduling option used, in words, including whether relationships to other projects were counted.
- **Statistics:** activities by status, milestones, relationships (and how many cross to other projects), and constraints.
- **Problems,** each listed in full, by Activity ID: unfinished activities without predecessors or successors, out-of-sequence progress, actual dates after the data date, constraints with their dates, negative float, and external dates from other projects.
- **Results:** the start and finish, the finish before this run, the number of critical activities, and whether the finish meets the must-finish date.

The same file and options always give the same log, so two logs can be compared. An existing file is never replaced.

## Ignoring other projects

`--ignore-external` calculates each project on its own logic. Links to activities in other projects are left out, whether those projects are in the file or not. So are the external early start and late finish that P6 stores on an activity for such a link.

```sh
totalfloat schedule plant.xer --ignore-external
```

A file that links to a project it doesn't hold is refused without the flag, and calculates with it. The text report starts by saying the option was used, and `--json` gives `"ignore_external": true`. The MCP `schedule` tool takes `"ignore_external": true` too.

It applies to that run only. The file field for the option isn't confirmed, so the file's own setting is left as it is. `edit` and the app recalculate with the file's options, so links to other projects count there.

## Float paths

Float paths show which chains of work would push an activity out first, second and so on. Ask for them with `--float-paths`, how many to find, and if wanted `--float-path-to`, the Activity ID they end at:

```sh
totalfloat schedule plant.xer --float-paths 3 --float-path-to A2
```

The end activity has to be unfinished, and can't be a level of effort or WBS summary. In a file with several projects, only the project that holds it gets paths. A file whose activities have no float path fields is refused, because the paths would have nowhere to go.

Path 1 is the chain of driving links into the end activity. Without `--float-paths-by`, the rest are numbered the way the file's own options say. `--float-paths-by total-float` numbers them by total float: the next path starts from the activity with the least slack summed along its route to the end activity. `--float-paths-by free-float` numbers them by free float: the next path starts from the activity nearest a path already found, so a chain that joins another a few hours out comes before one that joins much further out. Ties go to the lower relative float, then the later early finish, then the lower internal ID (`task_id` in XER). The full rule is in [Multiple float paths](../rules/TF-002-multiple-float-paths.md).

The paths are written to each activity's float path and its order on the path, which `query --columns code,float_path,float_path_order` lists. Without the flags, a file whose options turn float paths on gets them as those options say. The flags apply to that run only, and the file's own options are left as they are. The MCP `schedule` tool takes `float_paths`, `float_path_to` and `float_paths_by` too.

