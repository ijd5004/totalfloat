# TF-004: Units percent complete

How TotalFloat measures an activity's progress by its units, and what changes when a planner sets that percent or an assignment's units.

## Statement

1. **What counts.** An activity measured by units percent counts the units of its assignments to labor and nonlabor resources, which are hours. A material's units are in the material's own unit (tonnes, cubic metres), so adding them to hours would mean nothing. An assignment to a role with no resource has no resource type, and doesn't count either.
2. **The measure.** Units percent complete is the counted actual units, regular and overtime, over the counted actual plus remaining units: done over the units at completion. With no counted units there is nothing to measure, so the percent is empty, and finished work is 100%.
3. **Setting the percent.** On started, unfinished work measured by units, a percent given in a progress edit splits each counted assignment's units at completion by that percent: that share becomes actual, and the rest stays remaining. Each assignment keeps its own units at completion, so every counted assignment ends at the same percent, and so does the activity. Overtime units already recorded stay as they are and are part of the actual share; regular actual units are the rest of it. A remaining duration given in the same edit takes effect first, and the split is made at it: the duration type doesn't rescale units the percent has just set, while assignments the percent doesn't count follow the new duration as usual. A percent equal to the one the activity already has, to a hundredth, is a percent carried back unchanged (as text and progress tables do) and splits nothing.
4. **Units set directly.** An edit to an assignment's actual or remaining units changes the measure in item 2, so the activity's units percent follows. The percent stored on the activity follows too, to a hundredth, so carrying it back changes nothing (item 3). A resource on the same activity twice is refused, since nothing says which assignment is meant.
5. **What follows units.** An assignment's remaining units per hour follow its remaining units over the activity's remaining duration. A cost linked to its units splits at completion the same way as the units when the percent is set, and follows each unit at the cost per unit it had when units are set directly.
6. **What doesn't move.** Neither edit changes a duration. Remaining duration is the planner's to set; how units and durations drive each other by duration type is a separate rule ([TF-038](TF-038-units-and-costs-follow-durations.md)). Work not started keeps its units whatever percent is given, and has no actual units to set: an actual start comes first. Its remaining units set directly move its budgeted units with them, since work not started keeps the two equal ([TF-038](TF-038-units-and-costs-follow-durations.md), item 3). Finished work has no remaining units to set.
7. **Where files hold it.** In XER, the activity's type is `TASK.complete_pct_type` (`CP_Units`); an assignment's units are `TASKRSRC.act_reg_qty`, `act_ot_qty` and `remain_qty`, and its rate `remain_qty_per_hr`. The activity's `act_work_qty` and `remain_work_qty` (labor) and `act_equip_qty` and `remain_equip_qty` (nonlabor) are the sums over its labor and nonlabor assignments. They move by what those assignments' units move, so a difference a file already holds between a total and its sum is kept. No column stores the units percent itself, so it is worked out from the assignments when a file is read. In MS Project XML, an assignment's `ActualWork` and `RemainingWork` hold the same split, and a task's `PercentWorkComplete` is the same measure over its assignments' work.

## Options

None.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Appendix III, Table 8 (p. 170): units completed is one of the ways of measuring an activity's progress (item 2).
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task and Assignment elements: `PercentWorkComplete` is the share of a task's or an assignment's work that is done, and `ActualWork` and `RemainingWork` hold the work done and still to do (items 2 and 7).
- `observed-file`: the XER files in the corpus that fill the activity's unit totals. In most, the labor and nonlabor totals are the sums over the activity's labor and nonlabor assignments, with material left out (item 7). In a few, the nonlabor totals add material units too; TotalFloat moves a total only by what its labor or nonlabor assignments moved, so the difference a file holds is kept.
- `observed-file`: files in the corpus whose material resources name their own unit of measure (`RSRC.unit_id`), where labor and nonlabor count hours. Item 1.
- `own-choice`: items 1, 3 and 6; the reasoning is under Rationale.

## Rationale

- **Same percent on every assignment** (item 3). A planner who enters 40% on an activity reports that 40% of its work is done; with nothing to say which resource did more, the even reading is the one that adds no information the planner didn't give. A planner who knows better sets each assignment's units directly (item 4). TotalFloat's own design choice.
- **Material left out** (item 1). Summing hours and tonnes gives a number with no unit. Material progress is still recorded on its own assignment. TotalFloat's own design choice.
- **Durations stay** (item 6). Progress and forecast are separate questions: the share of work done says nothing about how long the rest takes. TotalFloat's own design choice.

## Tests

`crates/edit/src/tests.rs`: `a_units_percent_splits_each_assignment_s_units`, `a_units_percent_on_work_not_started_moves_no_units`, `a_units_percent_with_a_remaining_duration_keeps_its_split`, `units_set_on_an_assignment_move_the_units_percent`, `assignment_units_refusals`, `the_stored_percent_carried_back_splits_nothing`, `a_units_percent_leaves_blank_costs_blank`, `remaining_units_on_work_not_started_move_its_budget`, `a_resource_on_an_activity_twice_is_refused`; `crates/xer/src/patch/tests.rs`: `changing_actual_units_rewrites_that_cell_and_the_activity_s_actual_total`.

## Differences

None known.
