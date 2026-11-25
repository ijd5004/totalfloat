# TF-027: Level-of-effort and WBS summary activities

How TotalFloat dates the activities that measure other work instead of doing their own.

## Statement

1. Level-of-effort and WBS summary activities are spans. Their links take no part in the forward and backward passes, so a span never moves another activity, never drives one (TF-026), and never forms a loop in the network. Spans are measured after each pass from the dates the network produced, level-of-effort activities first, since a WBS summary can span one.
2. **Level of effort, forward pass.** Its early start is the earliest bound from links into its start, and no earlier than the point other work of its status would start from: the data date, or a later resume date, for work in progress (TF-020), and the later of the project's planned start and the data date for work not started. Its early finish is the latest bound from links into its finish, and from finish-to-start and finish-to-finish links out of it (the successor's date less the lag). Each date is read at working time on its own calendar. With no link setting its finish, the early finish follows from its duration.
3. **Level of effort, backward pass.** The same links set its late dates from its neighbors' late dates. With no link setting its finish, its late finish is the project deadline, or the calculated finish without one (TF-019). With no link setting its start, its late start follows from its duration, back from its late finish.
4. **Crossing ends.** Where neighbors put a level of effort's start after its finish, each pass holds the end it reaches first: the forward pass keeps the start and raises the finish to it, and the backward pass keeps the finish and lowers the start to it.
5. A finished level of effort is one point in each pass: the data date as its early dates (TF-020) and its late finish as its late dates. A level of effort has no free float.
6. **WBS summary.** Its early dates run from the earliest early start to the latest early finish of the activities in its WBS node and every node below it, and its late dates the same way from their late dates. Only unfinished activities count until all of them have finished. A summary with nothing to span keeps the dates the passes give it.
7. **Longest path.** No driving link reaches a span. An unfinished span is on the longest path when its total float, by the project's float type, equals the least total float among the unfinished activities on the traced path. A finished span is off it.
8. Links between two spans, and a link from a span to an activity in another project, are outside this rule: TotalFloat measures spans in activity order, and refuses a span linked to another project (TF-030).

## Options

None.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 1, p. 15; "Summary Logic", p. 34; Best Practice 6, pp. 84–85: level-of-effort work has no discrete output, takes its duration from the work it supports, and can't drive a successor; hammock activities that derive their dates from detailed work are the preferred way to show it; a summary activity's dates come from lower-level activities, and it should carry no logic of its own. Items 1, 2, 3 and 6.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §5.7 Summaries and Hammocks, p. 65; §5.8 Level of Effort, p. 67: summaries roll up the dates of their tasks, hammocks take their dates from the work they span and don't drive successors, and level of effort should never drive discrete work. Items 1, 2 and 6.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), HAMMOCK ACTIVITY and LEVEL OF EFFORT (LOE): a hammock's duration comes from the two points it spans, and it doesn't change other activities' dates. Items 1 and 2.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task elements, `Summary`: a file marks which tasks are summary tasks, so a summary is a kind of task with its own row. Item 6.
- `observed-file`: files in the corpus with level-of-effort or WBS summary activities. Their stored span dates are consistent with items 2 to 7: a level of effort runs from its earliest start-side neighbor to its latest finish-side neighbor; crossed neighbors hold the start in the forward pass and the finish in the backward pass; a WBS summary spans the unfinished work below it; and unfinished spans flagged on the longest path have the path's total float. No corpus span sits on a path with positive float. Items 2 to 7.
- `own-choice`: items 3 (a level of effort with no start link), 5 (free float), 7 (the comparison with the path's float, where the corpus is thin) and 8; the reasoning is under Rationale.

## Rationale

The sources agree that level-of-effort, hammock and summary activities take their dates from other work and must not drive it, so TotalFloat measures them after each pass and leaves the network as if they weren't there (item 1). A level of effort spans the work its links tie it to, so its start is the earliest of that work and its finish the latest (item 2). When the links say the work ends before it starts, each pass holds the end it controls, which keeps the span a valid span in both passes (item 4).

No corpus level of effort without a start link is unfinished, so item 3's late start from its duration mirrors how its early finish follows from its duration when nothing sets it. A level of effort can't delay anything, so it has no free float to report (item 5).

GAO and NDIA say level of effort should never be critical, while stored longest-path flags do include spans whose float equals the path's. TotalFloat follows the stored flags for the longest path (item 7): a span on it still drives nothing, and it shows the span runs the length of the path. Comparing a span's float with the path's, by the project's float type, is our own choice where the corpus has no case, such as a path with positive float. The critical flag follows the project's definition (TF-026).

Links between spans depend on the order spans are measured, and the corpus has almost no case of them, so TotalFloat makes no claim about them yet (item 8). That rule is still to be written.

## Tests

`crates/engine/src/tests.rs`: `level_of_effort_spans_its_links_and_drives_nothing`, `a_level_of_effort_takes_its_earliest_start_and_latest_finish` (items 1 and 2), `a_level_of_effort_late_dates_come_from_late_neighbors`, `a_level_of_effort_with_no_start_link_measures_back_from_its_finish` (item 3), `each_pass_holds_its_own_end_when_a_level_of_effort_crosses` (item 4), `a_wbs_summary_spans_its_subtree`, `a_wbs_summary_spans_what_is_unfinished` (item 6), `a_span_on_a_path_with_float_stays_on_it`, `a_span_with_float_is_off_the_longest_path`, `a_wbs_summary_with_float_is_off_the_longest_path`, `a_finished_span_is_off_the_longest_path` (item 7), `span_links_do_not_create_network_cycles` (item 1).

## Differences

- GAO and NDIA say level of effort should never be on the critical path ([GAO Schedule Assessment Guide](sources.md#gao-sag), pp. 15 and 84; [NDIA PASEG](sources.md#ndia-paseg), §5.8). TotalFloat can flag a span on the longest path (item 7), though it never drives it.
