# TF-008: Schedule options in an XER file

Where an XER file holds each project's schedule options, how TotalFloat reads them, and what applies when a file holds none.

## Statement

1. A project's schedule options come from two rows of an XER file: the project's `SCHEDOPTIONS` row and its `PROJECT` row. The table below gives the field or fields for each option in the model's `ScheduleOptions`.

   | Option | Field | Values read |
   |---|---|---|
   | `out_of_sequence` | `SCHEDOPTIONS.sched_retained_logic` with `sched_progress_override` | `Y`/`N` retained logic, `N`/`Y` progress override, `N`/`N` actual dates |
   | `lag_calendar` | `SCHEDOPTIONS.sched_calendar_on_relationship_lag` | see [TF-006](TF-006-lag-calendar.md) |
   | `float_type` | `SCHEDOPTIONS.sched_float_type` | `FT_FF` finish float, `FT_SS` start float, `FT_Min` the smaller of the two ([TF-015](TF-015-total-and-free-float.md)) |
   | `critical` | `PROJECT.critical_path_type` with `PROJECT.critical_drtn_hr_cnt` | `CT_TotFloat`: total float at most the threshold, in hours; `CT_DrivPath`: on the longest path |
   | `make_open_ended_critical` | `SCHEDOPTIONS.sched_open_critical_flag` | `Y` or `N` |
   | `ss_lag_from_early_start` | `SCHEDOPTIONS.sched_lag_early_start_flag` | `Y` or `N` |
   | `use_expected_finish` | `SCHEDOPTIONS.sched_use_expect_end_flag` | `Y` or `N` |
   | `float_from_project_finish` | `SCHEDOPTIONS.sched_use_project_end_date_for_float` | `Y` or `N` |
   | `set_planned_to_forecast` | `SCHEDOPTIONS.sched_setplantoforecast` | `Y` or `N` |
   | `float_paths` | `SCHEDOPTIONS.enable_multiple_longest_path_calc` and three more fields | see [TF-002](TF-002-multiple-float-paths.md) |

2. A project with no `SCHEDOPTIONS` row takes the default of every option that row holds, and the import reports it once for the project.
3. A flag that is missing or isn't `Y` or `N`, a code TotalFloat doesn't map, or a threshold that isn't a whole number of minutes leaves that option at its default, and the import reports the field. `Y` in both out-of-sequence flags is contradictory and is treated the same way.
4. A `SCHEDOPTIONS` row that names no project, or a project the file doesn't hold, is reported and ignored. Where a project has several rows, the first applies and the others are reported.
5. `set_planned_to_forecast` is read and written back, and no calculation uses it.
6. `SCHEDOPTIONS.sched_outer_depend_type` is kept in the file and reported as unconfirmed on import. It doesn't select `ignore_external_relationships`: that option is off unless a run turns it on (`schedule --ignore-external`).
7. A negative `key_activity_for_multiple_longest_paths` names no activity. The float paths end where they would with the field empty, the value stays in the file, and the import reports it.
8. The resource leveling fields of `SCHEDOPTIONS` stay in the file unchanged. No calculation reads them.
9. Writing back a file TotalFloat read keeps every option cell as the file had it. Writing it back with a changed option is refused. Writing a new file writes every field in the table from the model, with `sched_outer_depend_type` as `SD_Both`.

## Options

This rule says where each option comes from; the rule named in the table says what it does. The defaults, which apply under items 2 and 3:

- **`out_of_sequence`**: retained logic.
- **`lag_calendar`**: predecessor.
- **`float_type`**: finish float.
- **`critical`**: total float at most zero.
- **`make_open_ended_critical`**: off.
- **`ss_lag_from_early_start`**: on.
- **`use_expected_finish`**: on.
- **`float_from_project_finish`**: on.
- **`set_planned_to_forecast`**: off.
- **`ignore_external_relationships`**: off.
- **`float_paths`**: off, by total float, at most ten paths, ending at the project's finish.

Each default is the value most corpus projects that hold the field use (see Sources). Finish float is also the definition of total float the standards give first.

## Sources

- `standard`: [AACE 49R-06](sources.md#aace-49r-06), p. 3: total float is late finish minus early finish, or late start minus early start, and the two can differ. Item 1, the three `float_type` values.
- `standard`: [AACE 49R-06](sources.md#aace-49r-06), pp. 1–3: schedulers pick critical activities by lowest total float, negative total float or the longest path, among others. Item 1, the two `critical` values.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §5.11, p. 74, and [DCMA 14-point schedule metrics](sources.md#dcma-14pt), §3.1.2.3: total float is late finish minus early finish. The finish float default.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 9, pp. 126–129, and [AACE 10S-90](sources.md#aace-10s-90), RETAINED LOGIC and PROGRESS OVERRIDE: two treatments of out-of-sequence progress, which software may offer as a switch. Item 1, `out_of_sequence`.
- `observed-file`: the field names and codes are those in the `%F` and `%R` rows of the corpus's P6 exports. Most of those files hold a `SCHEDOPTIONS` row and some older ones don't. Each default above is the value most corpus projects hold; `sched_outer_depend_type` and `sched_setplantoforecast` hold one value in every corpus project that has them, so the corpus shows nothing about what they change. Items 1 and 2 and the defaults.
- `format-fact`: no corpus file holds `FT_SS` or `FT_Min`; both are in [MPXJ](sources.md#mpxj)'s code list for the field, read for the names only. Item 1, `float_type`.
- `own-choice`: the meanings of `FT_SS` and `FT_Min`, and items 2 to 9; the reasoning is under Rationale.

## Rationale

No openly licensed specification of the XER format exists, so the field for each option comes from the files themselves (item 1). Where a code's meaning can't be read from stored results, TotalFloat reads it from its name: `FT_FF` and `FT_SS` pair finish with finish and start with start, the two definitions of total float AACE gives, and `FT_Min` takes the smaller of them. The corpus can't confirm the reading, since its one start-float project has no progress (see Sources).

A schedule needs every option to calculate. When a file doesn't give one, the most common value in real files is the least surprising choice, and the import says it chose (items 2 and 3). Reporting the default keeps the choice visible to the planner.

Two fields have no meaning TotalFloat can support. Their values never vary in the corpus, so they are kept and written back, and nothing calculates from them (items 5 and 6). Ignoring other projects' links changes results a great deal, so it happens only when a run asks for it. A negative activity ID can't name an activity, so it is treated as no activity (item 7). Leveling isn't calculated yet, so its settings are kept for other software (item 8). Writing a file back keeps its settings as its author left them; until the patch writer can change an option cell, an edit to one is refused, so it is never dropped without notice (item 9).

## Tests

- `crates/model/src/schedule.rs`: `schedule_option_defaults_are_pinned` (the defaults).
- `crates/xer/src/mapping/tests.rs`: `missing_options_use_documented_defaults_and_unlimited_paths_are_explicit` (item 2), `float_type_codes_map_and_unknown_codes_are_reported` (items 1 and 3), `upper_case_lag_calendar_code_is_reported_not_guessed` (item 3).
- `crates/xer/src/fresh/tests.rs`: `calendar_intervals_exceptions_and_options_survive_exactly` (item 9, a new file).
- `crates/xer/src/patch/tests.rs`: `unsupported_edits_are_errors_not_ignored` (item 9, a changed option refused).
- `crates/cli/tests/cli.rs` snapshots `cli__inspect_human` and `cli__inspect_json`: the unconfirmed `sched_outer_depend_type` report (item 6).

## Differences

- MS Project XML holds its own project settings: `CriticalSlackLimit` for the critical threshold and `MultipleCriticalPaths`, among others ([MSPDI schema](sources.md#msp-xml-schema), Project elements). TotalFloat's MS Project reader reports a nonzero `CriticalSlackLimit` and keeps the default threshold.
