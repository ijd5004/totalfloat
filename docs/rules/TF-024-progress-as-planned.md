# TF-024: Progress as planned

What `edit --as-planned` records when the data date moves later: the progress the last calculation forecast for the time in between.

## Statement

1. The new data date has to be later than the current one, and the schedule has to calculate as it stands. The edit works from that calculation's early dates.
2. **Starts.** Work not started whose early start is before the new data date gets that early start as its actual start. An early start exactly at the new data date hasn't happened yet.
3. **Finishes.** Work whose early finish is at or before the new data date gets that early finish as its actual finish and no remaining duration.
4. **Work still going** keeps as its remaining duration the working time from the new data date to its early finish, on its own calendar, or on its resources' calendar for resource-dependent work (TF-028). Work with no working time left before its finish finishes at the new data date.
5. **Milestones and work with no duration** happen at one moment, so one whose moment is before the new data date is done. Work with no duration that spans nonworking time starts and finishes at its finish.
6. **Left alone:** finished work, level-of-effort and WBS summary activities, activities with no calculated dates, and any activity that the same edit progresses by hand.

## Options

None.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 9, "Statusing Progress", pp. 121–122: statusing records actual dates and remaining work at the status date, and all dates before the status date are actual. Items 2 to 5.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), DATA DATE: the data date separates actuals from forecast. Items 1 to 3.
- `own-choice`: items 1 to 6; the reasoning is under Rationale.

## Rationale

Progress as planned is a starting point for an update: the planner records what the schedule said would happen, then corrects the exceptions. The forecast is the last calculation's early dates, so anything forecast to start or finish before the new data date is recorded as having done so (items 2 and 3), and work still going keeps what its forecast leaves after the data date (item 4). An early start exactly at the new data date is the first moment of the forecast, so it hasn't happened yet; an early finish there has (items 2 and 3).

Spans take their dates from other work, so recording progress on them would contradict what they measure (item 6). An activity the planner progresses by hand in the same edit already has its own progress, which wins.

## Tests

`crates/cli/src/progress.rs`: `work_before_the_data_date_starts_or_finishes`, `finishing_exactly_at_the_data_date_finishes` (items 2 and 3), `remaining_work_skips_the_calendar_s_gaps` (item 4), `zero_duration_work_starts_and_finishes_at_its_finish`, `milestones_happen_at_one_moment_and_spans_are_left` (items 5 and 6), `work_the_caller_progresses_and_work_without_dates_are_left` (item 6). `crates/cli/tests/cli.rs`: `progress_as_planned_settles_and_needs_a_later_data_date` (item 1).

## Differences

None known.
