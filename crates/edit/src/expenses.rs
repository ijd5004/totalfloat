//! Expenses: costs on an activity that no resource carries (TF-037), and the
//! cost accounts that assignments' and expenses' costs are booked to.

use std::fmt::Write as _;

use totalfloat_model::{
    Accrual, ActivityId, CostAccount, CostAccountId, Expense, ExpenseCategoryId, ExpenseId,
    Schedule,
};

use crate::{
    EditError, ExpenseAccrual, NextIds, find, no_resource, no_role, one_named, refuse,
    resource_named, role_named,
};

impl From<ExpenseAccrual> for Accrual {
    fn from(accrual: ExpenseAccrual) -> Self {
        match accrual {
            ExpenseAccrual::Start => Self::Start,
            ExpenseAccrual::End => Self::End,
            ExpenseAccrual::Uniform => Self::Uniform,
        }
    }
}

/// A cost account's ID as a path: the short names from the top of the tree
/// down, joined by dots, as `1.3`. Short names repeat under different parents
/// (one corpus file has three that do), so the path is what names one account
/// for sure. `None` for an account the file doesn't hold; a loop of parents
/// stops the path where it would repeat.
#[must_use]
pub fn cost_account_path(schedule: &Schedule, id: CostAccountId) -> Option<String> {
    let mut parts = Vec::new();
    let mut at = Some(id);
    while let Some(here) = at {
        let Some(account) = schedule.cost_accounts.get(&here) else {
            break;
        };
        // A loop of parents: the path stops where it would repeat.
        if parts.len() >= schedule.cost_accounts.len() {
            break;
        }
        parts.push(account.short_name.as_str());
        at = account.parent;
    }
    if parts.is_empty() {
        return None;
    }
    parts.reverse();
    Some(parts.join("."))
}

/// The cost account a name means here: the one account whose path (see
/// [`cost_account_path`]) or short name it is, or else the one whose name it
/// is. A name that fits two accounts means neither.
#[must_use]
pub fn cost_account_named(schedule: &Schedule, wanted: &str) -> Option<CostAccountId> {
    let lower = wanted.to_lowercase();
    let only = |fits: &dyn Fn(&CostAccount) -> bool| {
        let mut found = schedule.cost_accounts.values().filter(|a| fits(a));
        match (found.next(), found.next()) {
            (Some(one), None) => Some(Some(one.id)),
            (Some(_), Some(_)) => Some(None),
            (None, _) => None,
        }
    };
    let by_id = |a: &CostAccount| {
        a.short_name.to_lowercase() == lower
            || cost_account_path(schedule, a.id).is_some_and(|p| p.to_lowercase() == lower)
    };
    only(&by_id)
        .or_else(|| only(&|a: &CostAccount| a.name.to_lowercase() == lower))
        .flatten()
}

/// The expense category a name means here.
#[must_use]
pub fn category_named(schedule: &Schedule, wanted: &str) -> Option<ExpenseCategoryId> {
    one_named(&schedule.expense_categories.values(), wanted, |c| {
        (&c.name, &c.name)
    })
    .map(|c| c.id)
}

/// How expense items are matched: ignoring case and spaces at the ends, which
/// an item can hold but nobody types.
#[must_use]
pub fn item_key(item: &str) -> String {
    item.trim().to_lowercase()
}

/// What's left of a cost at completion once the actual cost is spent. It
/// never crosses zero, so an overspent expense has nothing left to spend,
/// and what's left of a credit is still a credit.
#[must_use]
pub fn rest_of(at_completion: f64, actual: f64) -> f64 {
    let rest = at_completion - actual;
    if at_completion < 0.0 {
        rest.min(0.0)
    } else {
        rest.max(0.0)
    }
}

/// The expense on `activity` with this item.
fn expense_named(
    schedule: &Schedule,
    (code, activity): (&str, ActivityId),
    wanted: &str,
) -> Result<ExpenseId, EditError> {
    let key = item_key(wanted);
    let mut found = schedule
        .expenses
        .values()
        .filter(|x| x.activity == activity && item_key(&x.name) == key);
    match (found.next(), found.next()) {
        (Some(one), None) => Ok(one.id),
        (None, _) => refuse(
            "no-such-expense",
            format!("{code} has no expense {wanted}"),
            "`query --columns code,expenses` lists expenses",
        ),
        (Some(_), Some(_)) => refuse(
            "ambiguous-expense",
            format!("{code} has more than one expense {wanted}"),
            "rename one in P6, or edit the file's text",
        ),
    }
}

/// A cost, which is negative for a credit, as P6's files can hold.
fn money(value: f64, what: &str) -> Result<f64, EditError> {
    if value.is_finite() {
        Ok(value)
    } else {
        refuse(
            "invalid-cost",
            format!("{value} can't be the {what}"),
            "give a plain number, negative for a credit",
        )
    }
}

fn shown(value: f64) -> String {
    let cents = (value * 100.0).round() / 100.0;
    format!("{cents}")
}

pub(crate) struct NewExpense<'a> {
    pub(crate) activity: &'a str,
    pub(crate) name: &'a str,
    pub(crate) cost: f64,
    pub(crate) account: Option<&'a str>,
    pub(crate) category: Option<&'a str>,
    pub(crate) accrual: Option<ExpenseAccrual>,
}

/// One unit at the budget's price, as most corpus expenses are, with all of
/// it still to spend.
pub(crate) fn add_expense(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    new: &NewExpense<'_>,
) -> Result<String, EditError> {
    let NewExpense {
        activity: code,
        name,
        ..
    } = *new;
    // Spaces at the ends are kept, as an item read from a file can have them.
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return refuse(
            "invalid-expense",
            format!("{name:?} can't be an expense item"),
            "write it on one line",
        );
    }
    let cost = money(new.cost, "budgeted cost")?;
    let activity = find(schedule, code)?;
    let free = match expense_named(schedule, (code, activity), name) {
        Err(e) => e.code == "no-such-expense",
        Ok(_) => false,
    };
    if !free {
        return refuse(
            "duplicate-expense",
            format!("{code} already has an expense {name}"),
            "use set_expense to change its cost",
        );
    }
    let cost_account = match new.account {
        None => None,
        Some(wanted) => Some(
            cost_account_named(schedule, wanted).ok_or_else(|| EditError {
                code: "no-such-cost-account",
                message: format!("no one cost account matches {wanted}"),
                hint: "`inspect` lists cost accounts by path".to_owned(),
            })?,
        ),
    };
    let category = match new.category {
        None => None,
        Some(wanted) => Some(category_named(schedule, wanted).ok_or_else(|| EditError {
            code: "no-such-category",
            message: format!("no one expense category matches {wanted}"),
            hint: "`inspect` lists expense categories".to_owned(),
        })?),
    };
    let free = schedule
        .expenses
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let id = ExpenseId(ids.expense.max(free));
    ids.expense = id.0 + 1;
    let project = schedule.project.id;
    schedule.expenses.insert(
        id,
        Expense {
            id,
            activity,
            project,
            name: name.to_owned(),
            cost_account,
            category,
            accrual: Some(new.accrual.map_or(Accrual::Uniform, Accrual::from)),
            budgeted_cost: Some(cost),
            actual_cost: Some(0.0),
            remaining_cost: Some(cost),
            price_per_unit: Some(cost),
            budgeted_units: Some(1.0),
            unit_of_measure: String::new(),
            auto_compute_actuals: false,
            po_number: String::new(),
            vendor: String::new(),
            description: String::new(),
            source: None,
        },
    );
    Ok(format!("{code} has expense {name}, {}", shown(cost)))
}

/// Costs and accrual to set on an expense; those left out stay as they are.
#[derive(Clone, Copy)]
pub(crate) struct Costs {
    pub(crate) budgeted: Option<f64>,
    pub(crate) actual: Option<f64>,
    pub(crate) remaining: Option<f64>,
    pub(crate) accrual: Option<ExpenseAccrual>,
}

/// A new budget keeps the units and moves the price. Without a remaining
/// cost, the cost at completion stays as it was, so the remaining cost is what
/// the actual cost leaves of it, except that while nothing was spent it
/// follows a new budget. That's how the text format reads a line, too.
pub(crate) fn set_expense(
    schedule: &mut Schedule,
    (code, name): (&str, &str),
    costs: Costs,
) -> Result<String, EditError> {
    if costs.budgeted.is_none()
        && costs.actual.is_none()
        && costs.remaining.is_none()
        && costs.accrual.is_none()
    {
        return refuse(
            "invalid-edit",
            format!("nothing to set on {code}'s expense {name}"),
            "give a budgeted, actual or remaining cost, or an accrual",
        );
    }
    let budgeted = costs
        .budgeted
        .map(|v| money(v, "budgeted cost"))
        .transpose()?;
    let actual = costs.actual.map(|v| money(v, "actual cost")).transpose()?;
    let remaining = costs
        .remaining
        .map(|v| money(v, "remaining cost"))
        .transpose()?;
    let activity = find(schedule, code)?;
    let id = expense_named(schedule, (code, activity), name)?;
    let x = schedule.expenses.get_mut(&id).ok_or_else(|| EditError {
        code: "no-such-expense",
        message: format!("{code} has no expense {name}"),
        hint: String::new(),
    })?;
    let unspent = x.actual_cost.unwrap_or(0.0) == 0.0;
    let mut at_completion = x.at_completion_cost();
    if let Some(budget) = budgeted {
        x.budgeted_cost = Some(budget);
        if let Some(units) = x.budgeted_units.filter(|units| *units > 0.0) {
            x.price_per_unit = Some(budget / units);
        } else {
            x.budgeted_units = Some(1.0);
            x.price_per_unit = Some(budget);
        }
        if unspent {
            at_completion = budget;
        }
    }
    if let Some(actual) = actual {
        x.actual_cost = Some(actual);
    }
    // An accrual alone leaves the costs as they are.
    if let Some(remaining) = remaining {
        x.remaining_cost = Some(remaining);
    } else if budgeted.is_some() || actual.is_some() {
        x.remaining_cost = Some(rest_of(at_completion, x.actual_cost.unwrap_or(0.0)));
    }
    if let Some(accrual) = costs.accrual {
        x.accrual = Some(accrual.into());
    }
    Ok(format!(
        "{code} expense {name}: budget {}, actual {}, remaining {}",
        shown(x.budgeted_cost.unwrap_or(0.0)),
        shown(x.actual_cost.unwrap_or(0.0)),
        shown(x.remaining_cost.unwrap_or(0.0)),
    ))
}

pub(crate) fn delete_expense(
    schedule: &mut Schedule,
    (code, name): (&str, &str),
) -> Result<String, EditError> {
    let activity = find(schedule, code)?;
    let id = expense_named(schedule, (code, activity), name)?;
    let spent = schedule.expenses.remove(&id).and_then(|x| x.actual_cost);
    let mut line = format!("{code} expense {name} deleted");
    if let Some(spent) = spent.filter(|s| *s != 0.0) {
        let _ = write!(line, "; its actual cost of {} is gone", shown(spent));
    }
    Ok(line)
}

/// Books an assignment's or expense's costs to a cost account, or to none.
/// A resource on an activity more than once has each assignment booked.
pub(crate) fn set_cost_account(
    schedule: &mut Schedule,
    code: &str,
    (resource, role, expense): (Option<&str>, Option<&str>, Option<&str>),
    account: Option<&str>,
) -> Result<String, EditError> {
    let booked = match account {
        None => None,
        Some(wanted) => Some(
            cost_account_named(schedule, wanted).ok_or_else(|| EditError {
                code: "no-such-cost-account",
                message: format!("no one cost account matches {wanted}"),
                hint: "`inspect` lists cost accounts by path".to_owned(),
            })?,
        ),
    };
    let activity = find(schedule, code)?;
    let what = match (resource, role, expense) {
        (Some(wanted), None, None) => {
            let id = resource_named(schedule, wanted).ok_or_else(|| no_resource(wanted))?;
            book(schedule, booked, |a| {
                a.activity == activity && a.resource == Some(id)
            })
            .then(|| format!("{code}'s {wanted}"))
            .ok_or_else(|| not_assigned(code, wanted))?
        }
        (None, Some(wanted), None) => {
            let id = role_named(schedule, wanted).ok_or_else(|| no_role(wanted))?;
            book(schedule, booked, |a| {
                a.activity == activity && a.resource.is_none() && a.role == Some(id)
            })
            .then(|| format!("{code}'s role {wanted}"))
            .ok_or_else(|| not_assigned(code, wanted))?
        }
        (None, None, Some(wanted)) => {
            let id = expense_named(schedule, (code, activity), wanted)?;
            if let Some(x) = schedule.expenses.get_mut(&id) {
                x.cost_account = booked;
            }
            format!("{code}'s expense {wanted}")
        }
        _ => {
            return refuse(
                "invalid-edit",
                format!("which of {code}'s costs to book isn't clear"),
                "give one of a resource, a role or an expense",
            );
        }
    };
    Ok(match account {
        Some(name) => format!("{what} is booked to cost account {name}"),
        None => format!("{what} has no cost account"),
    })
}

/// Sets the cost account of the assignments `held` picks; false if none.
fn book(
    schedule: &mut Schedule,
    account: Option<CostAccountId>,
    held: impl Fn(&totalfloat_model::Assignment) -> bool,
) -> bool {
    let mut any = false;
    for a in schedule.assignments.values_mut().filter(|a| held(a)) {
        a.cost_account = account;
        any = true;
    }
    any
}

fn not_assigned(code: &str, wanted: &str) -> EditError {
    EditError {
        code: "not-assigned",
        message: format!("{code} doesn't have {wanted}"),
        hint: "`query --columns code,resources,roles` lists assignments".to_owned(),
    }
}
