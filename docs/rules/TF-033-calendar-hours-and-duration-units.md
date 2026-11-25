# TF-033: A calendar's hours per period, and spans in days, weeks, months and years

How a span typed in days, weeks, months or years becomes work time, and how edits treat a calendar's hours per day, week, month and year.

## Statement

1. **Hours per period.** A calendar holds how many hours make a day, a week, a month and a year. Each is a value of its own: none of them is worked out from the calendar's work week when a file is read.
2. **Spans in a unit.** An edit gives a span in hours (`h`), days (`d`), weeks (`w`), months (`m`) or years (`y`). A span in days is that many of its calendar's hours per day; in weeks, months or years, that many of its hours per week, month or year.
3. **Which calendar.** An activity's durations use the activity's calendar. A lag in days, weeks, months or years uses the calendar the lag runs on under `lag_calendar` ([TF-006](TF-006-lag-calendar.md)).
4. **A missing value.** A calendar without the hours a unit needs refuses a span in that unit, saying so. One exception: a calendar that states no hours per day, and whose working days in its week all have the same length, takes that length as its day.
5. **The 24-hour lag calendar.** On the 24-hour calendar a day is 24 hours and a week 168 hours. A month or a year has no fixed length there, so a lag in months or years on it is refused.
6. **Resource-dependent work.** Resource-dependent work runs on its resources' calendars, which can differ, so it has no one day to count in: its spans are given in hours.
7. **The week changes, the hours don't.** Changing a calendar's work week leaves its hours per day, week, month and year as they were. When the change leaves every working day the same length and that length isn't the hours per day, the edit's report says so.
8. **Setting the hours.** `set_hours_per_day` sets hours per day, in whole minutes up to 24 hours. `set_hours_per_period` sets hours per week, month or year, in whole minutes up to 168, 744 and 8,784 hours. Writing back changes only that cell.
9. **Schedule text.** In the text format a calendar without `copy-of` is its lines, so when its working days are all the same length, that length is its day unless an `hours-per-day` line says otherwise.
10. **Calendar edits in a file.** A changed calendar's working time is written from the model. A calendar based on another then holds the exceptions it inherited as its own. An inherited exception can only be removed on the base calendar.
11. **New calendars.** A new calendar is a project calendar copied from another: it holds its source's working time and hours per day, with no base.
12. **Where files hold it.** In XER, `CALENDAR.day_hr_cnt`, `week_hr_cnt`, `month_hr_cnt` and `year_hr_cnt`, and the working time in `clndr_data`. MS Project XML holds project-wide values: `MinutesPerDay`, `MinutesPerWeek` and `DaysPerMonth`.

## Options

None.

## Sources

- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Project element: `MinutesPerDay`, `MinutesPerWeek` and `DaysPerMonth` give the default minutes per day and per week and working days per month, stored beside the calendars. So hours per period are values a file holds, apart from the work week. Items 1, 2 and 12.
- `standard`: [Hendrickson, Project Management for Construction](sources.md#hendrickson-pmc), §10.5: hours per day are set to match the units estimates are made in, and calendar dates skip nonworking days. Items 1 and 2.
- `standard`: [Fondahl (1961)](sources.md#fondahl-1961), p. 26: times in working days and in calendar days have to be converted so a network stays in one unit. Items 2 and 5.
- `observed-file`: the calendars in the XER files of the corpus. Most calendars whose working days are all one length have that length as their hours per day, and some don't, with hours per week that don't match their week either. Where the two part, activity durations are whole multiples of the stored hours per day. Items 1, 2 and 7.
- `own-choice`: items 4 to 6 and 8 to 11; the reasoning is under Rationale.

## Rationale

A file holds hours per period beside the work week, and the corpus shows the two parting while durations follow the hours per period. So TotalFloat treats them as the file's own values and never rewrites them from the week (items 1 and 7), and the edit's report says when they part, so a planner can set them on purpose (item 8).

Refusing a unit the calendar can't measure is safer than picking a length for it (item 4). The one fallback, a week of equal working days, has only one reasonable day length. On the 24-hour calendar every hour counts, so days and weeks have fixed lengths, while months and years don't (item 5). Resource calendars can differ in their day lengths, so a day on resource-dependent work would mean different time for each resource (item 6).

Writing the model's working time back keeps one source of truth for a calendar's time. Copying the inherited exceptions into a changed calendar keeps what it meant when it was changed, and removing an exception where it is defined keeps the base calendar's other children consistent (item 10). A new calendar copied from another starts from working time the planner knows is right (item 11).

## Tests

`crates/edit/src/tests.rs`: `days_use_the_activity_calendar`, `an_even_work_week_says_how_long_a_day_is`, `days_need_a_calendar_that_says_how_long_a_day_is` (items 2 to 4), `weeks_months_and_years_use_the_calendar_s_hours`, `spans_read_and_write_weeks_months_and_years`, `weeks_need_hours_per_week_and_a_24_hour_lag_week_is_168_hours` (items 2, 4 and 5), `lag_days_use_the_calendar_lag_runs_on` (item 3), `resource_dependent_durations_have_no_day_to_count_in`, `resource_dependent_work_has_no_day_or_calendar_of_its_own` (item 6), `hours_per_day_stay_when_the_week_changes_until_set` (items 7 and 8), `a_date_becomes_a_holiday_and_goes_back`, `a_date_or_day_takes_shifts`, `calendar_change_refusals` (item 10), `a_calendar_is_added_as_a_copy_for_this_project` (item 11).

## Differences

- MS Project XML holds hours per day and per week and days per month for the whole project, where XER holds them per calendar ([MSPDI schema](sources.md#msp-xml-schema), Project element).
