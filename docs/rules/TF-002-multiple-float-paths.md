# TF-002: Multiple float paths

How TotalFloat finds and numbers several float paths to an activity: the chains of work that would push it out first, then next, and so on.

## Statement

1. **The end activity.** Paths run back from one activity: the one asked for (`schedule --float-path-to`, or the file's own setting), otherwise the unfinished activity with the latest early finish that isn't a level of effort or WBS summary. An end activity asked for has to be unfinished, and can't be a level of effort or WBS summary. Ties go to the earlier position.
2. **Slack on a link.** A link's slack is how far its predecessor could slip before moving its successor, in work time on the predecessor's calendar. In relative float and free float, negative slack counts as none. Only links that apply in the calculation count, and a finished predecessor takes no part.
3. **Relative float.** An activity's relative float is the least slack summed along any route of links from it to the end activity. The end activity's is zero. An activity with no route to the end stays on no path, and so do finished work and level of effort and WBS summary activities, whose links take no part in the network.
4. **A path.** Each path starts at a head and walks back through predecessors that aren't on a path yet, have the same relative float, and have exactly no slack on the link. A link with negative slack, broken by a mandatory date, doesn't join a chain. Where several qualify, the later early finish goes first, then the earlier position. The walk stops where none does. Order within a path counts from its earliest member, so the head comes last.
5. **Path 1.** In both modes, path 1 starts at the end activity, so it is the chain of driving links into it.
6. **By total float.** Each later path's head is the activity not yet on a path with the lowest relative float. Path numbers rise with relative float.
7. **By free float.** Each later path's head is the activity not yet on a path with the least slack on a link into work already on a path: its free float to the paths found so far. A path that joins another a few hours out comes before a path that joins one much further out, even if its relative float is higher.
8. **Ties.** When heads tie, the lower relative float goes first, then the later early finish, then the earlier position. Position is the order of the activities' internal IDs (`TASK.task_id` in XER), lowest first.
9. **The limit.** At most the number of paths asked for (`schedule --float-paths N`, or the file's own maximum). The rest stay on no path.
10. **Results.** In XER, the results are `TASK.float_path` and `TASK.float_path_order`. `schedule --float-paths` writes the results and leaves the options below as they were.

## Options

The options are `ScheduleOptions.float_paths` in the model; a file selects them through `SCHEDOPTIONS` in XER, and `schedule --float-paths`, `--float-path-to` and `--float-paths-by` set them for one run.

- **`float_paths.enabled`**: off (default) or on. A file selects it through `enable_multiple_longest_path_calc`.
- **`float_paths.mode`**: `total_float` (default, items 6 and 3) or `free_float` (item 7). A file selects it through `use_total_float_multiple_longest_paths` (`Y` for total float); `--float-paths-by total-float|free-float` selects it for one run.
- **`float_paths.max_paths`**: the limit in item 9, ten by default. A file selects it through `max_multiple_longest_path`, which applies when `limit_multiple_longest_path_calc` is `Y`.
- **`float_paths.end_activity`**: the end activity in item 1; without one, the latest early finish applies. A file selects it through `key_activity_for_multiple_longest_paths`.

## Sources

- `standard`: [AACE 92R-17](sources.md#aace-92r-17), pp. 1–2 (public preview): near-critical activities are at risk of becoming critical if delayed, and the practice sets out identifying and tracking them.
- `standard`: [AACE 49R-06](sources.md#aace-49r-06), pp. 1–3 (public preview): different tools and options give different critical paths for the same logic, and activities on one chain can show different float because of calendars and constraints, which is why slack is counted per link (item 2).
- `observed-file`: the stored float paths in files in the corpus, all numbered by free float. The members of nearly every stored path share one relative float, consistent with items 3 and 4. The stored paths don't settle the order of paths or their ties, which items 6 to 8 choose.
- `own-choice`: items 3, 4, 6, 7 and 8; the reasoning is under Rationale.

## Rationale

- **Relative float as the measure** (items 3 and 6). Summing slack along the least-slack route measures how much an activity could slip before it moves the end activity, which is the question near-critical path analysis asks. TotalFloat's own design choice.
- **Free float to the paths found** (item 7). Free float to the network as a whole would rank by how much each activity could slip before moving its own successor, which says nothing about the end activity. Measuring it to the paths already found ranks each new path by how near it runs to the ones before. TotalFloat's own design choice.
- **Ties** (items 4 and 8). The later early finish is nearer the end activity in time, and position makes the order deterministic. TotalFloat's own design choice; the corpus doesn't settle ties.

## Tests

`crates/engine/src/tests.rs`: `paths_run_from_the_lowest_relative_float`, `paths_stop_at_the_limit_and_need_the_option`, `paths_end_at_the_chosen_activity`, `a_finished_activity_is_on_no_path`, `float_paths_by_total_float_rise_with_relative_float`, `float_paths_by_free_float_take_the_least_slack_to_a_path_found`, `float_paths_that_tie_go_to_the_later_finish_then_the_earlier_position`, `float_paths_start_at_the_end_activity_even_when_a_predecessor_ties_with_it`. `crates/cli/tests/cli.rs`: `schedule_finds_float_paths_asked_for`.

## Differences

None known.
