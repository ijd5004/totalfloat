# TF-029: Calendars as the file holds them

TotalFloat calculates with the calendars a file holds, and reports stored dates that those calendars can't give.

## Statement

1. TotalFloat schedules every activity with the calendars in its input as they stand: the activity's calendar, its resources' calendar for resource-dependent work (TF-028), and the lag calendar (TF-006). It never reconstructs an earlier version of a calendar from the dates stored beside it.
2. Dates stored in a file were calculated when the file was last scheduled, and a calendar can change after that. A stored date or duration that the file's own calendar can't give is reported as a difference. It is not evidence about how work time is counted, and no rule, tolerance or per-file adjustment is added to reproduce it.
3. An unfinished activity's stored early, remaining or late date, other than a WBS summary's, at a time no calendar in the file works is such a difference, since these rules never place one there. So is an activity whose stored dates follow its resource assignments' dates where those dates fall outside the hours of every calendar in the file. A finished activity's point can sit in nonworking time by TF-020, and a WBS summary can take such a point from finished work below it (TF-027), so neither is counted.
4. A calendar's last-change date later than the data date doesn't make a file's stored results stale on its own.

## Options

None.

## Sources

- `observed-file`: files in the corpus store spans their own calendars can't give. In one, a calendar lacks two holidays that an identical calendar's base supplies; in another, exception dates repeat around weekends that the stored spans treat as nonworking. Item 2.
- `observed-file`: across the corpus, activities on calendars last changed after the data date are mostly consistent with those calendars. Item 4.
- `own-choice`: items 1 to 3; the reasoning is under Rationale.

## Rationale

TotalFloat answers from its inputs, so the same file always gives the same dates (items 1 and 2). A file holds the calendar at export, and nothing in it ties the stored dates to the calendar they were calculated with. Guessing an earlier calendar from the stored dates would fit the dates to the answer instead of calculating them.

A stored date at a time no calendar in the file works was calculated with hours the file no longer holds (item 3). Making activity dates follow stored assignment dates in that case would take those hours as input without knowing where they came from, so TotalFloat schedules such a file from its calendars and reports the difference.

A calendar edited after the data date is common and usually harmless, so its change date isn't enough to call a file stale (item 4).

## Tests

`xtask/src/fixtures/tests.rs`: `batch_002_regenerates_exactly_and_shows_a_calendar_edit_moving_a_span` (item 2, on the synthetic file `fixtures/xer/batches/002/cal-edit-after-schedule.xer`).

## Differences

None known.
