# TF-026: Driving relationships, the longest path and critical activities

Which links drive their successors, which activities are on the longest path, and which are critical.

## Statement

1. **A driving relationship** is a link with no slack: no work time between the date the link allows and the date it controls. Slack is counted on the predecessor's calendar, whatever calendar the lag runs on. A link that progress override drops (TF-022) doesn't drive, and neither does a link with negative slack, broken by a mandatory date.
2. **The longest path** starts from every activity whose early finish is the project's calculated finish as a moment on the clock (TF-019), and runs back through driving links. Ties all count, so an activity can have several driving predecessors on the path.
3. The path uses the final early dates, including as-late-as-possible moves (TF-017).
4. The path runs through finished work: finished activities stay on it whether or not they still hold anything back. Finished work can start the path only when logic pushed it past the data date (TF-022); being moved to working time doesn't count.
5. The longest path isn't defined by float. With a deadline after the calculated finish, the whole path has positive total float.
6. Level-of-effort and WBS summary activities take no part in driving links. TF-027 says when one is on the longest path.
7. **Critical.** An activity is critical under the project's definition:
   - **by total float:** its total float is at or below the threshold, and it isn't finished;
   - **by longest path:** it is on the longest path.
8. **Open ends.** With `make_open_ended_critical` on, an unfinished activity that no link limits in the backward pass, with no successor in another project and no external late finish, is an open end. The backward pass starts its late finish from its own early finish, or from the project deadline when that is earlier, and its own constraints then apply as usual (TF-016). With no constraint it has zero total float, or negative float from the deadline. It is critical under either definition, and its free float runs to its late finish.
9. `driving_relationships` lists the links with no slack, for tracing logic; it can name a link from finished work even where the data date set the successor's date.

## Options

- **`critical`**: `total_float_at_most(threshold)` (default, with a threshold of zero) or `longest_path`. An XER file selects it through `PROJECT.critical_path_type` (`CT_TotFloat` or `CT_DrivPath`) and the threshold through `PROJECT.critical_drtn_hr_cnt`. Without one, total float at or below zero applies, because nearly every corpus project uses the total-float definition and most with a zero threshold. MS Project XML holds a threshold in `CriticalSlackLimit` and has no longest-path definition.
- **`make_open_ended_critical`**: off (default) or on. An XER file selects it through `SCHEDOPTIONS.sched_open_critical_flag`. Without one, off applies, because every corpus project that holds the setting has it off. MS Project XML's nearest setting is `MultipleCriticalPaths`; see Differences.

## Sources

- `standard`: [AACE 49R-06](sources.md#aace-49r-06), pp. 1–3: no standard fixes how the critical path is calculated; the common definitions include lowest total float and the longest path, and activities on one chain can show different float because of calendars and constraints. Items 1, 2, 5 and 7, and the `critical` option.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), LONGEST PATH: the longest path is the chain of activities that sets the project's duration. Items 2 and 5.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 6, pp. 75–89: the critical path is the longest path to the finish, and the activities with the least float aren't always the ones driving it. Items 2, 5 and 7.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §10.1, p. 138: critical and driving path analysis. Items 1 and 2.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §8.1 Horizontal Traceability, p. 101: an activity with no predecessor or no successor has open-ended, or dangling, logic, so risk on it doesn't flow on to other work. Item 8.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Project elements: `CriticalSlackLimit` is how much slack a task can have and still be critical, and `MultipleCriticalPaths` asks for a critical path for each independent network of tasks. Items 7 and 8.
- `observed-file`: files in the corpus that store longest-path flags. Flagged activities follow links with no slack counted on the predecessor's calendar, start from the activities finishing at the project finish, include finished activities, and include activities with positive total float where the project has a later deadline. Every corpus project that holds `sched_open_critical_flag` has it off, so item 8 has no corpus case. Items 1 to 5.
- `own-choice`: items 4 (finished work starting the path), 8 and 9; the reasoning is under Rationale.

## Rationale

Slack on a link is how far the predecessor could slip before moving its successor, so it is counted in the predecessor's own work time; counting it on a 24-hour lag calendar would make an overnight gap look like slack (item 1). The longest path is a trace of logic, so it keeps finished work, which logic still passes through (item 4). Finished work that sits on the data date didn't set the finish, so it can't start a path; finished work that logic pushed later did.

The two definitions of critical answer different questions, and planners choose between them, so the project's setting chooses ([ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules)). Finished work has no float left to measure, so it is critical only by being on the longest path (item 7).

An open end is an activity whose finish nothing later depends on. Giving it its own early finish as its late finish makes each chain that ends there critical in its own right, the same idea as a critical path for each independent network (item 8). Where the project has a deadline earlier than that finish, the deadline still binds.

## Tests

`crates/engine/src/tests.rs`: `a_link_with_slack_on_the_lag_calendar_does_not_drive`, `an_overnight_gap_is_not_slack_on_a_24_hour_lag_calendar`, `a_link_broken_by_a_mandatory_date_doesnt_drive` (item 1), `the_longest_path_runs_back_through_driving_links`, `only_reaching_the_finish_on_the_clock_starts_the_path`, `equally_driving_predecessors_are_all_on_the_path` (item 2), `the_path_runs_through_finished_work_but_not_a_broken_link` (items 1 and 4), `a_longest_path_can_carry_float_and_a_deadline_can_leave_it` (item 5), `critical_uses_the_float_threshold` (item 7), `open_ended_work_can_be_made_critical`, `an_open_end_keeps_the_deadline_and_its_own_provenance`, `a_dropped_link_leaves_its_predecessor_an_open_end` (item 8). `crates/engine/src/progress_tests.rs`: `moving_to_working_time_is_not_a_push` (item 4).

## Differences

- MS Project's `MultipleCriticalPaths` gives each independent network of tasks its own critical path ([MSPDI schema](sources.md#msp-xml-schema), Project elements). TotalFloat's open-end option works per activity with no successor, so an activity that ends a branch of a connected network is an open end too.
