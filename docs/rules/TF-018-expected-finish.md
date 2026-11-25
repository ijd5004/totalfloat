# TF-018: Expected finish

How a planner's estimate of when an unfinished activity will finish sets its remaining duration.

## Statement

1. An unfinished activity may carry an expected finish. When the project uses expected finishes, the forward pass fixes the activity's early finish at the expected finish, and its remaining duration for this calculation becomes the work time, on its calendar, from its calculated early start to that finish. The backward pass uses that duration too.
2. The stored remaining duration and the expected finish are inputs, and the calculation doesn't change them.
3. An expected finish at the start or end of a working period stays as it is. One elsewhere in nonworking time moves to the next working time.
4. An estimate that leaves no work after the calculated early start, because it falls at or before that start as a work position, is ignored when the activity has remaining duration, and the remaining duration stands. One before the start is always ignored. On work with no remaining duration, an estimate at the start's position fixes the finish there, so it is honored. An ignored estimate doesn't hold an as-late-as-possible activity (TF-017).
5. A milestone has no work to stretch, so an expected finish on one is ignored.
6. A mandatory date wins over an expected finish (TF-016).
7. An honored expected finish holds an as-late-as-possible activity where it is.
8. An expected finish that conflicts with a finish bound is refused:
   - against a finish bound from a relationship, when there is work between the expected finish and the bound. At the same work position it is no conflict, and the finish takes the bound's spelling (TF-025);
   - against a finish-on or finish-on-or-after constraint, when the expected finish is earlier than the constraint date on the clock.
9. When the project doesn't use expected finishes, they are ignored.

## Options

- **`use_expected_finish`**: on (default) or off. An XER file selects it through `SCHEDOPTIONS.sched_use_expect_end_flag`. Without one, on applies, because nearly every corpus project that holds the setting has it on. MS Project XML has no expected finish.

## Sources

- `observed-file`: files in the corpus that use expected finishes. Their unfinished activities store a remaining finish at the expected finish, read as items 1 and 3 give. Estimates that fall at or before the activity's remaining start store a finish from the remaining duration instead, and an estimate on a nonworking day stores the end of the previous working day, the same work position as the next working time. Items 1, 3 and 4.
- `own-choice`: items 2 and 5 to 9; the reasoning is under Rationale.

## Rationale

An expected finish is the planner's estimate of when the work will end. Stretching or shrinking the remaining duration to meet it keeps the activity's start where logic puts it and its finish where the planner expects it (item 1). Leaving the inputs untouched means switching the option off gives back the original schedule (items 2 and 9).

An estimate at or before the start leaves no work to schedule, and a milestone has none to begin with, so there is nothing for the estimate to set; the remaining duration still describes the work (items 4 and 5). A mandatory date is the stronger statement about the activity's dates, so it wins (item 6). An estimate that would finish the work before logic or a constraint allows would break that bound without saying so, so TotalFloat refuses it and lets the planner decide which input is wrong (item 8).

## Tests

`crates/engine/src/constraint_tests.rs`: `expected_finish_recomputes_work_span_only_when_enabled` (items 1, 2 and 9), `an_expected_finish_before_the_start_is_ignored` and `an_estimate_at_the_start_position_leaves_no_work_and_is_ignored` (item 4), `an_ignored_estimate_doesnt_stop_as_late_as_possible` (items 4 and 7), `a_milestone_has_no_work_for_an_estimate_to_stretch` (item 5), `invalid_dates_and_conflicts_are_explicit_and_expected_estimates_do_not_override_mandatory` (item 6), `conflicting_expected_finish_cannot_silently_break_finish_bounds` (item 8). `crates/engine/src/tests.rs`: `an_expected_finish_at_a_finish_bounds_position_is_no_conflict` (items 3 and 8). `crates/cli/tests/cli.rs`: `an_ignored_expected_finish_says_so` (item 4).

## Differences

None known.
