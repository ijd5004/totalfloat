# TF-001: The rate in force

Which of a resource's or role's rates gives its prices and its limit at a moment.

## Statement

A resource or role holds rates over time. Each rate has five prices, a limit (the most units per hour it can work) and a start date, which can be absent. At a moment:

1. The rate in force is the one with the latest start date on or before that moment. Two rates with the same start date are told apart by ID, the higher one winning.
2. Before any dated rate has started, a rate with no start date is in force, as one that has always been. With several, the lowest ID is.
3. With no rate started and none undated, the earliest dated rate is in force, so work before a resource's first price is priced at it.
4. The order rates are listed in doesn't change the result.
5. A rate whose start date can't be read counts as one with no start date.
6. A limit given from a day on starts that day. Where no rate starts before that day, a rate holding the prices and limit in force before it is added, starting as item 7 says if that's before the day, or else the day before, so the new limit doesn't reach back (`set_resource_limit`, `set_role_limit`). As the earliest rate, it also covers any time before it (item 3). Of several rates starting that day, the limit goes on the one in force from it (item 1).
7. A rate TotalFloat makes with no day given starts at midnight on the project's planned start, or else its data date. With neither, it has no start date.

## Options

None.

## Sources

- `observed-file`: the corpus: nearly every resource holds one rate, and nearly every rate has a start date. No resource holds both a dated and an undated rate, so items 2 and 6 change no corpus result. Many first rates start on the first of a year.
- `own-choice`: items 1 to 7; the reasoning is under Rationale.

## Rationale

A rate says what holds from its start date on, so the latest one started is in force (item 1). A rate a file holds with no start date stands for what has always held, so a dated rate takes over from it on its own date and not before (item 2). A rate TotalFloat makes gets a start date (item 7), as the rates in real files have, and other software reading the file can expect: the project's start is the earliest day its work can need, and by item 3 the rate also covers anything before it, so the day changes no result in the project. A limit from a day keeps what held before on a rate of its own (item 6); without it, a limit set from January would also measure December. Item 3 gives work dated before every rate the earliest price, where none would leave it unpriced; it is the rule TotalFloat already used for prices ([TF-038](TF-038-units-and-costs-follow-durations.md), item 10). Item 4 keeps results deterministic whatever order a file lists rates in; IDs break ties.

## Tests

- `crates/model/src/resource.rs`: `the_rate_in_force_doesn_t_depend_on_order`, `an_undated_rate_holds_until_the_first_dated_one`.
- `crates/edit/src/tests.rs`: `a_dated_limit_takes_over_from_an_undated_one_on_its_day`, `a_first_dated_limit_doesn_t_reach_back`, `a_limit_on_the_earliest_rate_s_day_doesn_t_reach_back`, `a_limit_from_the_project_s_start_keeps_what_held_the_day_before`, `a_limit_on_a_day_two_rates_start_goes_on_the_one_in_force`, `a_limit_is_set_and_cleared_on_every_rate`, `a_new_resource_can_have_a_limit`, `a_new_assignment_takes_the_rate_in_force_when_its_work_starts`, `a_role_s_limit_is_set_and_cleared_on_every_rate`, `a_dated_role_limit_doesn_t_reach_back`, `a_new_role_can_have_a_limit`.
- `crates/cli/tests/resource_limit.rs`: `a_dated_limit_measures_the_weeks_after_it_against_the_new_one`.
- `crates/cli/tests/role_limit.rs`: `a_dated_role_limit_and_a_new_role_s_limit_are_new_rows`.

## Differences

None known.
