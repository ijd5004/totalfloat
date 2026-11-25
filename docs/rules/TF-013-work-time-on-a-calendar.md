# TF-013: Work time on a calendar

How TotalFloat counts working time between two moments on a calendar, and which moment it picks when it adds working time to one.

## Statement

1. The work between two moments on a calendar is the number of working minutes from the first, inclusive, to the second, exclusive. It is signed: swapping the two moments negates it.
2. A date's working time is its exception's shifts if the calendar has an exception for that date, and otherwise the shifts of its weekday. An exception replaces the whole day. Exceptions a calendar takes from its base are resolved first ([TF-011](TF-011-xer-calendar-data.md)).
3. A calendar's hours-per-day, per-week and per-month figures convert durations for display. They don't change which minutes are working time.
4. Each moment has a work position: the working minutes counted from a fixed point of the calendar. The end of one shift and the start of the next share a position, across the nonworking time between them.
5. Adding a positive amount of work gives the earliest moment at the position reached, so work that ends at the end of a shift finishes at the end of that shift. Adding a negative amount gives the latest moment at the position reached, so work that starts at the start of a shift starts there. Adding zero gives back the moment it started from, even in nonworking time.
6. The next working moment at or after a given moment is the start of a working minute. The previous working moment at or before it is the end of one.
7. Adding an amount of work and then taking it away returns to the same position. A moment that starts a working minute comes back unchanged when work is added and then taken away, and a moment that ends a working minute comes back unchanged when work is taken away and then added.
8. The 24-hour calendar has every minute of every day as working time, with no exceptions. Work that runs past midnight on any calendar is two shifts on two dates, one ending at the end of the first day and one starting at the start of the next.
9. A shift that is empty, finishes before it starts or overlaps another, a working week with no working time (even with working exceptions), and a moment outside the years 1 to 9999 are errors. TotalFloat never wraps or clamps them.

## Options

None.

## Sources

- `standard`: [AACE 10S-90](sources.md#aace-10s-90), CALENDAR: calendars set the working and nonworking periods of the project, an activity or a resource. Items 1 and 2.
- `standard`: [Fondahl (1961)](sources.md#fondahl-1961), p. 26: times may be in working or calendar days, and a network is kept in one unit by converting between them. Items 1 and 8.
- `standard`: [Hendrickson, Project Management for Construction](sources.md#hendrickson-pmc), §10.5: turning project day numbers into calendar dates skips weekends and holidays; hours per day are set to match estimating units. Items 2 and 3.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §5.11, p. 74: total float is counted in workdays of the task's calendar. Item 1.
- `observed-file`: for unstarted activities with no constraint in the corpus's XER files, the work their calendar counts from the stored early start to the stored early finish equals the stored remaining duration. Nearly all stored starts begin a working minute and nearly all stored finishes end one. Items 1 to 4 and 6.
- `own-choice`: items 4 to 9 where they choose one moment among several at the same position, and the errors; the reasoning is under Rationale.

## Rationale

Durations, lags and float are amounts of working time, which every source above counts on the activity's calendar, so TotalFloat counts working minutes and nothing else (items 1 to 3). Display figures such as hours per day only change how a number is shown.

Between two shifts there is nonworking time, and every moment in it is the same distance in work from any other moment. A finish belongs at the end of the work it finishes, and a start at the beginning of the work it starts, which is what the stored dates show. So adding work forward stops at the earliest such moment and adding it backward at the latest (items 4 to 6). Adding zero changes nothing, which keeps a milestone where logic put it; how an activity's own start and finish are placed is the engine's rule. Round trips that keep the position keep the two passes of the schedule consistent with each other (item 7).

Midnight is the only boundary two dates share, so work across it is split there (item 8). Inputs with no sensible meaning, and dates the civil calendar can't hold, stop the calculation with an error, since any value TotalFloat chose for them would be a guess (item 9).

## Tests

- `crates/calendar/src/tests.rs`: `split_shifts_and_distinct_start_finish_boundaries` (items 4 to 6), `signed_addition_and_work_coordinates_round_trip` (items 1, 5 and 7), `elapsed_calendar_and_signed_interval_counts_agree` (items 1 and 8), `cached_week_and_exception_coordinates_match_minute_oracle` (items 1 and 2, against a minute-by-minute count), `exceptions_replace_days_and_cache_patterns_without_stale_mutations` (item 2), `midnight_spans_sorted_patterns_and_display_factors_are_explicit` (items 3 and 8), `invalid_days_empty_weeks_range_edges_and_huge_spans_are_errors` (item 9), `selecting_a_coordinate_matches_searching_every_minute` (item 5).

## Differences

None known.
