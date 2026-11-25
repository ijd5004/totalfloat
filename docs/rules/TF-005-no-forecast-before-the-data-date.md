# TF-005: No forecast before the data date

Where a mandatory date gives way: to the data date for work that hasn't started, to the logic for work that has, and to zero for free float.

## Statement

1. An activity that hasn't started gets no early start before the data date, comparing the two as moments in time. When a mandatory start or mandatory finish would put its early start earlier, the forward pass schedules it from the data date instead: its early start is the data date, moved to the next working time on its calendar, and its early finish follows from its remaining duration. The mandatory date still overrides the activity's links, so predecessors that finish later don't move it.
2. A milestone held this way is at the data date, as item 1 gives for any activity with no duration.
3. Successors measure from these early dates as from any others.
4. The backward pass still sets the activity's late dates from the mandatory date. Its total float is negative by the time between the two.
5. A mandatory date that leaves the early start on or after the data date applies as it always has: it replaces the dates that logic gives, in both passes.
6. Free float is never below zero. Where a mandatory date breaks a link, so a predecessor already finishes past the point the link allows, that predecessor's free float is zero, and its total float shows the conflict.
7. An activity that has started keeps the early dates its links and the data date give when its mandatory finish leaves too little time for its remaining work: that is, when the remaining work would have to start earlier, measured in work on the activity's calendar. The mandatory finish doesn't move the remaining work or shorten it, and the project isn't refused. The backward pass sets the activity's late finish from the mandatory finish, so the activity, and the work driving it, show negative total float of the size of the miss. Its successors measure from its early dates (item 3).
8. A mandatory finish that leaves an activity that has started enough time for its remaining work applies as item 5 says.

## Options

None.

## Sources

- `standard`: [DCMA 14-point schedule metrics](sources.md#dcma-14pt), §4.9, p. 28: a forecast date before the status date is an invalid date. Items 1 and 2.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), DATA DATE: the data date separates actuals from forecast. Items 1 to 3.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), DATE CONSTRAINT: a date constraint overrides logic. Items 1 and 5.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 7, pp. 92–93: total float goes negative when a finish constraint is earlier than logic allows, and free float is the part of total float usable before the next activity is delayed. Items 4 and 6.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), FREE FLOAT: how far an activity can move past its early dates without pushing a successor past its early dates. Item 6.
- `observed-file`: a file in the corpus holds an unstarted finish milestone whose mandatory finish falls before the data date. Its stored early dates are the data date, though its predecessors finish later, and its stored late dates follow the mandatory date, with negative total float. Its predecessors store zero free float. No free float stored in the corpus is negative. Items 1, 2, 4 and 6.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 7, pp. 92–93, as above: a finish constraint earlier than logic allows gives negative total float. Item 7.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), RETAINED LOGIC and DATA DATE: the remaining work of a started activity waits for its predecessors, and the forecast starts at the data date. Item 7.
- `standard`: [DCMA 14-point schedule metrics](sources.md#dcma-14pt), §4.7, p. 27: any negative float is flagged for review. Item 7.
- `own-choice`: items 7 and 8, the maintainer's decision (2026-11-21); the reasoning is under Rationale.

## Rationale

The data date is where the forecast begins. A mandatory date says the planner wants a date whatever logic says, and TotalFloat honors that against the activity's links, but a date before the data date has already passed and can't be forecast (items 1 and 2). The planner's date isn't lost: the backward pass keeps it, so the activity and the work leading to it show negative float of the size of the miss (item 4). That is what the guidance asks for: dates nobody can work to don't appear in the forecast, and the conflict shows as negative float.

Free float measures how far an activity can slip before it moves a successor. A distance an activity can slip can't be below zero. When a mandatory date has already broken a link, the predecessor's total float shows the conflict, and a negative free float would count it a second time (item 6).

An activity that has started has an actual start and remaining work. A mandatory finish that leaves too little time for that work could shorten the work, hold the activity at the mandatory date against its logic, or give way. TotalFloat lets it give way in the forward pass (item 7). The remaining duration is the planner's estimate of the work left, and a date constraint isn't a way to change it. Holding the work at the mandatory date would forecast it finishing before its predecessors let it, or before the data date. So the early dates follow the logic, as retained logic and the data date say they should, and the planner's date stays in the backward pass, where the miss shows as negative float on the activity and on the chain driving it. That is the signal the guidance asks for: negative float is a finding to review, and the schedule is still calculated, so the rest of the project's dates stay useful. Unstarted work is different (item 1): there the mandatory date still overrides links, because the planner can still move work that hasn't started to meet it. A mandatory finish that leaves room is the planner's date and applies (item 8).

## Tests

- `crates/engine/src/progress_tests.rs`: `tf_005_a_mandatory_date_before_the_data_date_holds_unstarted_work_at_it` (items 1 to 4), `tf_005_a_mandatory_date_after_the_data_date_still_overrides_logic` (item 5), `remaining_work_is_never_placed_before_the_data_date` (item 7, a mandatory finish before the data date), `tf_005_started_work_keeps_its_logic_when_its_mandatory_finish_is_too_early` (items 7 and 8).
- `crates/cli/tests/cli.rs`: `explain_shows_what_sets_each_date_on_started_work_past_its_mandatory_finish` (item 7: the link sets the early dates, the mandatory finish the late finish).
- `crates/engine/src/constraint_tests.rs`: `tf_005_a_link_broken_by_a_mandatory_date_leaves_zero_free_float` (item 6), `mandatory_dates_override_logic_and_propagate_to_neighbors` (item 5).

## Differences

None known.
