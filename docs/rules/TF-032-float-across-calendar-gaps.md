# TF-032: Float across calendar gaps

How the backward pass reads a late finish across a calendar gap, and the second float TotalFloat reports where that loses time.

## Statement

1. **Two spellings.** On a calendar, the end of one working period and the start of the next are one work position with two spellings, and a finish is spelled at the end of the working period ([TF-025](TF-025-work-positions-and-clock-moments.md), items 1 and 2).
2. **The backward pass.** A finish-to-finish or start-to-finish link works back from its successor's late finish as spelled, at the end of the working period ([TF-025](TF-025-work-positions-and-clock-moments.md), item 6). The lag is taken away on the lag calendar ([TF-006](TF-006-lag-calendar.md)), or, with no lag, the predecessor's late date follows from that moment on its own calendar.
3. **Time lost.** Where such a link reaches its successor from another calendar, the forward pass can arrive in the successor's nonworking time, such as Saturday on a lag calendar that works weekends, and the successor's calendar moves that to its next working instant. Working back from the finish as spelled then starts earlier than the forward pass arrived, so the predecessor's late dates come out early and its total float low, possibly below zero with no date constraint anywhere.
4. **True total float.** TotalFloat also works out each activity's float with every such link inverted exactly: from the successor's late finish read as its next working instant. That float is the activity's true total float. It is reported only where it differs from total float.
5. **Set dates.** A late finish that a date someone set gives, from a constraint, an external date or the project's must-finish date, is used as it stands in both calculations. Lateness against a set date is real, so it shows in both numbers.
6. **What uses which.** Total float, free float, critical activities and the longest path use item 2. True total float is shown beside total float: `check` lists the unfinished activities whose two numbers differ, and `explain --field total_float` gives both.
7. **When they agree.** When every activity and every lag runs on one calendar, or the network holds only finish-to-start and start-to-start links, the two numbers are always the same.

## Options

None.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 7, p. 92: nonworking time isn't float. Items 3 and 4.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 4, pp. 66–68, and Best Practice 6, pp. 81–82 (Figure 32): several calendars in one network can give activities on one continuous driving path different total float. Items 3 and 7.
- `standard`: [AACE 49R-06](sources.md#aace-49r-06), p. 3 (public preview): activities on one logical chain can show different float because of activity calendars and lag calendars. Items 3 and 6.
- `observed-file`: files in the corpus with unfinished finish-to-finish and start-to-finish links across a calendar gap, where the two inverses give different predecessor dates. Their stored predecessor dates follow the late finish as spelled (item 2). Item 2.
- `own-choice`: items 4 to 7; the reasoning is under Rationale.

## Rationale

The forward pass and the backward pass should be inverses: a link the forward pass satisfies exactly should leave the predecessor zero float through it. Spelling the late finish at the end of the working period breaks that across a calendar gap, because the forward pass arrived somewhere in the gap and the backward pass starts from before it. Stored dates in the corpus that can show the difference read the late finish as spelled, so the main calculation keeps that reading (item 2).

The time lost is the gap itself, and nonworking time isn't float. So total float alone can tell a planner that a schedule is late when nothing about it is. TotalFloat reports the exact inverse beside it as true total float (item 4). Showing both keeps total float as files store it, while the planner still sees how far the work can really slip. Reporting the second number only where it differs keeps it out of the way everywhere else.

A set date isn't a work position the network reached: a must-finish date of Friday 16:00 means Friday 16:00. Moving it to the next working instant would hide real lateness, so both calculations keep it (item 5).

Free float and the longest path keep item 2, so that the activities they pick agree with total float.

## Tests

- `crates/engine/src/tests.rs`: `true_float_keeps_the_time_total_float_loses_to_a_calendar_gap` (items 2 to 4), `a_link_with_no_lag_loses_the_same_time` (item 2), `lateness_against_a_set_date_is_real` (item 5), `true_float_is_absent_when_nothing_differs` (items 4 and 7).
- `crates/engine/tests/properties.rs`: `true_float_is_never_negative_or_below_total_float` (items 3 and 4, over random networks with mixed calendars and every link type).
- `crates/check/src/tests.rs`: `float_lost_to_a_calendar_gap_shows_both_numbers` (item 6).

## Differences

None known.
