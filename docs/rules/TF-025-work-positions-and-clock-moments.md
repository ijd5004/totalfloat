# TF-025: Work positions and clock moments

How the forward and backward passes read dates: within one calendar as positions in work, and across a relationship as moments on the clock.

## Statement

1. **Positions.** On one activity's calendar, a date is a position in work: nonworking time adds nothing, so the end of one working period and the start of the next are one position with two spellings, such as Friday 17:00 and Monday 08:00 ([TF-013](TF-013-work-time-on-a-calendar.md), item 4). This rule says how the passes of [TF-014](TF-014-forward-and-backward-passes.md) read bounds in those terms. Durations and floats are counted in work time between positions, so the spelling changes neither.
2. **Spelling.** A start is spelled as the start of the next working period, and a finish as the end of the previous one.
3. **Links compare moments.** A relationship requires the predecessor's date plus the lag, counted on the lag calendar (TF-006), to be no later than the successor's date, compared as clock moments. A working hour on one calendar that is nonworking on the other still counts for the link.
4. **Forward pass, finish bound.** When a link or constraint bounds a finish, the start comes from the bound's position, and the finish is then at that position: it can differ from the bound only in spelling, and never reads earlier than the bound. No time is added.
5. **Backward pass, start bound.** A bound at a working moment or at the end of a working period is kept as it is. A bound in nonworking time backs up to the end of the previous working period.
6. **Backward pass, finish bound.** The finish backs up to the end of the previous working period. A start milestone keeps a bound at the start or end of a period, since it marks a start.
7. **Ties.** When two bounds give one start, the later finish wins, since two finish bounds at one position can be different moments. The order links are listed in never matters.
8. **A finish milestone** is a finish, so in the backward pass it ends the working period before its successor's start.
9. **The calculated project end** is a moment some activity reached. Where it is a working moment on an activity's calendar, work can finish exactly on it, and work with no duration sits entirely on it. Where it isn't, the activity backs up to the end of the previous working period. As-late-as-possible work follows the same rule. A project deadline is an input, and it is read back to working time like any finish bound (TF-019).
10. **A forward-pass lead** is clock arithmetic on the lag calendar, and its result keeps the spelling that gives.

## Options

None.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 7, p. 92: nonworking time isn't float. Item 1.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §5.11, p. 74: total float is counted in workdays of the task's own calendar. Item 1.
- `standard`: [AACE 49R-06](sources.md#aace-49r-06), p. 3: activities on one chain can show different float because of their calendars and lag calendars. Items 1 and 3.
- `standard`: [Fondahl (1961)](sources.md#fondahl-1961), p. 26: a network has to be kept in one unit of time, so calendar time is converted to working time. Item 1.
- `observed-file`: files in the corpus whose links join activities on different calendars. Their stored dates satisfy each link compared as clock moments, with the predecessor's late finish exactly the lag before the successor's date, and not before the successor's next working period. Finish milestones end the working period before their successor's start. Bounds that fall in nonworking time keep their spelling as items 4 to 6 give. Items 2 to 10.
- `own-choice`: items 7 and 10; the reasoning is under Rationale.

## Rationale

An activity's calendar says when it can be worked, so within one activity only working time can measure anything; the two spellings of one position are the same point in its work (items 1 and 2). Two calendars share no positions, so a link between activities on different calendars can only compare moments on the clock (item 3). Each pass then reads a bound in the activity's own terms: a forward bound on a finish fixes the position the work must reach, and a backward bound on a finish is read at the latest work before it (items 4 to 6).

Two finish bounds can share one work position and still be different moments; keeping the later one means neither link is broken (item 7). A lead is a clock offset on the lag calendar, and spelling its result as anything else would move it by nonworking time (item 10).

## Tests

`crates/engine/src/tests.rs`: `relationships_compare_clock_moments_across_calendars` (item 3), `a_finish_bound_on_a_new_work_period_needs_no_extra_time` (item 4), `a_start_bound_at_the_end_of_a_period_is_kept` (item 5), `finish_bounds_at_one_position_keep_the_later_moment` (item 7), `a_finish_milestone_ends_the_work_period_before_its_successor`, `a_lone_finish_milestone_stays_one_point` (item 8), `the_calculated_end_is_a_moment_a_deadline_normalizes_back`, `a_project_end_outside_working_time_backs_up_on_each_calendar`, `as_late_as_possible_work_reaches_the_same_project_end` (item 9), `mixed_calendar_relationships_remain_feasible_in_both_passes` (item 3, every relationship type, lag sign and lag calendar).

## Differences

None known.
