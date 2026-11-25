# TF-006: The lag calendar

Which calendar a relationship's lag is counted on.

## Statement

1. A relationship's lag is time counted on one calendar, the lag calendar. One setting, `lag_calendar`, chooses it for every relationship in a project.
2. The lag calendar is one of:
   - the predecessor's calendar: the calendar the predecessor's dates run on, which is its resources' calendar for resource-dependent work;
   - the successor's calendar, chosen the same way for the successor;
   - a 24-hour calendar, on which every minute counts;
   - the project's default calendar.
3. The forward pass adds the lag, on the lag calendar, to the predecessor's date that the relationship measures from. The backward pass takes the lag away from the successor's late date on the same calendar.
4. A relationship compares moments in time. Where the lag ends is a moment, and the successor's own calendar turns it into the successor's start or finish.
5. A relationship to or from an activity in another project counts its lag the same way, on that activity's calendar where the setting names it.
6. A project whose lag calendar is its default calendar, which has no default calendar and has any relationship, within the project or to another, is refused. One with no relationships schedules as usual.

## Options

- **`lag_calendar`**: `predecessor` (default), `successor`, `24-hour` or `project-default`. An XER file selects it through `SCHEDOPTIONS.sched_calendar_on_relationship_lag` (`rcal_Predecessor`, `rcal_Successor`, `rcal_24Hour`, `rcal_ProjDefault`). Without one, `predecessor` applies, because most corpus projects that hold the setting use it. MS Project XML holds no project-wide setting; see Differences.

## Sources

- `standard`: [AACE 49R-06](sources.md#aace-49r-06), p. 3: activities on one logical chain can show different float because of lag calendars, among other causes, so a lag's calendar is a choice that changes results. Items 1 and 2.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), CALENDAR: calendars set working and nonworking periods for the project, an activity or a resource. Item 2.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task elements, `PredecessorLink` with `LinkLag` and `LagFormat`: a lag has a format, and some formats count elapsed time. Item 2, the 24-hour calendar.
- `observed-file`: files in the corpus whose relationships carry lag between activities on different calendars. Some set the successor's calendar and some the predecessor's, and each file's stored dates are consistent with its own setting. None sets the 24-hour or project default calendar on a project with such links. Items 1 to 4.
- `own-choice`: items 5 and 6; the reasoning is under Rationale.

## Rationale

A lag stands for waiting time, such as curing or delivery, and that time passes on some calendar. Schedulers differ on which, so it's a setting ([ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules)), and an XER file's own setting selects it. Counting the lag on one calendar in both passes keeps the two passes consistent: a link the forward pass satisfies exactly is satisfied exactly by the backward pass too (item 3). Moments in time are the only thing two calendars share, so the end of the lag is a moment and each activity reads it on its own calendar (item 4).

A relationship to another project's activity is still one relationship, so it follows the same setting, using the other activity's calendar (item 5). A default calendar that doesn't exist can't count anything, and choosing another calendar in its place would be a guess, so the project is refused (item 6).

## Tests

- `crates/engine/src/tests.rs`: `tf_006_successor_lag_runs_on_the_successors_calendar_both_ways` (items 1, 3 and 4), `each_lag_calendar_and_mixed_workweeks` (item 2, all four calendars), `mixed_calendar_relationships_remain_feasible_in_both_passes` (item 3, every relationship type, lag sign and lag calendar).

## Differences

- MS Project XML gives each relationship its own lag format and has no project-wide lag calendar. TotalFloat's reader keeps a working-time lag as it is and turns an elapsed lag into the same number of working days, with a note ([MSPDI schema](sources.md#msp-xml-schema), `LagFormat`).
