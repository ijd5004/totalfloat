# TF-039: Resource curves

How a resource curve shapes an assignment's units over time, and how units spread with no curve.

## Statement

1. **A curve.** A resource curve has 21 points, numbered 0 to 20, each a percent of an assignment's units.
2. **Twentieths.** Point `i`, for `i` from 1 to 20, is the share of the units in the `i`-th twentieth of the assignment's work. Point 0 is the start itself, and its share falls where work begins.
3. **Work time.** Twentieths are measured in work time on the assignment's calendar, and within a twentieth units follow work time. A period with no work, such as a holiday or a weekend, gets no units. A period boundary inside a twentieth splits that twentieth's share by the work on each side.
4. **In proportion.** The points are used in proportion to their sum, so a curve that doesn't sum to 100 still spreads all the units. A curve with a negative point, or with nothing in it, is refused.
5. **No curve.** With no curve, units spread evenly over work time. A flat curve, 5 at each of points 1 to 20, gives the same spread.
6. **The assignment's calendar.** An assignment's work runs on its resource's calendar on a resource-dependent activity, when the resource has one, and on the activity's calendar otherwise, as for scheduling ([TF-028](TF-028-resource-dependent-activities.md)).
7. **Which work.** Remaining units are shaped over the activity's remaining dates, or its early dates when it has no remaining dates. A curve shapes the remaining units over that span, from point 0 to point 20. Actual units spread evenly over the work from the actual start to the actual finish, or to the data date on work still in progress.
8. **Periods.** Spread units are given per day, per week starting on any weekday, or per month. A finish at midnight ends the day before. With no work between the dates, all the units fall in the first period. The periods' units sum to the assignment's units, to rounding.
9. **A curve the file lacks.** An assignment naming a curve the file doesn't hold spreads evenly, unless it holds a typed spread ([TF-040](TF-040-typed-spreads.md)).
10. **Where files hold it.** In XER, `RSRCCURVDATA` holds each curve (`curv_id`, `curv_name`, `default_flag` and `pct_usage_0` to `pct_usage_20`), and `TASKRSRC.curv_id` names an assignment's curve. An empty cell means no curve. In MS Project XML, an assignment's `WorkContour` names its shape.

## Options

None.

## Sources

- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Assignment element, `WorkContour`: an assignment says how its work is distributed across its duration. Items 1 and 10.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §6.2, pp. 81–85: tools compute resource loading from duration, total quantity and time phasing against a task or resource calendar, and their rules differ. Items 3, 6 and 8.
- `observed-file`: the files in the corpus that hold resource curves. Every curve puts nothing at point 0 and sums to 100. Curves for an even spread hold equal points from 1 to 20, which is even only when each point covers one twentieth (item 2). A bell-shaped curve is symmetric over points 1 to 20, and front-loaded and back-loaded curves mirror each other over them, which holds only when point 0 stands apart. Items 2, 4 and 5.
- `own-choice`: items 2 (where point 0's share falls), 3, 4 and 6 to 9; the reasoning is under Rationale.

## Rationale

- **Twentieths** (item 2). The corpus curves only make sense with each point from 1 to 20 covering one twentieth: an even curve would otherwise not be even, and a symmetric bell would be lopsided. No corpus curve puts anything at point 0, so where a nonzero point 0 falls is untested. Putting its share where work begins keeps every unit inside the assignment's span.
- **Work time** (item 3). Units are work, and work happens in working time. Under this reading the flat curve gives exactly the even spread of item 5, so a planner who picks the flat curve sees no change.
- **In proportion** (item 4). A curve typed by hand may not sum exactly to 100. Spreading in proportion keeps the assignment's units, which the curve only shapes.
- **The calendar** (item 6). Resource-dependent work is scheduled on its resources' calendars, so its units fall in their working time. Other work follows its activity.
- **Remaining span** (item 7). The remaining dates are when the rest of the work happens, so the rest of the units fall between them. A curve describes the shape of work to be done, so it shapes what's left. Without a record of when actual work fell, the even spread is the one that adds nothing the file doesn't say.
- **No work** (item 8). Units between dates with no working time have to go somewhere to keep the total, and the first period is where the work was due to begin.
- **Missing curves** (item 9). A curve the file doesn't hold can't shape anything, and the even spread is what no curve means.

## Tests

- `crates/engine/src/spread_tests.rs`: `each_built_in_curve_over_a_week_with_a_holiday` (items 2 and 3), `no_curve_spreads_evenly_over_work_time`, `a_flat_curve_equals_the_even_spread` (item 5), `a_twentieth_split_by_a_day_divides_by_work_time` (item 3), `point_zero_falls_where_work_begins` (item 2), `weeks_begin_on_the_day_asked_for`, `months_split_at_the_first`, `a_finish_at_midnight_ends_the_day_before`, `no_work_puts_everything_in_the_first_period`, `spread_units_sum_to_the_units` (item 8), `bad_inputs_are_refused` (item 4), `an_assignment_works_on_its_resource_calendar_only_when_resource_dependent` (item 6), `an_assignments_remaining_distribution` (item 9).
- `crates/xer/tests/curves.rs`: `curves_and_an_assignment_s_curve_are_read`, `a_curve_the_file_lacks_is_cleared_with_a_warning_and_kept_in_the_file`, `a_curve_missing_a_point_is_left_out_and_reported` (items 9 and 10).

## Differences

None known.
