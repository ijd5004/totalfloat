# TF-042: Activity steps

How an activity's steps measure its physical percent complete.

## Statement

1. **Steps.** An activity's work can be broken into steps, each with a name, a weight, a percent complete and a completed mark. A step marked complete counts as 100 percent. A step's percent is 0 to 100 and its weight 0 or more.
2. **The weighted percent.** The steps' percent complete is the sum of each step's weight times its percent, over the sum of the weights. Steps with no weight add nothing. With no weight at all there's nothing to measure, and there's no steps' percent.
3. **When steps drive progress.** Where the project measures percent complete by steps and the activity's percent complete type is physical, a step edit sets the activity's physical percent complete to the steps' percent, rounded to a hundredth. Where the project doesn't, or the activity's type is duration or units, steps are kept and shown but don't change its percent complete.
4. **Step edits.** An edit adds, changes, completes or deletes a step. A new step has weight 1 unless given one. Setting a step's percent to 100 marks it complete, and completing a step sets its percent to 100.
5. **Shown.** `query --columns steps_percent` shows the steps' percent for any activity with weighted steps.
6. **Where files hold it.** In XER: `TASKPROC` (`proc_name`, `seq_num`, `proc_wt`, `complete_pct`, `complete_flag`), the project's setting `PROJECT.step_complete_flag`, and the activity's type `TASK.complete_pct_type` (`CP_Phys`) with its percent in `phys_complete_pct`.

## Options

- **`percent_from_steps`**: off (default) or on, per project. A file selects it through `PROJECT.step_complete_flag` (`Y` for on). Without one, steps don't drive progress, because a project that doesn't say so hasn't chosen it.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Appendix III, Table 8, p. 170: weighted milestones are a way of measuring progress, each part of the work earning its share of the whole when done. Items 1 and 2.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task element, `PhysicalPercentComplete`: physical percent complete is the share of the work done, entered apart from duration percent and usable for earned value. Item 3.
- `observed-file`: the one file in the corpus that holds steps, in a project that doesn't set `step_complete_flag`. None of its steps has progress, so it can't test the weighting. Item 6.
- `own-choice`: items 2 to 4 and the option; the reasoning is under Rationale.

## Rationale

A step is a measurable part of an activity's work, and its weight says how much of the activity it is. A weighted average is the measure that gives each step its share, the way weighted milestones give each milestone its share of a work package (item 2). A step with no weight has no share to give.

Steps measure work done, so they drive the percent that measures work done, physical percent, and only where the project has chosen to measure by them (item 3). The project setting's name and place say that it chooses this, and no corpus file has steps part done to show more. Duration and units percent come from their own measures ([TF-023](TF-023-duration-and-physical-percent-complete.md), [TF-004](TF-004-units-percent-complete.md)), so steps leave them alone. Rounding to a hundredth matches how percent complete is stored.

A weight of 1 for a new step gives equal steps when the planner gives none (item 4).

## Tests

- `crates/edit/src/tests.rs`: `steps_are_added_progressed_and_drive_physical_percent` (items 1 to 4), `steps_leave_physical_percent_alone_unless_the_project_measures_by_them` (item 3), `step_refusals` (items 1 and 4).
- `crates/xer/tests/steps.rs`: `steps_are_read_and_give_the_physical_percent` (items 2 and 6), `a_step_s_progress_changes_its_row_only`, `new_and_deleted_steps_are_rows_added_and_removed` (item 6).

## Differences

None known.
