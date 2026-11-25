# TF-011: XER calendar data

How TotalFloat reads the working week, the exceptions and the base calendar from an XER calendar, and writes them back.

## Statement

1. `CALENDAR.clndr_data` is a tree of nodes, each written `(0||Name(payload)(children))`. The root is `CalendarData`, or has no name. Whitespace, the DEL character (`0x7f`) and double quotes around a multi-line cell only lay the tree out and carry no meaning. The original text, with its quoting, DEL characters and line endings, is kept, so an unedited calendar is written back exactly.
2. `DaysOfWeek` holds one node per weekday, numbered 1 for Sunday to 7 for Saturday. A day's shifts are child nodes whose payload pairs an `s` (start) time with an `f` (finish) time, in either order. A day can have several shifts. A day with no shifts is nonworking.
3. A shift finishing at `00:00` finishes at the end of its day, so a shift from `00:00` to `00:00` is a full working day. TotalFloat writes the end of a day as `00:00`.
4. `Exceptions` holds one node per date, whose payload `d` is a serial day number. Day 0 is 1899-12-30, and each number after it is the next civil day, with no time zone and no adjustment for a leap day that never happened. A serial with a nonzero fraction is a fault (item 10).
5. An exception with no shifts makes its date nonworking. An exception with shifts replaces that date's working time with them.
6. Two exceptions for the same date with the same shifts are one exception; the text keeps both. Two for the same date with different shifts are a fault (item 10).
7. A calendar that names a base calendar takes the base's exceptions, through the whole chain of bases. Its own exception for a date wins over the base's, including one that makes the date nonworking.
8. A calendar's working week and its hours-per-period figures are its own. A calendar that doesn't define every weekday is unresolved: the import reports it, and activities on it aren't calculated (TotalFloat's rule for it is still to be written).
9. A base calendar that's missing, a chain of bases that loops, and a chain more than 64 levels deep are reported, and the calendar is unresolved.
10. A fault makes the whole calendar unresolved: the import reports it, the calendar keeps its text, and activities on it aren't calculated. The faults are a root other than `CalendarData` or unnamed; a section other than one `DaysOfWeek`, one `Exceptions` and a `VIEW` with no children; a weekday outside 1 to 7 or defined twice; a shift that is empty, finishes before it starts (other than at `00:00`) or overlaps another on the same day; an exception date that isn't a whole serial day; and two different definitions of one date.
11. A `VIEW` section with no children describes display and is skipped.

## Options

None.

## Sources

- `standard`: [AACE 10S-90](sources.md#aace-10s-90), CALENDAR: calendars set working and nonworking periods for the project, an activity or a resource. Items 2 and 5.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 4, pp. 66–68: holidays and shutdowns are exceptions to a calendar's working periods. Item 5.
- `observed-file`: the corpus's calendars. Read with Sunday as day 1, most working weeks run Monday to Friday, and many others leave only Friday nonworking; read from Monday instead, the most common week would leave Monday and Sunday nonworking. Shifts finishing at `00:00` mostly also start at `00:00`, and the weekly hours they give agree with the files' own weekly figures. Exception dates read on the 1899-12-30 origin fall on January 1 and December 25 far more often than on the same origin shifted a day or two either way. Many files repeat an exception for the same date, always with the same shifts. Items 1 to 6.
- `observed-file`: in some files in the corpus, calendars lack exceptions their base calendar has, and their activities' stored spans are consistent with the base's exceptions applied. In most other files a calendar already repeats its base's exceptions. Item 7.
- `own-choice`: items 8 to 11, and writing `00:00` in item 3; the reasoning is under Rationale.

## Rationale

The tree's grammar comes from the files, as no public specification of it exists. Weekday numbering and the date origin are settled by what makes the files' calendars sensible: the usual working weeks and the usual holidays (items 2 and 4).

A finish at `00:00` can only mean midnight at the end of the day, since a shift can't finish before it starts. TotalFloat writes the end of a day as `00:00` because that is how the files spell it. Other reversed or overlapping shifts, two different definitions of one date and structure TotalFloat doesn't know have no clear meaning, and using the rest of the calendar without them would give working time the file may not mean. So the calendar isn't used, and the import says why (item 10).

A base calendar holds what many calendars share, such as public holidays, which is why the exceptions come from it (item 7). The corpus supports that for exceptions. It doesn't show a calendar taking its working week from its base: nearly every calendar in it defines a full week of its own. What applies to a calendar with no week of its own is a separate rule, and until it is written, such a calendar isn't used (item 8). A chain of bases that loops or never ends can't be resolved (item 9).

## Tests

- `crates/xer/src/calendar_data/tests.rs`: `weekdays_split_shifts_and_exact_layout_retention` (items 1 and 2), `serial_day_epoch_and_working_exceptions` (items 4 and 5), `midnight_finish_and_identical_exception_repetition` (items 3, 6 and 10), `base_inheritance_keeps_local_week_and_overrides_exceptions` (items 7 and 8), `empty_payload_does_not_inherit_standard_week_or_fabricate_work` (item 8), `missing_bases_cycles_duplicates_and_depth_limit_are_diagnostics` (items 9 and 10).
- `crates/xer/tests/corpus_raw.rs`: `corpus_calendars_decode_and_keep_original_text` (items 1 and 4).

## Differences

- MS Project XML stores calendars as elements, with weekdays numbered 1 for Sunday to 7 for Saturday and exceptions as date ranges ([MSPDI schema](sources.md#msp-xml-schema), Calendar elements). Its `BaseCalendarUID` names a base calendar.
