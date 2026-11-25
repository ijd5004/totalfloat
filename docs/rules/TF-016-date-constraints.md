# TF-016: Date constraints

How each date constraint bounds an activity in the forward and backward passes, and which combinations of a primary and a secondary constraint TotalFloat accepts.

## Statement

1. A constraint bounds one end of an activity, its start or its finish, in one or both passes:

   | Constraint | Forward pass | Backward pass |
   |---|---|---|
   | Start on or after | Early start no earlier than the date | None |
   | Finish on or after | Early finish no earlier than the date | None |
   | Start on or before | None | Late start no later than the date |
   | Finish on or before | None | Late finish no later than the date |
   | Start on | Early start no earlier than the date | Late start no later than the date |
   | Finish on | Early finish no earlier than the date | Late finish no later than the date |
   | Mandatory start | Early start is the date | Late start is the date |
   | Mandatory finish | Early finish is the date | Late finish is the date |

2. A bound that isn't mandatory combines with logic: the forward pass takes the later of the bound and what the activity's links allow, and the backward pass the earlier. Logic can still move a start-on or finish-on activity past its date, and its total float then goes negative by the difference.
3. A mandatory date replaces the dates logic gives, in both passes. Links into the activity don't move it in the forward pass, and links out of it don't move it in the backward pass. Its neighbors measure their links from the mandatory dates. TF-005 sets where a mandatory date gives way: to the data date for work not started, and to the logic for work in progress whose mandatory finish leaves too little time for its remaining work.
4. A bound whose date falls in nonworking time on the activity's calendar is read at a work position: a forward-pass bound at the next working time, a backward-pass bound at the end of the previous working period, as TF-025 reads any bound. A mandatory finish keeps that endpoint even where another clock moment has the same work position.
5. A dated constraint needs a date. As late as possible needs none (TF-017).
6. An activity can carry a secondary constraint beside its primary one. Both apply, each as item 1 gives; where they bound the same end in the same pass, the later forward bound and the earlier backward bound win.
7. These combinations are refused as invalid input:
   - a secondary constraint without a primary one;
   - a secondary constraint beside a primary start on, finish on, mandatory start or mandatory finish;
   - a mandatory start or mandatory finish as the secondary constraint;
   - a dated constraint with no date.

## Options

None.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), "Date Constraints", pp. 36–37: "no earlier than" constraints act on the forward pass and "no later than" constraints on the backward pass; must-start-on and must-finish-on constraints act on both and override network logic. Items 1 to 3.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Appendix V, Table 10, pp. 181–182: the standard constraint names with their aliases (start on or after, finish on or before, mandatory start and others) and the pass each acts on. The note after the table says some software lets a constraint set the early and late finish while logic can still carry the activity later, which shows negative float; that is the start on and finish on rows. Items 1 and 2.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §5.5 Task Constraints, p. 59: soft constraints let logic move a task later; hard constraints stop it moving. Items 2 and 3.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), DATE CONSTRAINT: a date imposed on an activity's start or finish that overrides logic. Item 3.
- `standard`: [DCMA 14-point schedule metrics](sources.md#dcma-14pt), §4.5 Hard Constraints, pp. 26–27: which constraint types count as hard and which as soft. Item 1.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task elements, `ConstraintType` and `ConstraintDate`: a task holds one constraint type with its date. Items 1 and 5.
- `observed-file`: files in the corpus whose activities carry constraints. Their stored early and late dates fall within these bounds, and stored total float goes negative where a bound is earlier than logic allows. No corpus activity carries a secondary constraint. Items 1 to 4.
- `own-choice`: items 4, 6 and 7; the reasoning is under Rationale.

## Rationale

The sources agree on what each constraint bounds and in which pass, and on mandatory dates overriding logic. TotalFloat keeps two kinds of two-sided constraint apart: start on and finish on bound both passes while logic still wins, so a conflict shows as negative float, and the mandatory dates replace logic. GAO's note on software-specific constraints describes the first kind; its must-start-on is the second.

A bound in nonworking time has to be read somewhere in working time. Reading it the way TF-025 reads every bound keeps constraints consistent with links (item 4).

Two constraints on one activity are both kept, so neither is silently dropped (item 6). A primary constraint that already fixes an end, or two mandatory dates, leaves a secondary one either nothing to add or a conflict with no stated winner, so TotalFloat refuses those pairs rather than inventing a precedence between them (item 7).

## Tests

`crates/engine/src/constraint_tests.rs`: `each_dated_constraint_type_and_provenance` (items 1 and 2), `soft_deadlines_create_negative_float_and_both_constraints_are_kept` (items 2 and 6), `mandatory_dates_override_logic_and_propagate_to_neighbors` (item 3), `nonworking_mandatory_dates_normalize_in_opposite_pass_directions` and `mandatory_finish_keeps_the_normalized_endpoint_even_on_a_work_plateau` (item 4), `invalid_dates_and_conflicts_are_explicit_and_expected_estimates_do_not_override_mandatory` (items 5 and 7).

## Differences

- MS Project's must-start-on and must-finish-on can give way to logic when the project doesn't honor constraint dates ([MSPDI schema](sources.md#msp-xml-schema), Project elements, `HonorConstraints`). TotalFloat has no such setting: a mandatory date holds against logic except where TF-005 makes it give way (item 3).
