# TF-038: Units and costs follow durations

How an assignment's units, units per hour and costs relate to its activity's durations, and what a duration edit or a new assignment does to them.

## Statement

1. **Units and hours.** An assignment's budgeted units are its budgeted units per hour times the activity's original duration in hours. Its remaining units are its remaining units per hour times the activity's remaining duration in hours.
2. **Linked costs.** When an assignment's cost follows its units, each of its budgeted, actual and remaining costs moves with the matching units at a cost per unit. A new assignment uses the resource's price (item 10). A cost that already exists keeps the cost per unit it implies (item 5), which can differ from the stored price.
3. **Work not started.** On work not started whose remaining duration equals its original duration, remaining units, units per hour and cost equal the budgeted ones.
4. **A duration edit, by duration type.** When an activity's original or remaining duration changes, the matching units follow by the activity's duration type:
   - fixed units per time, and fixed duration and units per time, keep the units per hour and scale the units with the duration;
   - fixed units, and fixed duration and units, keep the units and work out the units per hour again.
   Scaling the units keeps a stored units per hour exact even where the file rounded it.
5. **Costs follow at the file's price.** A linked cost scales with its units, at the cost per unit the stored cost already gives. A price changed since the cost was worked out doesn't move it.
6. **Actuals stay.** A duration change never changes actual units or costs. A units percent given in a progress edit splits units at completion into actual and remaining by [TF-004](TF-004-units-percent-complete.md).
7. **No time left.** A remaining duration of zero leaves no remaining units, and the units per hour stay for later.
8. **Rounding.** Units and units per hour are rounded to six decimal places, and costs to four.
9. **Budget and forecast at completion.** On started work, edits don't move budgeted units to match actual plus remaining units: the budget is the plan, and actual plus remaining is the forecast at completion. On work not started the two agree (item 3), so remaining units set there move the budgeted units with them ([TF-004](TF-004-units-percent-complete.md), item 6).
10. **A new assignment.** A new assignment takes the units given, or else the resource's default units per hour over the activity's durations. Its price is the first price of the resource's rate in force on the activity's planned start, or else its early start, or else the data date ([TF-001](TF-001-rate-in-force.md)). Its cost follows its units when the resource's does, and is units times that price. It takes the activity's dates when it's made, which aren't kept current afterwards.
11. **Milestones.** A milestone takes no resources.
12. **Where files hold it.** In XER: `TASK.duration_type`, `target_drtn_hr_cnt`, `remain_drtn_hr_cnt`; `TASKRSRC.target_qty`, `target_qty_per_hr`, `remain_qty`, `remain_qty_per_hr`, `act_reg_qty`, `act_ot_qty`, `target_cost`, `remain_cost`, `act_reg_cost`, `act_ot_cost`, `cost_per_qty`, `cost_qty_link_flag` and `rate_type`; `RSRCRATE.cost_per_qty`. In MS Project XML, an assignment's work, units and costs ([TF-003](TF-003-msproject-xml-assignments.md)).

## Options

None. Each activity's duration type selects item 4's behavior, and each assignment's cost link selects item 2.

## Sources

- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §6.2, pp. 81–85: tools compute resource loading from duration, total quantity and time phasing, and their rules differ, so the rules must be stated. Items 1 and 4.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 3, pp. 49–61: duration estimates should be consistent with the effort and resources assigned. Items 1 and 4.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task element `Type` and `EffortDriven`, and [Project VBA reference](sources.md#msp-vba-reference), `Application.OptionsSchedule` `TaskType`: another scheduler names the same choice, which of duration, units and rate stays when another changes. Item 4.
- `observed-file`: the assignments in the XER files of the corpus. Remaining and budgeted units equal units per hour times the remaining and original hours on nearly every assignment; linked costs equal units times price on most; on work not started, remaining units and cost equal the budgeted ones; and budgeted units equal actual plus remaining units on most. Items 1 to 3 and 9.
- `observed-file`: the exceptions to the linked costs in files in the corpus. Some finished labor assignments hold costs while their stored price is zero, and some budgets on work not started aren't their units times the stored price. So a stored cost can imply a price other than the one the file holds; why it differs isn't recorded in the file. Items 2, 5 and 6.
- `observed-file`: the XER files in the corpus. Every project's default rate type is the first price, and almost no assignment sits on a milestone. Items 10 and 11.
- `own-choice`: items 4 to 11; the reasoning is under Rationale.

## Rationale

- **Duration types** (item 4). A duration type says which of duration, units and rate the planner fixed. When the duration changes, the fixed one stays and the other gives way. Scaling the units by the ratio of the durations gives the same answer as rate times hours when the stored values agree, and doesn't add the rounding of a stored rate.
- **The file's price** (item 5). The corpus shows stored costs that rest on prices the file no longer holds. Rescaling at the cost per unit already stored keeps the planner's pricing. Repricing at today's price would change costs the planner didn't touch.
- **Actuals** (item 6). An actual is a record of what happened, so a forecast change can't move it.
- **No time left** (item 7). No remaining time means no remaining work. Keeping the rate lets a later remaining duration bring the units back at the same rate.
- **Rounding** (item 8). Six decimals for units and four for costs are what the corpus's stored values use, so written values look like the rest of the file.
- **Budget and forecast** (item 9). The budget records what was planned, so a forecast change shouldn't rewrite it. Comparing the two is how a planner sees an overrun.
- **New assignments** (item 10). The rate in force when the work starts prices the work. The first price is the one every corpus project uses by default.
- **Milestones** (item 11). A milestone has no duration, so it has no time for units to be spread over.

## Tests

- `crates/edit/src/units.rs`: `units_per_time_types_keep_the_rate_and_move_the_units`, `units_types_keep_the_units_and_move_the_rate` (item 4), `units_scale_with_the_duration_even_when_the_rate_was_rounded` (items 4 and 8), `a_remaining_duration_of_its_own_moves_only_what_remains` (items 1 and 3), `finishing_leaves_no_remaining_units` (items 6 and 7), `a_cost_keeps_the_price_the_file_used`, `an_unlinked_cost_stays` (items 2 and 5), `a_new_assignment_spreads_its_units_or_the_default_rate` (item 10).
- `crates/edit/src/tests.rs`: `a_new_assignment_takes_the_rate_in_force_when_its_work_starts`, `a_new_assignment_takes_the_resource_s_cost_link` (item 10), `work_with_resources_can_t_become_a_milestone_or_be_staffed_once_finished` (item 11).
- `crates/cli/tests/corpus_units.rs`: `a_duration_edit_and_its_reverse_leave_units_and_costs_on_every_corpus_file` (items 4 and 5, on the corpus).

## Differences

None known.
