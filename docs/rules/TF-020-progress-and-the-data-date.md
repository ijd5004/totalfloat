# TF-020: Progress and the data date

What TotalFloat calculates for work that has finished or started, given the data date.

## Statement

1. The data date separates what has happened from the forecast. An activity's actual start and actual finish are inputs, and the calculation never changes them or copies them into its early or late dates.
2. **Finished work** is one point in each pass. Its early start and early finish are the data date, as it stands, even in nonworking time, unless out-of-sequence progress pushes it later (TF-022). Its late start and late finish are the bound its successors give, as it falls, with no move to working time, so the point can fall before the data date. Finished work has no remaining dates and no float.
3. **Work in progress** schedules its remaining duration from the next working time at or after the data date, or from its resume date when that is later. Its early dates are its remaining dates, and its late dates its remaining late dates. Work in progress with no remaining duration is one point.
4. **Work not started** gets no early start before the data date (TF-005).
5. Constraints apply only while they can bind: work in progress keeps only its finish constraints, finished work keeps none, and as late as possible applies only to work not started (TF-017). A mandatory finish that leaves work in progress too little time for its remaining work gives way to the logic and the data date in the forward pass, and keeps the late finish (TF-005, item 7). An expected finish before the remaining work can start is ignored (TF-018).
6. A link from finished or started work runs only the part of its lag still to run (TF-021).
7. A successor that has finished passes its late point back to its predecessors like any other successor.
8. A finished activity needs an actual finish. Its actual start is needed only when a link has lag to count from it.
9. These inputs are refused rather than approximated:
   - progress with no data date, and status, actual, suspend and resume dates that contradict each other (invalid input);
   - work in progress that is suspended with no resume date.

## Options

None. Out-of-sequence progress has its own options (TF-022).

## Sources

- `standard`: [AACE 10S-90](sources.md#aace-10s-90), DATA DATE: the data date separates actuals from forecast. Items 1 to 4.
- `standard`: [DCMA 14-point schedule metrics](sources.md#dcma-14pt), §4.9, p. 28: a forecast date before the status date and an actual date after it are both invalid. Items 3 to 5 and 9.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 9, "Statusing Progress", pp. 121–122: the status date (data date) divides work performed from remaining work; dates before it are actual and dates after it are forecast, and all unfinished work is scheduled after it. Items 1, 3 and 4.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task elements, `Resume`: the date the remaining part of a task is scheduled to resume. Item 3.
- `observed-file`: files in the corpus with progress and no out-of-sequence links. Finished activities store one point at the data date as early dates, unmoved to working time, and no remaining dates. Activities in progress store early dates equal to their remaining dates, from the data date. Items 2, 3 and 7.
- `own-choice`: items 5, 8 and 9; the reasoning is under Rationale.

## Rationale

What has happened is a fact, so it is an input, and the forecast begins at the data date (items 1 to 4). Finished work still takes part in both passes as a point, so logic can be traced through it (TF-026) and, under retained logic, unfinished predecessors can still push it (TF-022). A late point that fell before the data date isn't moved forward, because doing so would make the backward pass report finished work as later than its successors allow (item 2).

A constraint describes when work may start or finish. Once work has started, its start is a fact and a start constraint has nothing left to bind; once it has finished, neither has (item 5).

TotalFloat refuses progress it can't read consistently, because any answer would be a guess about which input is wrong (item 9). A suspended activity with no resume date gives no point to restart from.

## Tests

`crates/engine/src/progress_tests.rs`: `completed_work_collapses_to_the_data_date`, `a_lone_completed_activity_ends_at_the_data_date`, `a_nonworking_data_date_is_stored_as_it_is_on_completed_work`, `a_completed_late_point_is_the_bound_as_it_falls` (item 2); `in_progress_work_reports_only_what_remains`, `remaining_work_starts_at_the_next_work_after_the_data_date`, `a_later_resume_date_holds_remaining_work`, `in_progress_work_with_nothing_remaining_is_one_point` (item 3); `unstarted_predecessors_stay_on_their_early_dates` (item 4); `constraints_stop_applying_as_work_progresses`, `remaining_work_is_never_placed_before_the_data_date` (item 5); `a_finished_successor_still_passes_on_its_late_point` (item 7); `lag_between_finished_work_needs_no_actual_start`, `lag_from_a_completed_activity_needs_its_actual_start` (item 8); `impossible_progress_is_refused`, `unsupported_progress_is_refused_rather_than_approximated` (item 9).

## Differences

- MS Project can move completed parts of a task after the status date back to it, and remaining parts before it forward to it, each as a separate option ([MSPDI schema](sources.md#msp-xml-schema), Project elements; [Project VBA reference](sources.md#msp-vba-reference), `OptionsCalculation`, `MoveCompleted` and `MoveRemaining`). TotalFloat always forecasts remaining work from the data date and never moves actual dates.
