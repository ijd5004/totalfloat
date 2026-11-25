# TF-034: Edits to activities, links and WBS nodes

What an edit to an activity's durations, constraints, type, name or links does, how new activities and WBS nodes are made, and what deleting removes.

## Statement

1. **Durations on work not started.** Setting the original duration also sets the remaining duration, unless a remaining duration is given in the same edit.
2. **Durations on work in progress.** Original and remaining duration each change only when given, so a change to the plan leaves the work left alone. Finished work has no remaining duration to set.
3. **Constraint pairs.** An activity takes the constraint pairs the scheduling engine accepts. A secondary constraint needs a primary, so clearing a primary while a secondary is in place is refused, and the secondary isn't promoted to take its place.
4. **Constraints that bind nothing.** A constraint on a level of effort or WBS summary activity, or on finished work, binds nothing, so it's refused.
5. **Milestones.** A milestone has no duration. Turning an activity into a milestone sets its duration to zero and says so, and turning a milestone into a task needs a duration. Work with resources can't become a milestone.
6. **Who changes type.** Only work not started changes type. Level of effort and WBS summary activities can't change type by edit, and other activities can't become one.
7. **New activities.** A new activity takes an internal ID above every activity row in the file, and a GUID derived from the file and that ID. Its flags and quantities take the values nearly every row in real files holds: not locked, no feedback, normal priority, zero quantities. With no calendar given, it takes the project's default calendar.
8. **Activity IDs and names.** An activity ID holds up to 40 characters. A name holds up to 120 characters, and a WBS node's name up to 100, on one line.
9. **Links between projects.** A link between activities in two projects is one row, held by both projects. A lag typed in days, weeks, months or years counts by the successor's project's lag calendar ([TF-006](TF-006-lag-calendar.md)), because the row belongs to the successor's project. An XER file stores the lag in hours (`TASKPRED.lag_hr_cnt`), with no record of the unit it was typed in.
10. **Deleting.** Deleting an activity deletes its links, assignments, expenses and every other row that names it, and the report names any activity left with no predecessor or no successor. An activity linked to another project can't be deleted until that link is removed. The activity the project's float-path setting names as its end ([TF-002](TF-002-multiple-float-paths.md), `float_paths.end_activity`) can't be deleted.
11. **WBS nodes.** A new node goes last among its siblings and copies the settings of the project's root node, such as its responsible OBS node and earned value settings. Codes are unique among siblings. The project's own root node can't be changed by these edits.
12. **Where files hold it.** In XER: `TASK` (`target_drtn_hr_cnt`, `remain_drtn_hr_cnt`, `cstr_type`, `cstr_date`, `cstr_type2`, `cstr_date2`, `task_type`, `task_code`, `task_name`, `guid`), `TASKPRED` (with `proj_id` and `pred_proj_id`) and `PROJWBS`.

## Options

None.

## Sources

- `standard`: [DoD EVMSIG](sources.md#dod-evmsig), §7.2 Definitions, MILESTONE, p. 84: a milestone is a zero-duration event. Item 5.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), "Date Constraints", pp. 36–41: hard and soft constraints, and the pass each acts on. Items 3 and 4.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §5.7 and §5.8, pp. 65–67: summaries and level of effort take their dates from the work they span. Items 4 and 6.
- `observed-file`: the XER files in the corpus. Nearly every activity row holds the flag and quantity values in item 7. Activity IDs and names stay within the lengths item 8 allows. Items 7 and 8.
- `observed-file`: the one file in the corpus that holds links between projects. Each such link row is held under its successor's project (`TASKPRED.proj_id`). Item 9.
- `own-choice`: items 1 to 4, 6, 8, 9 (which project's lag calendar counts) and 10 to 11; the reasoning is under Rationale.

## Rationale

- **Durations** (items 1 and 2). Before work starts, the plan and the forecast are the same estimate, so changing one changes both. Once work has started, the remaining duration is the planner's forecast of the rest, and a change to the original plan shouldn't overwrite it.
- **Constraints** (items 3 and 4). Refusing what the engine would refuse at the edit says what's wrong where it was typed. Promoting a secondary constraint would turn a planner's second condition into their first without being asked. A constraint that can't bind anything would only mislead the next reader.
- **Types** (items 5 and 6). A milestone is an event, so it has no duration to keep. A level of effort or WBS summary takes its dates from other work, and its links don't take part in loop checks or links to other projects. Changing an activity into or out of one would change what its links mean. Started work has a record under its type, such as actual dates on a task, which a change of type would leave meaning something else.
- **New rows** (item 7). A new row should look like the rows around it to any software that reads the file, so it takes the values real files hold. A GUID derived from the file and the ID is the same each time the same edit runs, which keeps output deterministic.
- **Lengths** (item 8). 40 characters is twice the longest activity ID in the corpus, and 120 and 100 are the longest activity and WBS names there. Refusing longer values keeps files within what real files hold. A line break in a name would break text output and tables.
- **Links between projects** (item 9). A lag typed in days needs one calendar to count on, and the project that holds the row is the one whose setting applies.
- **Deleting** (item 10). A link or assignment without its activity means nothing, and naming the open ends left behind lets the planner fix the logic. A link to another project is held by both projects, so removing it is an edit to both, which the planner makes on purpose first. A float-path setting needs the end activity it names.
- **WBS** (item 11). Copying the root's settings gives a new node the project's own defaults. The root node stands for the project itself, so it changes with the project.

## Tests

`crates/edit/src/tests.rs`: `remaining_follows_original_on_work_not_started`, `both_durations_can_be_given`, `original_changes_alone_on_work_in_progress`, `duration_refusals` (items 1 and 2), `a_constraint_is_set_and_cleared`, `a_primary_with_a_secondary_stays_until_the_secondary_goes`, `constraint_pairs_the_engine_refuses_are_refused_here`, `a_constraint_that_would_do_nothing_is_refused`, `constraint_refusals` (items 3 and 4), `a_milestone_has_no_duration`, `a_milestone_becomes_a_task_only_with_a_duration`, `work_with_resources_can_t_become_a_milestone_or_be_staffed_once_finished`, `spans_keep_their_type` (items 5 and 6), `an_activity_is_added_above_every_id_and_can_be_linked_at_once`, `an_added_activity_falls_back_on_the_project_calendar`, `a_new_id_avoids_what_flags_use_as_separators`, `add_refusals` (item 7), `a_name_fits_what_files_hold`, `names_stay_on_one_line` (item 8), `a_link_between_projects_is_in_both`, `a_lag_between_projects_counts_days_by_the_successor_s_option`, `a_link_between_projects_changes_and_goes_from_both` (item 9), `a_link_to_another_project_stays` (items 9 and 10), `deleting_says_what_it_opens`, `deleting_finished_work_says_its_actuals_are_gone`, `deleting_an_activity_takes_its_expenses`, `delete_refusals` (item 10), `a_wbs_node_is_added_renamed_and_moved`, `wbs_refusals`, `a_repeated_code_is_refused_with_the_ids_to_use` (item 11).

## Differences

None known.
