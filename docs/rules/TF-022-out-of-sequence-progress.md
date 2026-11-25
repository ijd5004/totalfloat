# TF-022: Out-of-sequence progress

When recorded progress has broken a relationship, and how each out-of-sequence setting schedules the work around it.

## Statement

1. A relationship is out of sequence when recorded progress has broken it:

   | Type | Broken when |
   |---|---|
   | Finish to start | The successor has started and the predecessor hasn't finished |
   | Start to start | The successor has started and the predecessor hasn't |
   | Finish to finish | The successor has finished and the predecessor hasn't |
   | Start to finish | The successor has finished and the predecessor hasn't started |

   The same test applies to a link to or from an activity in another project, reading that activity's status.
2. **Retained logic** keeps every link. A started successor's remaining work waits for its unfinished predecessor as the link requires.
3. Under retained logic, a finished activity with an unfinished predecessor is a point that the predecessor can push: the point moves to where the link puts it, which can be after the data date, and it holds back its own successors from there, finished ones included.
4. Under retained logic, a finished point that a predecessor holds but doesn't push past the data date moves to the next working time at or after the data date. Moving to working time alone doesn't count as a push. A pushed point stays where its bound falls, such as at the end of a working day.
5. **Progress override** drops every broken link from both passes. The successor carries on from the data date, and the predecessor's late dates owe the successor nothing. Links that progress hasn't broken stay. Finished work stays on the data date.
6. A project that selects neither retained logic nor progress override is refused once any of its activities has started. With no progress there is nothing out of sequence, and it schedules as usual.

## Options

- **`out_of_sequence`**: `retained_logic` (default) or `progress_override`. An XER file selects it through `SCHEDOPTIONS.sched_retained_logic` and `SCHEDOPTIONS.sched_progress_override`: `Y` and `N` mean retained logic, `N` and `Y` progress override, and `N` in both selects neither, which [TF-008](TF-008-xer-schedule-options.md) records as actual dates (item 6). Without a setting, retained logic applies, because most corpus projects that hold one use it; `Y` in both is a contradiction, and the reader warns and applies retained logic. MS Project XML has no project-wide setting of this kind; see Differences.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 9, "Out-of-Sequence Logic", pp. 126–127 and Figures 45–47: out-of-sequence progress is an activity starting before its finish-to-start or start-to-start predecessor allows, or finishing before its finish-to-finish predecessor has finished; under retained logic the rest of the activity waits for its predecessor, and under progress override work carries on regardless of the predecessor. Items 1, 2 and 5.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), OUT-OF-SEQUENCE PROGRESS, RETAINED LOGIC and PROGRESS OVERRIDE: the same two treatments, and that software may offer them as a switch. Items 1, 2, 5 and the option.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §9.1.1, pp. 111–114: out-of-sequence status moves forecast dates. Item 2.
- `observed-file`: files in the corpus with out-of-sequence links. Under retained logic, remaining work stores dates after its unfinished predecessor, and some finished activities store a point after the data date, where their unfinished predecessor puts them. Under progress override, the successor stores dates from the data date and the predecessor's late dates ignore the broken link. Items 2 to 5.
- `own-choice`: items 1 (start to finish), 3, 4 and 6; the reasoning is under Rationale.

## Rationale

The sources name the two treatments and agree on what each does to the out-of-sequence activity; the setting chooses between them ([ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules)). GAO lists the broken conditions for three link types. A start-to-finish link is broken by the same reasoning, when the successor has finished before its predecessor started (item 1).

Retained logic keeps every link, so an unfinished predecessor still bounds a finished successor's point, and the stored dates in the corpus show that point moving (item 3). A point that is held without being pushed reads as the next working time, the place an activity would start (item 4).

An XER file can turn both flags off, which selects neither treatment. No corpus file does, and no public source we can cite says what it means. TotalFloat refuses it instead of guessing (item 6).

## Tests

`crates/engine/src/progress_tests.rs`: `each_relationship_type_detects_broken_logic` (item 1), `retained_logic_holds_remaining_work_for_each_broken_link` (item 2), `finish_side_links_push_finished_work_under_retained_logic`, `retained_logic_pushes_finished_work_and_on_through_it` (item 3), `something_still_holding_finished_work_moves_it_to_working_time`, `moving_to_working_time_is_not_a_push` (item 4), `progress_override_drops_a_broken_link_in_both_passes`, `progress_override_drops_finish_side_links_too`, `progress_override_keeps_links_progress_has_not_broken`, `progress_override_leaves_finished_work_on_the_data_date` (item 5), `unsupported_progress_is_refused_rather_than_approximated` (item 6).

## Differences

- MS Project has no out-of-sequence setting. Its options split in-progress tasks and move completed and remaining parts around the status date ([Project VBA reference](sources.md#msp-vba-reference), `OptionsSchedule`, `AutoSplit`; `OptionsCalculation`, `MoveCompleted` and `MoveRemaining`).
