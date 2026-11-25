# TF-030: Links to other projects and external dates

How a relationship to an activity in another project, or an external date standing in for one, bounds a project calculated on its own.

## Statement

1. A project is calculated on its own network. An activity in another project stays outside it: its dates are fixed inputs, read from what is stored for it.
2. **External early start.** An activity's external early start bounds it, while it hasn't started, like a start-on-or-after constraint (TF-016): the later bound wins.
3. **External late finish.** An activity's external late finish bounds unfinished work in the backward pass like a finish-on-or-before constraint.
4. **A link in** from an activity in another project in the file bounds the local activity, while it hasn't started, through the other activity's stored early date plus the lag. Where the other activity's end has happened, the link counts the whole lag from that actual date. Started and finished local work reads no link in.
5. **A link out** to an activity in another project bounds the local activity's late dates through the other activity's stored late date less the lag. A started successor is read through its remaining late dates.
6. The lag runs on the calendar the project's lag setting names (TF-006), which can be the other activity's calendar. Lag on a link out from a local activity that has started or finished counts down as TF-021 gives.
7. In the backward pass, links out follow the local link rules: a finished successor limits nothing, progress override drops a broken link (TF-022), and a link measured from a started activity's actual start doesn't limit it (TF-021).
8. Finished work reads no external date and no link to another project.
9. Free float counts a successor in another project, read through its stored early date less the lag. An activity with a successor in another project, or with an external late finish, isn't an open end (TF-026).
10. Links to other projects take no part in tracing driving links within the project. A date they set records the link as its cause, and the trace ends there.
11. These are refused, because nothing settles them:
    - a link whose other end isn't in the file, with no external date to stand in for it;
    - a link whose other end has no stored dates, when the projects aren't scheduled together (TF-031);
    - a level-of-effort or WBS summary activity linked to another project;
    - an as-late-as-possible activity with a successor in another project;
    - an expected finish (TF-018) on an activity with a finish-side link from another project.
12. With `ignore_external_relationships` on, links to other projects and external dates are both left out, and each project is calculated on its own logic.

## Options

- **`ignore_external_relationships`**: off (default) or on. `schedule --ignore-external`, and the MCP `schedule` tool's `ignore_external`, set it for one run. No file field selects it: the XER field that may hold it isn't confirmed, so TotalFloat neither reads nor writes it. MS Project XML has no such setting.

## Sources

- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task elements, `ExternalTask` and `ExternalTaskProject`, and `PredecessorLink` with `CrossProject` and `CrossProjectName`: a file can hold a task that stands for one in another project, and a link to a task in another project. Items 1, 4 and 5.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), "Date Constraints", pp. 36–37: constraints are generally used to show an external event's effect on the schedule, as a start-no-earlier or finish-no-later bound. Items 2 and 3.
- `observed-file`: files in the corpus with external dates, and with links into an activity from another project in the same file. The stored dates are within the external dates as bounds, and a linked activity's stored dates follow the other end's stored dates plus the lag. Items 1 to 4.
- `own-choice`: items 5 to 12; the reasoning is under Rationale.

## Rationale

Another project's network isn't part of this calculation, so its stored dates are what the file says about it, and a link to it acts as a bound, the way GAO describes constraints standing for external events (items 1 to 3). Reading links in through early dates and links out through late dates keeps each pass reading the same pass of the other project (items 4 and 5). Links out follow the same progress, lag and calendar rules as local links (items 6 and 7). Links in bound only work not started, the only case the corpus shows, and count the lag from the other end's actual date in full, since that project's data date isn't this one's to count it down to (item 4).

The corpus has only a few such links, so every pattern it doesn't show is refused instead of guessed (item 11). Ignoring other projects is a planner's what-if, a project calculated on its own logic, and it stays an option of the run until the file field that holds it is known (item 12).

## Tests

`crates/engine/src/tests.rs`: `other_projects_stay_out_of_the_results` (item 1), `an_external_early_start_holds_back_work_not_started` (item 2), `an_external_late_finish_limits_late_dates` (item 3), `a_link_from_another_project_reads_its_stored_dates`, `a_link_from_finished_work_counts_from_its_actual_date`, `an_external_date_stands_in_for_a_project_not_loaded` (item 4), `a_link_to_another_project_limits_late_dates_through_its_stored_dates` (item 5), `lag_from_another_project_uses_its_resource_calendar` (item 6), `a_finished_successor_in_another_project_limits_nothing` (item 7), `finished_work_reads_no_successor_in_another_project` (item 8), `free_float_counts_a_successor_in_another_project`, `a_successor_with_no_early_start_leaves_free_float_alone`, `a_successor_in_another_project_is_not_an_open_end` (item 9), `links_the_engine_cannot_honor_are_refused` (item 11), `finished_work_linked_to_a_project_not_loaded_can_be_scheduled` (item 8). `crates/cli/tests/ignore_external.rs`: `a_link_from_another_project_is_ignored_only_when_asked`, `an_external_early_start_is_ignored_only_when_asked`, `a_link_to_a_project_not_in_the_file_can_be_ignored` (item 12).

## Differences

None known.
