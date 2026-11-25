# TF-021: Lag from work that has happened

How much of a relationship's lag is left when the end it measures from has already happened.

## Statement

1. A link measures from one end of its predecessor: its start for start-to-start and start-to-finish links, its finish for finish-to-start and finish-to-finish links. That end has happened when the predecessor has an actual date there: an actual start on started or finished work, an actual finish on finished work.
2. On such a link, the lag counts down in work time on the lag calendar (TF-006) from the actual date to the data date. The part still to run is the lag less that work time, and never less than zero. It runs from where the predecessor stands now: its remaining start or its finished point (TF-020).
3. Lag that ran out before the data date has no effect beyond passing on the predecessor's position, as a link with no lag does.
4. A lead (negative lag) on an end that has happened is used up: it counts as zero.
5. When the actual date is after the data date, no lag has run yet. The time between the two adds nothing.
6. A link between two finished activities carries no lag.
7. The backward pass uses the same remaining lag, so a started or finished predecessor's late dates follow its successors in the same way.
8. A finished predecessor with lag on a link from its start needs an actual start; without one the project is refused as invalid input.
9. With `ss_lag_from_early_start` off, a link from the start of work in progress (start to start or start to finish) runs its full lag from the predecessor's actual start, and it doesn't limit the predecessor's remaining work in the backward pass. With it on, items 2 to 7 apply.

## Options

- **`ss_lag_from_early_start`**: on (default) or off. An XER file selects it through `SCHEDOPTIONS.sched_lag_early_start_flag` (`Y` or `N`). Without one, on applies, because nearly every corpus project that holds the setting has it on. MS Project XML has no such setting.

## Sources

- `standard`: [AACE 10S-90](sources.md#aace-10s-90), DATA DATE: the data date separates what has happened from the forecast, so lag time before it has already passed. Item 2.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), "Lags and Leads", pp. 41–46: a lag stands for real waiting time, such as curing, and a lead lets a successor overlap its predecessor. Items 2 and 4.
- `observed-file`: files in the corpus with progress. Where a link's predecessor started or finished long before the data date, the successor's stored dates show no trace of the lag; where the lag was still running, they show what was left of it, counted from the predecessor's remaining start. A lead on a side that has happened is used up. Where `sched_lag_early_start_flag` is on, start-to-start links from work in progress count from the actual start. Items 2 to 5, 7 and 9.
- `own-choice`: items 6 and 8; the reasoning is under Rationale.

## Rationale

A lag stands for time that has to pass, such as curing. Time between the actual date and the data date has already passed, so only the rest of it is still to wait (item 2). A lead lets the successor overlap work that hasn't happened; once that end has happened there is nothing left to overlap (item 4). Time can't run before something happens, so an actual date after the data date leaves the whole lag to run (item 5).

Two finished activities are both history, so neither can hold the other back (item 6). A finished predecessor with lag from its start and no actual start leaves no date to count the lag from, and TotalFloat doesn't invent one (item 8).

## Tests

`crates/engine/src/progress_tests.rs`: `lag_from_work_that_happened_counts_down_to_the_data_date`, `what_is_left_of_a_lag_runs_from_where_the_predecessor_stands` (item 2), `a_lag_that_ends_before_the_data_date_has_no_effect`, `a_lag_tie_with_the_data_date_is_still_the_data_date` (item 3), `a_lead_on_a_side_that_has_happened_is_used_up` (item 4), `an_actual_after_the_data_date_adds_no_lag` (item 5), `lag_between_finished_work_needs_no_actual_start` (item 6), `lag_from_a_completed_activity_needs_its_actual_start` (item 8), `an_actual_start_link_leaves_remaining_work_free`, `an_actual_start_link_ends_a_chain_on_the_actual_date` (item 9), `provenance_records_the_lag_that_was_applied`.

## Differences

None known.
