# TF-017: As late as possible

How an as-late-as-possible activity uses its free float.

## Statement

1. As late as possible is a constraint with no date. It applies only to work that hasn't started.
2. The forward pass first calculates every activity's early dates as usual. Then, taking as-late-as-possible activities in reverse network order, each moves later to the latest span its successors' early dates and the project's calculated finish allow, within its own backward-pass bounds. With projects scheduled together and float measured from their finish, that finish is the one TF-031 item 5 sets. Its successors and that finish don't move.
3. The activity moves only when that gains it working time. A span that only changes spelling across nonworking time, such as a finish read on Friday evening or Monday morning, is no move.
4. The moved dates are the activity's early dates. Its successors' links, free float and the longest path (TF-026) all use them.
5. A link that progress override drops (TF-022) doesn't hold an as-late-as-possible activity back.
6. An honored expected finish (TF-018) holds the activity where it is. An estimate that's ignored doesn't.
7. A date that the move sets records as-late-as-possible as its cause, wrapping the cause that bounded it.

## Options

None.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Appendix V, p. 182: as late as possible is a standard constraint with no date that, in a forward-scheduled network, makes an activity begin as late as possible and uses up its float; depending on the software it may use up successors' float as well. Items 1 and 2.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task elements, `ConstraintType`: as late as possible is one of a task's constraint types, with no date. Item 1.
- `observed-file`: files in the corpus that hold as-late-as-possible activities. Their stored early dates sit late within free float, their successors' stored dates stay where logic puts them, and stored longest-path flags follow the moved dates. Items 2 and 4.
- `own-choice`: items 3, 5, 6 and 7; the reasoning is under Rationale.

## Rationale

GAO leaves open whether as late as possible also consumes successors' float. TotalFloat moves the activity within its free float only, so marking one activity this way never moves another or the project finish, and the corpus files that use it store dates that fit that (item 2).

A move that gains no working time changes nothing a planner could use, and reporting it would show free float that isn't there (item 3). The activity's dates after the move are its forecast, so everything downstream reads them (item 4). A link that progress override drops no longer constrains anything, in either direction (item 5). An expected finish is the planner's estimate of when the work will end, so it wins over a preference for lateness (item 6; TF-018).

## Tests

`crates/engine/src/constraint_tests.rs`: `alap_consumes_free_float_without_delaying_successors` (items 2 and 7), `an_ignored_estimate_doesnt_stop_as_late_as_possible` (item 6). `crates/engine/src/tests.rs`: `as_late_as_possible_work_reaches_the_same_project_end` (items 2 and 3), `as_late_as_possible_stops_at_an_external_late_finish` (item 2). `crates/engine/src/progress_tests.rs`: `a_dropped_link_does_not_hold_back_as_late_as_possible_work` (item 5).

## Differences

- GAO notes that some software lets an as-late-as-possible activity consume its successors' float too ([GAO Schedule Assessment Guide](sources.md#gao-sag), Appendix V, p. 182). TotalFloat keeps successors on their early dates.
