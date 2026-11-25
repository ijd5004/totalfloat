# TF-037: Expenses and cost accounts

What an expense on an activity holds, how its costs relate when edits change them, and how costs are booked to cost accounts.

## Statement

1. **An expense.** An expense is a cost on an activity that no resource carries, such as a purchase or a fee. It can name a cost account and an expense category.
2. **Its costs.** An expense has a budgeted cost, an actual cost and a remaining cost. Its cost at completion is its actual plus its remaining cost. Its costs count in the activity's total costs and in its earned value ([TF-041](TF-041-earned-value.md)).
3. **A new expense.** A new expense is one unit at its budgeted cost, with no actual cost and all of it remaining. It spreads uniformly over the activity unless told otherwise.
4. **A new budget.** A new budgeted cost keeps the units and changes the price per unit.
5. **Cost at completion.** Unless an edit gives the remaining cost, the cost at completion holds and the remaining cost is what the actual cost leaves of it, never below zero. While nothing is spent, a new budget also becomes the cost at completion.
6. **Credits.** A cost can be below zero, a credit. What's left of a credit stays at or below zero.
7. **Finding an expense.** Expenses are found by their item name on their activity, ignoring case and spaces at the ends. So these edits can't put one item on an activity twice. An item's own spaces are kept as written.
8. **Finished work.** A finished activity can take a new expense, such as a late invoice.
9. **Deleting.** Deleting an activity deletes its expenses, and deleting an expense deletes its user-defined field values.
10. **Cost accounts.** An assignment's costs and an expense's costs are booked to one cost account from the file's tree of accounts, or to none. An edit names an account by its path, the short names from the top of the tree down joined by dots, or by a short name or name that only one account has. Booking a resource's costs on an activity books each of its assignments there.
11. **An account the file lacks.** An assignment or expense that names an account the file doesn't hold reads as booked to none, with a warning.
12. **The project's default account.** A project's default cost account is kept as the file holds it, and isn't given to new assignments or expenses.
13. **Not edited yet.** Cost accounts and expense categories can't be added or changed by edits.
14. **Where files hold it.** In XER: `PROJCOST` (`target_cost`, `act_cost`, `remain_cost`, `target_qty`, `cost_per_qty`, `cost_load_type`, `acct_id`, `cost_type_id`), `ACCOUNT` (`acct_short_name`, `parent_acct_id`), `COSTTYPE`, `TASKRSRC.acct_id` and `PROJECT.acct_id`.

## Options

None.

## Sources

- `standard`: [DoD EVMSIG](sources.md#dod-evmsig), §7.2 Definitions, ESTIMATE AT COMPLETION and ESTIMATE TO COMPLETE, p. 82: the estimate at completion is the actual cost plus the estimate to complete. Items 2 and 5.
- `standard`: [DoD EVMSIG](sources.md#dod-evmsig), §7.2 Definitions, BUDGET AT COMPLETION, p. 79: the budget at completion sums the budgets through any level of the breakdown structure, so costs book to one place in a tree. Items 2 and 10.
- `observed-file`: the files in the corpus that hold expenses. Every expense's budgeted cost is its price times its units, most have one unit, and most spread uniformly. None is a credit. Items 3, 4 and 6.
- `observed-file`: files in the corpus whose assignments are booked to cost accounts. None names an account its file lacks. One file repeats short names under different parents, which is why item 10 names accounts by path. Items 10 and 11.
- `own-choice`: items 2, 5 to 9 and 11 to 13; the reasoning is under Rationale.

## Rationale

- **Cost at completion holds** (item 5). An expense's budget is a plan, and a planner who records what was spent hasn't said the total changed, so the rest is what remains of it. While nothing is spent, the budget and the forecast are the same estimate. Overspending shows in the actual cost, so the remaining cost stops at zero.
- **Credits** (item 6). A file can hold a refund or credit as a negative cost, so TotalFloat keeps one, and its remainder keeps the credit's sign.
- **Finding by item** (item 7). Planners type item names, and case and stray spaces at the ends shouldn't make one item into two. The file keeps the name as it was written.
- **Finished work** (item 8). Invoices arrive after work ends, and the cost belongs to the activity it paid for.
- **Accounts by path** (item 10). Short names repeat across branches in real files, so the path is the name that is always unique.
- **A missing account** (item 11). A cost booked to an account the file doesn't hold can't be shown under it, and refusing the file would lose the rest of it. A warning says what happened.
- **Default account** (item 12). Giving every new cost the project's default would book it without the planner choosing to.

## Tests

`crates/edit/src/tests.rs`: `an_expense_is_added_with_its_account_category_and_accrual` (items 1, 3 and 10), `an_expense_s_cost_at_completion_holds_without_a_remaining_cost`, `an_expense_s_costs_are_set_and_it_is_deleted`, `an_accrual_alone_leaves_an_expense_s_costs` (items 4, 5 and 9), `an_expense_can_be_a_credit` (item 6), `two_expenses_of_one_name_can_t_be_told_apart`, `an_expense_item_keeps_spaces_at_its_ends_and_is_found_without_them` (item 7), `deleting_an_activity_takes_its_expenses` (item 9), `costs_are_booked_to_a_cost_account_and_to_none`, `a_cost_account_is_named_by_its_path_where_short_names_repeat`, `a_cost_account_name_that_fits_two_is_refused_and_loops_have_paths` (item 10).

## Differences

None known.
