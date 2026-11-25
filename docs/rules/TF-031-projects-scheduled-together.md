# TF-031: Projects scheduled together

How TotalFloat calculates several linked projects at once, and which finish their float is measured to.

## Statement

1. `schedule` calculates the projects open together: every project in a file, or the ones chosen. Each project is calculated by TF-030, reading the other projects' latest results as the other ends of its links, and the round repeats until a round changes nothing.
2. Each project keeps its own data date and its own settings.
3. A linked activity with no stored dates gets a first set from a calculation of its project on its own logic, so the result doesn't depend on which project the rounds start from.
4. A loop of links through activities in different projects is refused, naming its activities.
5. **Float from the opened projects' finish.** With `float_from_project_finish` off, a project's backward pass starts from the latest calculated finish among the projects open together, when that is later than its own. Another project's finish is its calculated finish by TF-019 item 1, from the latest round, never its stored project finish. Where a project is read without being recalculated (item 7), that is the latest early finish stored on its activities, leaving out level-of-effort and WBS summary activities. That later finish is also the project's scheduled finish. The longest path still starts from the project's own calculated finish (TF-026). A project deadline still takes precedence (TF-019).
6. With `float_from_project_finish` on, each project's own calculated finish applies.
7. Commands that recalculate one project of a file for analysis (`explain`, `query`, `check`) treat every project in the file as open, and read the other projects' stored results in one calculation, without rounds. They agree with `schedule` on a file it wrote for all its projects, without `--ignore-external`. Where other projects' stored results are out of date, or `schedule` calculated only some of the projects, they can differ.

## Options

- **`float_from_project_finish`**: on (default) or off. An XER file selects it through `SCHEDOPTIONS.sched_use_project_end_date_for_float` (`Y` for on). Without one, on applies, because most corpus projects that hold the setting have it on. MS Project XML has no such setting.

## Sources

- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task elements, `PredecessorLink` with `CrossProject`: a link's predecessor can be part of another project, so projects can depend on each other. Item 1.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), TOTAL FLOAT: total float is measured against project completion, or a target that the planner names. Items 5 and 6.
- `observed-file`: one file in the corpus has projects that set `sched_use_project_end_date_for_float` to `N`. Their stored late dates count back from the latest finish among them, including a project whose own work ends much earlier. Files with several projects hold one data date per project. Items 2, 5 and 6.
- `own-choice`: items 1, 3, 4, 5 (the other projects' finish read from activity dates) and 7; the reasoning is under Rationale.

## Rationale

Links between projects make each one an input to the others, so calculating them in rounds until nothing changes gives the same dates as calculating them as one network would, without merging their settings (items 1 and 2). Without a loop through the links the rounds settle, and the starting point doesn't matter (item 3); a loop has no settled answer this way, so it is refused (item 4).

Measuring float to the latest finish of the projects open together shows how far work can slip before the last of them finishes (item 5). Reading the other projects' finishes from their activities' dates keeps an old stored project finish from holding the finish later forever. The longest path is a trace of this project's logic, so it starts from this project's own finish.

An analysis of one project should give the same dates `schedule` writes, so the analysis commands open the whole file. They read what the file stores for the other projects, which is what `schedule` leaves there (item 7).

## Tests

`crates/cli/tests/linked_projects.rs`: `a_project_with_no_stored_dates_is_calculated_with_the_project_it_links_to` (items 1 and 3), `a_project_not_chosen_still_stands_in_with_its_stored_dates` (item 1), `a_change_ripples_through_projects_in_both_directions` (item 1), `a_loop_through_links_between_projects_is_named` (item 4), `float_from_the_opened_projects_finish`, `the_written_file_holds_the_opened_projects_finish`, `as_late_as_possible_work_doesn_t_hold_the_opened_finish` (item 5). `crates/engine/src/tests.rs`: `float_from_the_opened_projects_finish` (items 5 and 6), `an_earlier_opened_project_leaves_the_finish_alone` (item 5). `crates/cli/src/mcp/tests.rs`: `schedule_measures_float_from_the_opened_projects_finish` (item 5).

## Differences

None known.
