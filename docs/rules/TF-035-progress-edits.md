# TF-035: Progress edits

What recording, changing and clearing an activity's progress does to its status, durations and dates.

## Statement

1. **Status follows the actuals.** An actual start makes work in progress. An actual finish completes it.
2. **Finishing.** An actual finish sets remaining duration to zero, physical percent complete to 100, and clears the activity's float and remaining dates, which finished work doesn't carry.
3. **Milestones.** A milestone happens at one moment, so either actual gives it both and finishes it. If both are given, they must be the same.
4. **Refused actuals.** An actual date after the data date is refused, comparing a data date moved in the same edit. So is an actual finish before the actual start, or an actual finish on work with no actual start. Level of effort and WBS summary activities take their progress from the work they span, so actuals on them are refused.
5. **Percent.** How a percent and the remaining duration set each other is [TF-023](TF-023-duration-and-physical-percent-complete.md) for duration and physical percent, and [TF-004](TF-004-units-percent-complete.md) for units. On finished work, a percent below 100 is refused.
6. **Something left.** Work in progress has something left to do, so an actual start on work whose remaining duration is zero, with no actual finish, is refused.
7. **Clearing progress.** Clearing progress makes the work not started: the actuals go, remaining duration returns to the original duration, and suspend and resume dates go too, because work not started can't carry them.
8. **Where files hold it.** In XER: `TASK.status_code`, `act_start_date`, `act_end_date`, `remain_drtn_hr_cnt`, `phys_complete_pct`, `complete_pct_type`, `suspend_date`, `resume_date`, and the remaining dates `restart_date` and `reend_date`.

## Options

None.

## Sources

- `standard`: [DCMA 14-point schedule metrics](sources.md#dcma-14pt), §4.9, p. 28: an actual date after the status date is invalid. Item 4.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), DATA DATE: the data date separates actuals from the forecast. Items 2 and 4.
- `standard`: [DoD EVMSIG](sources.md#dod-evmsig), §7.2 Definitions, MILESTONE, p. 84: a milestone is a zero-duration event. Item 3.
- `observed-file`: the XER files in the corpus. No completed activity row holds a remaining early date, and every started milestone is complete with both actual dates. Items 2 and 3.
- `own-choice`: items 1 and 4 to 7; the reasoning is under Rationale.

## Rationale

An actual date is a record of what happened, so it can't be later than the data date the record is made at, and a finish can't come before its start (item 4). A level of effort or WBS summary activity measures the work it spans, so giving it progress of its own would contradict that work.

Finished work has nothing left to forecast: its remaining duration, remaining dates and float all describe work still to do (item 2). Physical percent at 100 says the same in the file's own terms.

Finished work has all its work done, so a percent below 100 would contradict its actual finish (item 5).

Clearing progress should give back the activity as it was before work started, and work not started has no suspension to resume from (item 7).

## Tests

`crates/edit/src/tests.rs`: `a_start_starts_work_and_a_finish_finishes_it` (item 1), `finishing_clears_the_forecast_on_finished_work` (item 2), `a_milestone_takes_both_actuals_from_one`, `a_milestone_finishes_from_either_actual_and_they_must_match` (item 3), `progress_refusals`, `percent_or_remaining_alone_checks_no_dates` (item 4), `a_percent_on_finished_work_is_refused` (item 5), `work_in_progress_needs_something_left_to_do` (item 6), `progress_can_be_taken_away`, `clearing_progress_takes_suspension_with_it_and_leaves_spans_alone` (item 7).

## Differences

None known.
