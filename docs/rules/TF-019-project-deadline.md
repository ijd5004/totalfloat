# TF-019: The project deadline

Where the backward pass starts: the project's own deadline when it has one, otherwise the calculated finish.

## Statement

1. The calculated project finish is the latest early finish of the project's activities ([TF-014](TF-014-forward-and-backward-passes.md), item 6), leaving out level-of-effort and WBS summary activities (TF-027). It is a moment some activity reached (TF-025).
2. A project may have a deadline, a must-finish-by date. When it does, the backward pass starts from the deadline: every activity's late finish is at most the deadline, read on its own calendar at the end of the previous working period when the deadline falls in nonworking time, and an activity that nothing later limits takes it as its late finish. Its links can only bring a late finish earlier. Without a deadline, the calculated finish plays the same part.
3. The deadline doesn't change the calculated finish or any early date.
4. A deadline before the calculated finish gives the activities that reach it negative total float by the difference. A later deadline gives them positive total float.
5. Free float measures to successors' early dates. An activity with no successor measures its free float to the calculated finish, never to the deadline. A level of effort has no free float (TF-027).
6. When projects are scheduled together, the calculated finish in items 2 and 5 can be the latest finish among them (TF-031).

## Options

None. An XER file holds the deadline in `PROJECT.plan_end_date`. MS Project XML holds deadlines per task (`Deadline`), which TotalFloat doesn't read as a project deadline.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 7, pp. 92–93: total float is how far an activity can slip before the finish slips, and it goes negative when a finish date is earlier than logic allows; footnote 28 notes that float is usually computed to the last activity. Items 1, 2 and 4.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), TOTAL FLOAT and FREE FLOAT: total float is measured against project completion or a target date; free float against successors' early dates. Items 2, 4 and 5.
- `standard`: [Fondahl (1961)](sources.md#fondahl-1961), pp. 26–28: the backward pass starts from the earliest project finish when no other finish is set. Items 1 and 2.
- `observed-file`: files in the corpus that hold a must-finish-by date. Their stored late dates count back from the deadline, read on each activity's calendar, while the stored scheduled finish stays at the latest early finish. Items 2 to 4.
- `own-choice`: item 5; the reasoning is under Rationale.

## Rationale

A deadline is a target the planner sets, and the calculated finish is what logic gives. Keeping them apart shows the difference between the two as float (items 3 and 4). The deadline is an input, so where it falls in nonworking time it is read like any other finish bound, at the end of the previous working period (item 2; TF-025).

Free float asks how far an activity can slip before it moves something else. An activity with no successor moves the calculated finish, so its free float runs to that finish. Measuring it to the deadline would mix the target into a measure of the network (item 5).

## Tests

`crates/engine/src/constraint_tests.rs`: `project_deadline_controls_late_dates_not_early_finish` (items 2 to 4). `crates/engine/src/tests.rs`: `the_calculated_end_is_a_moment_a_deadline_normalizes_back` (items 1 and 2), `a_project_end_outside_working_time_backs_up_on_each_calendar` (item 2), `a_longest_path_can_carry_float_and_a_deadline_can_leave_it` (item 4).

## Differences

None known.
