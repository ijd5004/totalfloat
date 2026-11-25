# TF-007: Choosing an assignment's spread

How an edit sets the way an assignment's units spread over time: evenly, by a resource curve, or by units typed for each period.

## Statement

1. An assignment's units spread in one of three ways: evenly over its work, by one of the file's resource curves, or by units typed for each period. Choosing one clears the others.
2. A curve is chosen by its name, ignoring case. A name no curve has, or one that two curves share, is refused. Choosing no curve spreads evenly.
3. A curve or an even spread is refused on an assignment whose actual units were typed by period, because the choice would drop them.
4. Typed units are given for consecutive days, weeks or months from a first day. A month period starts on the first of a month.
5. Each period's units go on the assignment's remaining work inside that period. That work lies between the activity's remaining start and finish, or its early start and finish when it has no remaining dates. It is counted on the calendar the assignment's work runs on. The stored spread is those stretches of work time, each with its units.
6. The typed spreads are refused when:
   - a period lies wholly outside the activity's remaining dates;
   - a period with no work holds units;
   - the periods leave some of the remaining work uncovered;
   - their units differ from the assignment's remaining units by more than 0.0005 for each stored stretch. The refusal names the difference, because the edit never changes units.
7. On work not yet started, the planned spread becomes the same as the typed remaining spread. On started work, the planned and actual spreads stay as they were, and a planned spread the assignment didn't have stays absent.
8. A finished activity has no remaining units, so typing units for it is refused. A curve or an even spread can still be chosen.

## Options

None.

## Sources

- `observed-file`: the files in the corpus that hold typed spreads. Every typed spread on work not started holds a planned spread equal to its remaining one, and some started assignments still hold them equal. Item 7.
- `own-choice`: items 1 to 6 and 8; the reasoning is under Rationale.

## Rationale

The three ways are alternatives: keeping a stale curve beside typed units, or stale units beside a curve, would leave the spread ambiguous (item 1). A name two curves share can't say which one is meant, so it's refused (item 2). Dropping actuals that were typed by period would lose the record of when the work was done, so a whole-assignment shape isn't put over them (item 3).

Planners think of loading in days, weeks and months, while a file holds a spread as stretches of work time ([TF-040](TF-040-typed-spreads.md)). Turning each period into the work inside it on the assignment's own calendar gives a spread that `totalfloat_engine::spread` and `query --usage` give back period by period (item 5). The early dates are the remaining work's dates on started work. So falling back to them gives the same span that `query --usage` spreads over (item 5).

An edit that changed an assignment's units as a side effect would surprise its user, so units that don't add up are refused, with the difference named. The tolerance is the reader's, so a spread an edit accepts reads back without a warning (item 6).

Started work's planned spread describes the budget, and the remaining units aren't the budget, so the plan is left alone (item 7).

## Tests

- `crates/edit/src/tests.rs`: `an_assignment_s_curve_is_chosen_or_cleared` (items 1 and 2), `an_unknown_curve_or_assignment_is_refused` (item 2), `units_typed_per_period_become_the_typed_spread` (items 4, 5 and 7), `typing_started_work_keeps_its_plan_and_actuals` (items 3 and 7), `spreads_that_don_t_fit_are_refused` (items 4, 6 and 8), `a_spread_without_remaining_dates_takes_the_early_dates` (item 5).
- `crates/cli/tests/spreads.rs`: `edit_chooses_a_curve_an_even_spread_or_typed_units` (items 1, 5 and 7, through the file), `query_usage_follows_the_chosen_spread` (item 5), `spreads_that_can_t_be_chosen_are_refused` (items 2 and 6).

## Differences

None known.
