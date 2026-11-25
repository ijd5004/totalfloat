# TF-041: Earned value

How TotalFloat works out an activity's budget, planned value, earned value, actual cost, estimates, variances and indices, and how it totals them.

## Statement

1. **Budget at completion (BAC).** An activity's budget at completion is the budgeted cost of its assignments and its expenses. An activity with neither has no budget, and its measures are empty unless the baseline plan gives it one (item 3).
2. **Planned value (PV).** Planned value is the planned budget times the share of the planned work that falls before the data date, counted in work time on the activity's calendar. With a baseline, the plan is the baseline activity's planned dates and budget. Without one, it is the activity's own planned dates and budget; an activity with no planned start or finish uses its actual date, or else its early date, in its place. An activity with no calendar counts the share in elapsed time.
3. **Work the plan doesn't hold.** Work the baseline didn't plan has no planned value. Planned work whose assignments have gone keeps its baseline planned value, with no budget now.
4. **Earned value (EV).** Under the percent complete technique, earned value is the budget at completion times the activity's percent complete, by its percent complete type ([TF-023](TF-023-duration-and-physical-percent-complete.md), [TF-004](TF-004-units-percent-complete.md)). Finished work has earned its whole budget. A WBS node with no technique set uses percent complete.
5. **Other techniques.** A technique code the model doesn't name is kept as written. Its activities earn no value TotalFloat can name, so they count as unmeasured.
6. **Actual cost (AC).** Actual cost is the regular and overtime actual cost of the activity's assignments, plus its expenses' actual cost.
7. **Estimate to complete (ETC),** by the WBS node's method. The budget left is the budget at completion less earned value, never below zero.
   - Remaining cost: the stored remaining cost. A node with no method set uses this.
   - Performance factor 1: the budget left.
   - Cost performance index: the budget left divided by the cost performance index. With nothing spent yet there's no index, and the factor is 1. With money spent and nothing earned the index is 0, and there's no estimate.
   - User factor: the budget left divided by the node's user factor, or multiplied by it, as the `user_factor` option says. With no factor above zero stored, there's no estimate.
   - Any other method code gives no estimate.
8. **Estimate at completion (EAC).** Actual cost plus the estimate to complete.
9. **Variances and indices.** Schedule variance is earned value less planned value, and cost variance is earned value less actual cost. The schedule performance index is earned value over planned value, empty with nothing planned yet. The cost performance index is earned value over actual cost, empty with nothing spent yet.
10. **Totals.** Totals for a group, such as a WBS node, add the amounts and work the indices out again from the totals. An estimate is totalled only if every member has one. Unmeasured activities are counted as such and left out of the totals, their spending included.
11. **Where files hold it.** In XER, per WBS node: `PROJWBS.ev_compute_type` (technique; `EC_Cmp_pct` is percent complete), `ev_etc_compute_type` (method; `EE_Rem_hr`, `EE_PF_1`, `EE_PF_cpi` and `EE_PF_user`), `ev_user_pct` and `ev_etc_user_value`. Costs come from `TASKRSRC` and `PROJCOST`, planned dates from `TASK.target_start_date` and `target_end_date`. An XER file stores no cumulative earned value results. In MS Project XML, `EarnedValueMethod` chooses percent complete or physical percent complete, and tasks and assignments store `BCWS`, `BCWP` and `ACWP`.

## Options

Each WBS node selects its technique and method, through the `PROJWBS` fields in item 11; the model holds them as `Wbs.earned_value`.

- **`technique`**: percent complete (default) or a code the model keeps without naming. Without a value, percent complete applies, because it's the technique every corpus node that sets one uses.
- **`estimate`**: remaining cost (default), performance factor 1, cost performance index, user factor, or a code the model keeps without naming. Without a value, remaining cost applies, because it's the method most corpus nodes use.
- **`user_factor`**: `divide` (default) or `multiply`, how the user-factor method applies the node's factor to the budget left. A file holds the factor but not which way it reads, so no file selects this; `query --user-factor` selects it for a run. Without it, `divide` applies, because every public estimate-at-completion formula divides the work left by an efficiency index.

## Sources

- `standard`: [DoD EVMSIG](sources.md#dod-evmsig), §7.2 Definitions, pp. 78–87: budget at completion, budgeted cost for work scheduled (planned value) and performed (earned value), actual cost of work performed, estimate to complete, estimate at completion as actual cost plus estimate to complete, and cost and schedule variance as earned value less actual cost and less planned value. Items 1, 2, 4, 6, 8 and 9.
- `standard`: [DCMA 14-point schedule metrics](sources.md#dcma-14pt), §2.2, p. 3: an estimate at completion divides the budget left by an efficiency index: the cost performance index, or the product of the cost and schedule indices. §3.1.1.1, p. 14, and §3.1.2.1, p. 16: the cost performance index is earned value over actual cost, and the schedule performance index is earned value over planned value. Item 7, the default of the `user_factor` option, and item 9.
- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Appendix III, Table 8, p. 170: percent complete is one way of measuring earned value, among 0/100, 50/50, weighted milestones and units. Items 4 and 5.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), `EarnedValueMethod` and the Task and Assignment elements `BCWS`, `BCWP` and `ACWP`: another scheduler chooses between percent complete and physical percent complete, and stores the three values. Items 4 and 11.
- `observed-file`: the WBS nodes in the XER files of the corpus. Every node that sets a technique uses `EC_Cmp_pct`. Most use the remaining cost method; `EE_PF_1`, `EE_PF_cpi` and `EE_PF_user` each appear, the last with a user factor stored. Item 11 and the defaults under Options.
- `own-choice`: items 2, 3, 5, 7 (no estimate where the index or factor is missing, and the `user_factor` option), 10 and 11 (the meanings of the `EC_*` and `EE_*` codes, read from their names); the reasoning is under Rationale.

## Rationale

- **Planned value in work time** (item 2). An even spread of the budget over working time is the shape a plan with no curve has ([TF-039](TF-039-resource-curves.md), item 5). Without a baseline, the activity's own planned dates are the only plan the file holds, so the project is measured against itself.
- **Missing plan** (item 3). Planned value is what the plan said, so work outside the plan has none, and work the plan held keeps what it said even after its resources go.
- **Unnamed techniques** (item 5). The corpus holds one technique code. Guessing what another code means would report earned value nobody can check, so those activities are counted apart.
- **The user factor** (item 7). A file holds the factor a planner typed, and nothing says which way it reads. The cost performance method doesn't settle it: dividing by the index and multiplying by its inverse give the same estimate. The public formulas all divide the work left by an efficiency index, so `divide` is the default; read that way, a factor of 0.9 says the rest will go at nine tenths of the planned efficiency, and raises the estimate. A planner who typed the factor as a multiplier on the budget left chooses `multiply`. Since no source shows which reading a file's factor holds, the choice is an option ([ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules)), and only one corpus file uses the method.
- **No estimate** (item 7). With money spent and nothing earned, dividing by an index of zero has no answer. A user factor the file doesn't give can't be guessed.
- **Totals** (item 10). Ratios don't add, so indices come from the totals. A total estimate missing some members would understate the whole.

## Tests

- `crates/evm/src/lib.rs`: `percent_complete_earns_its_share_of_the_budget`, `finished_work_has_earned_its_whole_budget` (item 4), `an_unnamed_technique_earns_nothing_it_can_name` (item 5), `each_estimate_method` (items 7 and 8, and both readings of the `user_factor` option), `spending_with_nothing_earned_gives_no_estimate_by_cpi`, `with_nothing_spent_the_cost_index_waits_and_its_factor_is_one` (items 7 and 9), `totals_add_amounts_and_recompute_indices` (item 10).
- `crates/cli/src/query/earned.rs`: `planned_value_is_the_share_of_the_plan_due_by_the_data_date` (item 2), `the_baseline_decides_what_was_planned` (items 2 and 3).
- `crates/cli/tests/cli.rs`: `a_user_performance_factor_divides_unless_told_to_multiply` (item 7 and the `user_factor` option, through `query --user-factor`).

## Differences

None known.
