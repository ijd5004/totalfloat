//! Earned value columns and totals for `query` (TF-041).

use serde::Serialize;
use serde_json::Value;
use totalfloat_calendar::WorkCalendar;
use totalfloat_evm::{Inputs, Measures, UserFactor};
use totalfloat_model::{Activity, DateTime, EarnedValue, Schedule};

use super::percent::{self, Totals};

/// The columns this module fills. `actual_cost` comes from the progress columns.
pub(super) const COLUMNS: [&str; 9] = [
    "budget_at_completion",
    "planned_value",
    "earned_value",
    "estimate_to_complete",
    "estimate_at_completion",
    "schedule_variance",
    "cost_variance",
    "schedule_performance_index",
    "cost_performance_index",
];

/// The plan planned value is measured against: the baseline's planned dates
/// and budget, or else the activity's own planned dates and budget.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Plan {
    pub(super) start: Option<DateTime>,
    pub(super) finish: Option<DateTime>,
    pub(super) budget: Option<f64>,
}

/// The activity's own plan: its planned dates, or its dates if it has none.
pub(super) fn own_plan(a: &Activity) -> Plan {
    Plan {
        start: a.planned_start.or_else(|| super::start_of(a)),
        finish: a.planned_finish.or_else(|| super::finish_of(a)),
        budget: None,
    }
}

/// The share of planned work before the data date, in working time on the
/// activity's calendar, or in calendar time if it has none.
fn planned_fraction(
    calendar: Option<&WorkCalendar>,
    plan: Plan,
    data_date: Option<DateTime>,
) -> f64 {
    let (Some(start), Some(finish), Some(now)) = (plan.start, plan.finish, data_date) else {
        return 0.0;
    };
    if now <= start {
        return 0.0;
    }
    if now >= finish {
        return 1.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "minutes are far below 2^52")]
    let share = |done: i64, all: i64| {
        if all > 0 {
            done as f64 / all as f64
        } else {
            1.0
        }
    };
    match calendar {
        Some(c) => share(
            c.work_between(start, now).get(),
            c.work_between(start, finish).get(),
        ),
        None => share(
            now.minutes_since(start).get(),
            finish.minutes_since(start).get(),
        ),
    }
}

/// Why an activity has no earned value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unmeasured {
    /// No assignments or expenses, now or in the plan, so nothing to cost.
    NoCost,
    /// Its WBS node's technique isn't one the model names.
    Technique,
}

/// An activity's measures. With a baseline plan, work it didn't plan has no
/// planned value, and planned work with no assignments left still has its plan.
pub(super) fn measures(
    schedule: &Schedule,
    a: &Activity,
    totals: Option<&Totals>,
    (plan, calendar): (Plan, Option<&WorkCalendar>),
    reading: UserFactor,
) -> Result<Measures, Unmeasured> {
    let planned = plan.budget.is_some_and(|b| b > 0.0);
    let held = match totals {
        Some(held) => *held,
        None if planned => Totals::default(),
        None => return Err(Unmeasured::NoCost),
    };
    let held = &held;
    let default = EarnedValue::default();
    let settings = schedule
        .wbs
        .get(&a.wbs)
        .map_or(&default, |w| &w.earned_value);
    let inputs = Inputs {
        budget: held.budgeted_cost,
        planned_budget: plan.budget.unwrap_or(held.budgeted_cost),
        planned_fraction: planned_fraction(calendar, plan, schedule.project.data_date),
        actual_cost: held.actual_cost,
        remaining_cost: held.remaining_cost,
        percent_complete: percent::complete(a, Some(held)).unwrap_or(0.0),
        status: a.status,
    };
    totalfloat_evm::activity(&inputs, settings, reading).ok_or(Unmeasured::Technique)
}

fn money(value: f64) -> Value {
    Value::from((value * 100.0).round() / 100.0)
}

fn index(value: Option<f64>) -> Value {
    value.map_or(Value::Null, |v| Value::from((v * 1000.0).round() / 1000.0))
}

/// One of [`COLUMNS`].
pub(super) fn cell(column: &str, m: Option<&Measures>) -> Value {
    let Some(m) = m else {
        return Value::Null;
    };
    match column {
        "budget_at_completion" => money(m.budget_at_completion),
        "planned_value" => money(m.planned_value),
        "earned_value" => money(m.earned_value),
        "estimate_to_complete" => m.estimate_to_complete.map_or(Value::Null, money),
        "estimate_at_completion" => m.estimate_at_completion.map_or(Value::Null, money),
        "schedule_variance" => money(m.schedule_variance()),
        "cost_variance" => money(m.cost_variance()),
        "schedule_performance_index" => index(m.schedule_performance_index()),
        "cost_performance_index" => index(m.cost_performance_index()),
        _ => Value::Null,
    }
}

/// Earned value over every activity the query matched, before paging. With
/// `--wbs`, that's the node and everything under it.
#[derive(Debug, Serialize)]
pub(super) struct Total {
    /// Activities with earned value.
    pub(super) activities: usize,
    /// Activities left out because their WBS node's technique isn't one the
    /// model names. Their spending isn't in the totals either.
    pub(super) unmeasured: usize,
    pub(super) budget_at_completion: Value,
    pub(super) planned_value: Value,
    pub(super) earned_value: Value,
    pub(super) actual_cost: Value,
    pub(super) estimate_to_complete: Value,
    pub(super) estimate_at_completion: Value,
    pub(super) schedule_variance: Value,
    pub(super) cost_variance: Value,
    pub(super) schedule_performance_index: Value,
    pub(super) cost_performance_index: Value,
}

pub(super) fn total(measured: &[Measures], unmeasured: usize) -> Total {
    let t = totalfloat_evm::total(measured);
    Total {
        activities: measured.len(),
        unmeasured,
        budget_at_completion: money(t.budget_at_completion),
        planned_value: money(t.planned_value),
        earned_value: money(t.earned_value),
        actual_cost: money(t.actual_cost),
        estimate_to_complete: t.estimate_to_complete.map_or(Value::Null, money),
        estimate_at_completion: t.estimate_at_completion.map_or(Value::Null, money),
        schedule_variance: money(t.schedule_variance()),
        cost_variance: money(t.cost_variance()),
        schedule_performance_index: index(t.schedule_performance_index()),
        cost_performance_index: index(t.cost_performance_index()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime {
        text.parse().unwrap()
    }

    fn project() -> (Schedule, Activity) {
        let mut s = Schedule::new(totalfloat_model::Project::new(
            totalfloat_model::ProjectId(1),
            "P",
        ));
        s.project.data_date = Some(at("2026-01-10 00:00"));
        let mut a = Activity::new(
            totalfloat_model::ActivityId(1),
            totalfloat_model::ProjectId(1),
            totalfloat_model::WbsId(1),
            totalfloat_model::CalendarId(1),
            "A1".to_owned(),
            "Work".to_owned(),
        );
        a.planned_start = Some(at("2026-01-05 00:00"));
        a.planned_finish = Some(at("2026-01-15 00:00"));
        (s, a)
    }

    /// With a baseline, work it didn't plan has no planned value, and work it
    /// planned keeps its planned value after its assignments are gone.
    #[test]
    fn the_baseline_decides_what_was_planned() {
        let (s, a) = project();
        let mut costs = Totals::default();
        costs.budgeted_cost = 1000.0;
        let unplanned = Plan {
            budget: Some(0.0),
            ..own_plan(&a)
        };
        let m = measures(&s, &a, Some(&costs), (unplanned, None), UserFactor::Divide).unwrap();
        assert_eq!(m.planned_value, 0.0);
        // Half of the plan's ten days fall before the data date: 800 x 0.5.
        let planned = Plan {
            budget: Some(800.0),
            ..own_plan(&a)
        };
        let m = measures(&s, &a, None, (planned, None), UserFactor::Divide).unwrap();
        assert_eq!((m.planned_value, m.budget_at_completion), (400.0, 0.0));
        assert_eq!(
            measures(&s, &a, None, (own_plan(&a), None), UserFactor::Divide),
            Err(Unmeasured::NoCost)
        );
    }

    #[test]
    fn planned_value_is_the_share_of_the_plan_due_by_the_data_date() {
        let plan = Plan {
            start: Some(at("2026-01-05 00:00")),
            finish: Some(at("2026-01-15 00:00")),
            budget: Some(1000.0),
        };
        // Four of ten calendar days are due by the data date.
        let share = planned_fraction(None, plan, Some(at("2026-01-09 00:00")));
        assert!((share - 0.4).abs() < 1e-12);
        assert_eq!(
            planned_fraction(None, plan, Some(at("2026-01-01 00:00"))),
            0.0
        );
        assert_eq!(
            planned_fraction(None, plan, Some(at("2026-02-01 00:00"))),
            1.0
        );
        assert_eq!(
            planned_fraction(None, Plan::default(), Some(at("2026-01-09 00:00"))),
            0.0
        );
    }
}
