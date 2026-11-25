# TF-023: Duration and physical percent complete

How TotalFloat measures an activity's progress by duration or as a physical percent, and how a duration percent and the remaining duration set each other.

## Statement

1. Each activity has a percent complete type: duration, physical or units. Its percent complete is measured by that type. Units percent is TF-004.
2. **Duration percent** is the original duration less the remaining duration, over the original duration, kept between 0 and 100. Finished work is 100. Work with no original duration is 0 until it finishes, so a milestone is 0 or 100.
3. **Physical percent** is the percent the planner enters, as stored. Where the project measures progress by steps, step edits keep it at the steps' weighted percent.
4. On started work measured by duration that isn't a milestone, a progress edit that gives a percent, with no remaining duration and no actual finish, sets the remaining duration to the original duration times (100 − percent) / 100, to the minute. The percent is also kept as the physical percent, so a file or table that carries both says the same thing.
5. In the case item 4 covers, a percent that leaves no remaining minutes after rounding, such as 100, is refused: finishing work takes an actual finish.
6. A remaining duration given in the same edit wins over a percent. Setting a remaining duration changes the duration percent, which item 2 works out from it.
7. Physical and units percents don't change the remaining duration. Work not started keeps its remaining duration whatever its type.

## Options

None. An XER file holds the type in `TASK.complete_pct_type` (`CP_Drtn`, `CP_Phys`, `CP_Units`) and the physical percent in `TASK.phys_complete_pct`; no column stores the duration percent, so it is worked out when a file is read. MS Project XML holds a task's duration percent in `PercentComplete` and its physical percent in `PhysicalPercentComplete`.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Appendix III, Table 8, p. 170: percent complete, units and other ways of measuring progress, each chosen per piece of work. Items 1 and 3.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task elements: `PercentComplete` is the share of the task's duration completed, `RemainingDuration` is the time needed for the unfinished part and can be calculated from percent complete, and `PhysicalPercentComplete` is a separate, entered percent of the work done. Items 2, 3, 4 and 6.
- `own-choice`: items 4 to 7, and the 0 for work with no original duration in item 2; the reasoning is under Rationale.

## Rationale

A duration percent and a remaining duration describe one fact, how much of the activity's time is left, so setting either sets the other (items 4 and 6). Rounding to the minute matches TotalFloat's unit of time. A remaining duration is the more direct statement, so it wins when both are given (item 6). A physical percent is a separate judgment of work done, so it never moves the schedule (item 7).

Work with no original duration has no time to divide, so its duration percent says only whether it has finished (item 2). Finishing work records an actual finish, which a percent doesn't carry, so a percent that leaves nothing is refused rather than inventing a finish date (item 5). A milestone has no duration to divide, so a percent on one doesn't touch its remaining duration (item 4).

The corpus can't check items 2 and 4: XER files don't store a duration percent.

## Tests

`crates/cli/src/query/percent.rs`: `each_type_picks_its_own_measure`, `a_milestone_is_done_only_when_finished`, `remaining_beyond_original_is_zero_percent_not_negative` (items 1 to 3). `crates/edit/src/tests.rs`: `a_duration_percent_sets_remaining_duration` (items 4 and 5), `remaining_duration_sets_the_duration_percent` (item 6), `physical_and_units_percent_leave_remaining_alone` (item 7). `crates/cli/tests/cli.rs`: `a_duration_percent_sets_remaining_and_remaining_sets_it_back` (items 4 and 6).

## Differences

None known.
