//! Costs grouped by the cost account they're booked to.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Serialize;
use totalfloat_model::{ActivityId, CostAccountId, ProjectId, Schedule};

use super::Found;

/// One cost account's costs over the matching activities: its assignments'
/// and expenses' costs booked to it. Accounts under it have their own lines.
#[derive(Debug, Serialize, PartialEq)]
pub(super) struct AccountCosts {
    /// The account's path, its short names from the top down joined by dots
    /// (`1.3`); none for costs booked to no account.
    account: Option<String>,
    name: Option<String>,
    /// The path of the account it sits under, so callers can roll up.
    parent: Option<String>,
    budgeted_cost: f64,
    actual_cost: f64,
    remaining_cost: f64,
    at_completion_cost: f64,
}

/// What the costs of one account add up to, before rounding.
#[derive(Default, Clone, Copy)]
struct Sums {
    budgeted: f64,
    actual: f64,
    remaining: f64,
}

/// Each account's costs over the matching activities, sorted by path, with
/// costs booked to no account last. Accounts with no costs here are left out.
pub(super) fn by_account(found: &[Found<'_>]) -> Vec<AccountCosts> {
    let mut wanted: BTreeMap<ProjectId, (&Schedule, BTreeSet<ActivityId>)> = BTreeMap::new();
    for f in found {
        wanted
            .entry(f.schedule.project.id)
            .or_insert_with(|| (f.schedule, BTreeSet::new()))
            .1
            .insert(f.activity.id);
    }
    let mut sums: BTreeMap<Option<CostAccountId>, Sums> = BTreeMap::new();
    let mut add = |account, budgeted: Option<f64>, actual: f64, remaining: Option<f64>| {
        let held = sums.entry(account).or_default();
        held.budgeted += budgeted.unwrap_or(0.0);
        held.actual += actual;
        held.remaining += remaining.unwrap_or(0.0);
    };
    for (schedule, activities) in wanted.values() {
        for a in schedule
            .assignments
            .values()
            .filter(|a| activities.contains(&a.activity))
        {
            let m = &a.amounts;
            let actual = m.actual_cost.unwrap_or(0.0) + m.actual_overtime_cost.unwrap_or(0.0);
            add(a.cost_account, m.budgeted_cost, actual, m.remaining_cost);
        }
        for x in schedule
            .expenses
            .values()
            .filter(|x| activities.contains(&x.activity))
        {
            let actual = x.actual_cost.unwrap_or(0.0);
            add(x.cost_account, x.budgeted_cost, actual, x.remaining_cost);
        }
    }
    // Accounts are file-wide, so any matching schedule names them.
    let first = wanted.values().next().map(|(s, _)| *s);
    let account = |id: Option<CostAccountId>| id.and_then(|id| first?.cost_accounts.get(&id));
    let path = |id: Option<CostAccountId>| {
        id.zip(first)
            .and_then(|(id, s)| totalfloat_edit::cost_account_path(s, id))
    };
    let mut out: Vec<AccountCosts> = sums
        .into_iter()
        .map(|(id, s)| {
            let held = account(id);
            AccountCosts {
                account: path(id),
                name: held.map(|a| a.name.clone()),
                parent: path(held.and_then(|a| a.parent)),
                budgeted_cost: cents(s.budgeted),
                actual_cost: cents(s.actual),
                remaining_cost: cents(s.remaining),
                at_completion_cost: cents(s.actual + s.remaining),
            }
        })
        .collect();
    out.sort_by(|a, b| (a.account.is_none(), &a.account).cmp(&(b.account.is_none(), &b.account)));
    out
}

fn cents(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// One line per account, for the human report.
pub(super) fn lines(costs: &[AccountCosts]) -> String {
    let mut out = String::from("\nCosts by cost account:");
    for c in costs {
        let account = c.account.as_deref().unwrap_or("(none)");
        let _ = write!(
            out,
            "\n  {account}: budget {}, actual {}, remaining {}, at completion {}",
            c.budgeted_cost, c.actual_cost, c.remaining_cost, c.at_completion_cost
        );
    }
    out
}
