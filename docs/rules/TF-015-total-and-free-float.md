# TF-015: Total float and free float

How TotalFloat measures an activity's total float and free float from its early and late dates.

## Statement

1. An activity's start float is the work on its calendar from its early start to its late start. Its finish float is the work on its calendar from its early finish to its late finish ([TF-013](TF-013-work-time-on-a-calendar.md)).
2. Its total float is its finish float, its start float, or the smaller of the two, as the `float_type` option says.
3. Its free float is how much work, on its own calendar, it can be delayed from its early dates before any successor's early dates move. For each relationship out of it that bounds it, TotalFloat takes the successor's early date the relationship bounds, steps back through the relationship's type and lag as the backward pass does ([TF-014](TF-014-forward-and-backward-passes.md), items 7 and 8), and finds the start that keeps that date. The free float is the least work from the early start to any of those starts. Relationships the passes leave out, such as those on level-of-effort and WBS-summary activities ([TF-027](TF-027-level-of-effort-and-wbs-summary.md)) and those out-of-sequence progress drops or measures from an actual start ([TF-022](TF-022-out-of-sequence-progress.md), [TF-021](TF-021-lag-from-work-that-has-happened.md)), don't bound it.
4. An activity with no successor measures its free float to the project's finish: the work from its early finish to the project's finish. Under the `make_open_ended_critical` option it measures to its own late finish instead ([TF-026](TF-026-longest-path-and-critical-activities.md)).
5. Free float is never below zero ([TF-005](TF-005-no-forecast-before-the-data-date.md), item 6). A level-of-effort activity's free float is zero ([TF-027](TF-027-level-of-effort-and-wbs-summary.md)).
6. A finished activity has no total float and no free float.

## Options

- **`float_type`**: `finish` (default), `start` or `smallest`. An XER file selects it through `SCHEDOPTIONS.sched_float_type` ([TF-008](TF-008-xer-schedule-options.md)). Without one, finish float applies, because the guidance defines total float as late finish minus early finish and nearly every corpus project uses it.

## Sources

- `standard`: [Fondahl (1961)](sources.md#fondahl-1961), pp. 27–29: total float is latest finish minus earliest finish, the same as latest start minus earliest start; free float is the gap from an operation's earliest finish to the earliest early start of the operations that follow it. Items 1, 3 and 4.
- `standard`: [AACE 49R-06](sources.md#aace-49r-06), p. 3: total float is late finish minus early finish or late start minus early start, and the two can differ. Items 1 and 2, and the option.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §5.11, p. 74: total float is late finish minus early finish, counted in workdays of the task's calendar; free float is the gap from an activity's early finish to the earliest early start among its successors. Items 1, 3 and the default.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), TOTAL FLOAT and FREE FLOAT: total float is how far an activity can slip without delaying completion; free float is how far it can move past its early dates without pushing a successor past its early dates. Items 2 and 3.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 7, pp. 92–93: total and free float defined, with float usually measured to the last activity in the file and nonworking time not counted as float. Items 1, 3 and 4.
- `standard`: [Kelley and Walker (1959)](sources.md#kelley-walker-1959), p. 163: total and free float from earliest and latest times. Items 1 and 3.
- `observed-file`: the stored total and free floats in the unprogressed projects of files in the corpus. No corpus project stores start float or the smaller of the two. Items 1 to 4.
- `own-choice`: item 3 for relationships other than finish to start and for activities on different calendars, the `smallest` value, and item 6; the reasoning is under Rationale.

## Rationale

The sources define total float on the late and early finishes or starts, and AACE notes the two can differ, so the definition is an option ([ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules)) with the guidance's own definition as the default (items 1 and 2). Taking the smaller of the two is the cautious reading when they differ.

The sources define free float for finish-to-start links on one calendar: the gap from an activity's early finish to its successors' early starts. TotalFloat keeps its meaning, the delay that leaves every successor's early dates alone, and applies it to every relationship type and calendar by stepping back through each relationship and measuring the delay on the activity's own calendar, where its work happens (item 3). With no successor, the project's finish is the next thing a delay would move, as GAO describes float measured to the last activity (item 4). A finished activity can't be delayed, so it has no float (item 6).

## Tests

- `crates/engine/src/tests.rs`: `branched_network_float_open_ends_and_provenance` (items 1 to 4, every `float_type` value), `mixed_calendar_relationships_remain_feasible_in_both_passes` (items 1 and 3 across relationship types, lags and calendars).
- `crates/engine/src/constraint_tests.rs`: `tf_005_a_link_broken_by_a_mandatory_date_leaves_zero_free_float` (item 5).
- `crates/engine/src/progress_tests.rs`: `completed_work_collapses_to_the_data_date` (item 6).

## Differences

None known.
