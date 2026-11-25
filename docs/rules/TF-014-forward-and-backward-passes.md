# TF-014: The forward and backward passes

How TotalFloat calculates early and late dates for work that hasn't started: where the forward pass begins, what each relationship type requires, and where the backward pass ends.

## Statement

1. This rule covers task-dependent activities and milestones with no constraint, expected finish, progress or external date, and the relationships between them. Other rules extend it: constraints, expected finishes and deadlines ([TF-016](TF-016-date-constraints.md) to [TF-019](TF-019-project-deadline.md)), progress and out-of-sequence progress ([TF-020](TF-020-progress-and-the-data-date.md) to [TF-022](TF-022-out-of-sequence-progress.md)), where starts and finishes land ([TF-025](TF-025-work-positions-and-clock-moments.md)), open ends and the longest path ([TF-026](TF-026-longest-path-and-critical-activities.md)), level-of-effort, WBS-summary and resource-dependent activities ([TF-027](TF-027-level-of-effort-and-wbs-summary.md), [TF-028](TF-028-resource-dependent-activities.md)), and other projects ([TF-030](TF-030-links-to-other-projects.md), [TF-031](TF-031-projects-scheduled-together.md)).
2. The passes use the activities' durations and calendars, the relationships with their types and lags, and the project's planned start and data date. An activity's own planned dates and the dates a file stored from an earlier calculation don't decide any date.
3. Unstarted work starts no earlier than the project's anchor: the later of its planned start and its data date, moved to the next working time on the activity's calendar. When the two are equal, the data date is recorded as the cause. A project with neither is refused.
4. A relationship bounds its successor by its predecessor, with the lag counted on the lag calendar ([TF-006](TF-006-lag-calendar.md)):
   - finish to start: the successor starts no earlier than the predecessor's finish plus the lag;
   - start to start: the successor starts no earlier than the predecessor's start plus the lag;
   - finish to finish: the successor finishes no earlier than the predecessor's finish plus the lag;
   - start to finish: the successor finishes no earlier than the predecessor's start plus the lag.

   A lag is a whole number of minutes and may be negative.
5. The forward pass visits activities in an order where every predecessor comes before its successors, taking the lowest activity ID first among those ready. An activity's early start is the earliest moment that satisfies the anchor and every relationship into it. A bound on its finish becomes a bound on its start by taking its duration away on its calendar. Its early finish is its early start plus its duration on its calendar ([TF-013](TF-013-work-time-on-a-calendar.md)).
6. The project's finish is the latest early finish of its activities. A project with no activities has no finish.
7. The backward pass visits activities in the reverse order. Each relationship out of an activity bounds the activity's start (start to start, start to finish) or its finish (finish to start, finish to finish). The bound is the successor's late start, or its late finish for a relationship to the successor's finish, as that date is written, less the lag on the same lag calendar the forward pass used. Where several moments across nonworking time are the same nonzero lag away, the latest of them counts. The activity's late finish is the earliest of the project's finish and every bound, a bound on its start becoming one on its finish by adding its duration on its calendar. Its late start is its late finish less its duration on its calendar.
8. A late finish is written at the end of a working minute on the successor's calendar. Where the lag calendar has working time between that moment and the successor's next working time, measuring from the late finish as written doesn't count it, so the predecessor's late dates can be earlier than the latest it could keep. [TF-032](TF-032-float-across-calendar-gaps.md) reports that time as true total float.
9. The placement of zero-duration activities is still to be written.
10. Work that hasn't started has remaining dates equal to its early and late dates.
11. Every date records its cause: the relationship that drove it, the anchor, the project's finish or the activity's own other date. Among relationships that give the same date, the lowest relationship ID is the cause. A date moved to working time records that move around its cause.
12. A loop of relationships refuses the project, and the error lists every activity in the loop. So does a relationship to an activity the project doesn't hold, an activity with no usable calendar, or a date outside the calendar's range. A refused project gets no dates at all.

## Options

- **`lag_calendar`**: see [TF-006](TF-006-lag-calendar.md).

## Sources

- `standard`: [Kelley and Walker (1959)](sources.md#kelley-walker-1959), pp. 162–163: earliest times come from a forward pass and latest times from a backward pass against a fixed completion time. Items 5 to 7.
- `standard`: [Fondahl (1961)](sources.md#fondahl-1961), pp. 26–28: the forward pass runs in operation-number order and the backward pass starts from the earliest project finish. Items 5 to 7.
- `standard`: [Hendrickson, Project Management for Construction](sources.md#hendrickson-pmc), §10.2, §10.3, §10.6 and §10.7: the forward and backward passes, and activity-on-node networks with finish-to-start, start-to-start, finish-to-finish and start-to-finish links, each with a lead or lag. Items 4, 5 and 7.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), FINISH-TO-START, START-TO-START, FINISH-TO-FINISH, START-TO-FINISH, LAG and LEAD: what each relationship type and a lag require. Item 4.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §5.11, pp. 74–76: the forward pass gives earliest dates and the backward pass latest dates, using calendars, links, lags, durations, constraints and the status date. Items 2, 3, 5 and 7.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Appendix IV, pp. 172–180: a forward and backward pass worked by hand. Items 5 to 7.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), DATA DATE, and [DCMA 14-point schedule metrics](sources.md#dcma-14pt), §4.9, p. 28: the data date separates actuals from forecast, and a forecast before it is invalid. Item 3.
- `observed-file`: the unprogressed projects of files in the corpus. Their stored early and late dates are consistent with this rule, apart from zero-duration tasks and work on calendars changed after the data date ([TF-029](TF-029-calendars-as-the-file-holds-them.md)). Items 2 to 7 and 10.
- `own-choice`: items 2, 3, 11 and 12 where they choose an order, a cause or a refusal; item 7's latest moment and its measure from a late finish as written; the reasoning is under Rationale.

## Rationale

The passes are the critical path method as its founding papers and the guidance describe it (items 4 to 7). The sources start the forward pass at the project's start, and the guidance adds the status date: nothing unstarted can be forecast before it, so the anchor is the later of the two (item 3). Dates a file stored are results of an earlier calculation, and planned dates are targets, so using either as an input would make the result depend on the past; TotalFloat calculates from logic alone (item 2).

Ties need a rule so that the same input always gives the same output, and IDs give one (items 5 and 11). Across nonworking time, the latest of the moments the same lag away is the one that doesn't take float from the predecessor (item 7). The backward pass measures from a successor's late finish as written, the end of a shift, which is where that work finishes; item 8 says what that costs across calendars, and TF-032 measures it. A loop has no first activity, so there's nothing to calculate; reporting the whole loop tells the planner which link to break. A partly calculated project would show dates that look valid and aren't, so a project with any of these faults gets none (item 12).

## Tests

- `crates/engine/src/tests.rs`: `all_relationship_types_and_signed_lags` (item 4), `project_anchors_milestones_and_calendar_adjustment` (items 3 and 11), `branched_network_float_open_ends_and_provenance` (items 5, 6, 10 and 11), `mixed_calendar_relationships_remain_feasible_in_both_passes` (items 4, 5 and 7), `negative_lag_inverse_allows_the_entire_lag_calendar_plateau` (item 7), `true_float_keeps_the_time_total_float_loses_to_a_calendar_gap` (item 8), `cycles_report_the_full_loop_and_do_not_recurse` and `deep_cycles_and_invalid_relationships_return_structured_errors` (item 12), `unsupported_and_invalid_inputs_are_errors_not_partial_results` (items 3 and 12).

## Differences

None known.
