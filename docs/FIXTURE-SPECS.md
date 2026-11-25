# Authored fixture specs (schema 1)

```sh
cargo xtask fixtures build fixtures/xer/batches/001
# Validate and build edited specs into a new directory:
cargo xtask fixtures build fixtures/xer/batches/001 --out /tmp/new-batch
```

The directory's lowercase `.toml` files are read in sorted order. Each produces
an identically named XER through the fresh writer, plus a shared README and
MANIFEST containing descriptions and SHA-256 hashes of the specs and outputs.
Every spec must be valid before files are written. Rebuilding identical files
is allowed; edited files, symlinks and other differing artifacts are refused.
Use a new output directory when changing a batch. I/O failures may leave a
partial batch; rerunning completes identical artifacts without replacing edits.

Specs and generated instructions use LF on every platform through repository
attributes; XER files retain the fresh writer's CRLF bytes. Filenames permit only
lowercase ASCII letters, digits, hyphens and underscores. The filename stem
becomes the synthetic project's short name, project name and root WBS code.
Project/WBS IDs are 1; activity and calendar IDs are explicit, and relationship
IDs follow spec order starting at 1. Use distinct seeds for separate projects;
duplicate generated project GUIDs across specs are refused, so no two generated
projects share an identity. Duplicate entity IDs and unsupported input
are refused, never silently overwritten or rounded.

## Format

```toml
schema = 1
seed = 42
export_date = "2026-01-01"
data_date = "2026-01-12 08:00"
description = "Authored FS network with a negative lag; no expected dates yet"

[options]
float_type = "finish"
lag_calendar = "predecessor"
out_of_sequence = "retained_logic"

[[calendars]]
id = 100
name = "Authored workweek"
weekdays = [1, 2, 3, 4, 5]
intervals = ["08:00-12:00", "13:00-17:00"]
[calendars.exceptions]
"2026-01-15" = []

[[activities]]
id = 1
name = "Authored predecessor"
calendar = 100
kind = "task"
duration = 480

[[activities]]
id = 2
name = "Authored successor"
calendar = 100
kind = "task"
duration = 480
[activities.constraint]
kind = "start_on_or_after"
date = "2026-01-14 08:00"

[[relationships]]
predecessor = 1
successor = 2
kind = "fs"
lag = -60
```

Unknown keys are errors at every level. `schema`, `seed`, both dates, a nonempty
description, calendars and activities are required. `relationships` and
`options` may be omitted. All datetimes are civil, minute-resolution scheduling
inputs. No field accepts computed schedule results.

Calendar weekdays use Sunday=0 through Saturday=6. Intervals use
`HH:MM-HH:MM`; end-of-day may be entered as `24:00` and is exported as the
`00:00` that XER calendar data uses for the end of a day. Exceptions map dates to replacement interval lists;
an empty list means no work. The first calendar is the project default. Every
calendar must survive the fresh writer's read-back, so unused global calendars
are refused. There is no inferred workweek or duration conversion factor.

Activity kinds are `task`, `loe`, `wbs_summary`, `start_milestone` and
`finish_milestone`. `duration` and optional `remaining` are integer minutes;
remaining defaults to duration. Durations and lags use whole minutes; nonterminating decimal hours are written
at six decimal places and read back to the same minute (TF-012). Optional `actual_start`, `actual_finish`,
`suspend`, `resume` and `expected_finish` are explicit inputs. Status is derived
from actual dates: completed if actual finish is present, in progress if only
actual start is present, otherwise not started. Supply consistent actual dates
and remaining duration; canonical model validation remains mandatory.

Optional `constraint` and `secondary_constraint` tables support `start_on`,
`start_on_or_after`, `start_on_or_before`, `finish_on`, `finish_on_or_after`,
`finish_on_or_before`, `mandatory_start`, `mandatory_finish` and
`as_late_as_possible`. Each requires a `date` except ALAP, which forbids one.
Relationship kinds are `fs`, `ss`, `ff`, `sf`; `lag` is signed integer minutes.

Options support `float_type` (`start`, `finish`, `smallest`), `lag_calendar`
(`predecessor`, `successor`, `project`, `24_hour`) and `out_of_sequence`
(`retained_logic`, `progress_override`, `actual_dates`). Optional booleans are
`make_open_ended_critical`, `float_from_project_finish`, `use_expected_finish`,
`ss_lag_from_early_start` and `set_planned_to_forecast`. Omitted options use the
canonical `ScheduleOptions` defaults, the most common values in the corpus; see TF-008. Generated XERs embed the resulting choices.

## Batch 001

The 23 authored specs cover all four relationship types with both lag signs,
all four lag-calendar modes, nine constraint types, three float types, open
ends, LOE, WBS summary, both milestones and retained-logic/progress-override
out-of-sequence cases. The predecessor, successor and project calendars use
distinct work patterns, all different from the 24-hour lag calendar. The OOS
pair has identical scheduling inputs except its out-of-sequence option; file
identities, seeds and descriptions differ, so the two are separate projects.

Every generated manifest entry says the file has **no stored results**. The
batches are inputs for TotalFloat's own tests and for differential tests against
open-source schedulers; expected results come from the written rules in
`docs/rules/` ([ARCHITECTURE.md, Scheduling rules](ARCHITECTURE.md#scheduling-rules)).

## Batch 002

Three authored specs, each covering a case an engine rule decides:

- `span-links-and-open-starts`: a level of effort with no start link, a level of
  effort linked to another, and spans under a late deadline ([TF-027](rules/TF-027-level-of-effort-and-wbs-summary.md)).
- `zero-duration-after-weekend`: zero-duration tasks that follow a Friday finish
  and a midweek finish (TF-014).
- `cal-edit-after-schedule`: a seven-day calendar with Friday exceptions, for the
  calendar-state question in [TF-029](rules/TF-029-calendars-as-the-file-holds-them.md).

All three fixtures passed the fresh writer's read-back; none has been loaded in
XER Tools. Their expected results come from the rules that cover each case once
they're written, never from a vendor run ([ARCHITECTURE.md, Scheduling rules](ARCHITECTURE.md#scheduling-rules)).
