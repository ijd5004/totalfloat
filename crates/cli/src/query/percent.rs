//! Percent complete by the activity's type, and its assignments' units and
//! costs and its expenses, as `query` columns (TF-020).

use std::collections::BTreeMap;

use serde_json::Value;
use totalfloat_model::{
    Activity, ActivityId, ActivityStatus, PercentCompleteType, ResourceType, Schedule,
};

/// The columns this module fills.
pub(super) const COLUMNS: [&str; 16] = [
    "percent_complete",
    "percent_type",
    "duration_percent",
    "physical_percent",
    "units_percent",
    "budgeted_units",
    "actual_units",
    "remaining_units",
    "budgeted_cost",
    "actual_cost",
    "remaining_cost",
    "at_completion_cost",
    "budgeted_expense_cost",
    "actual_expense_cost",
    "remaining_expense_cost",
    "at_completion_expense_cost",
];

/// An activity's assignments and expenses added up. Units are labor and
/// nonlabor hours; material is in its own units, so it isn't added to them.
/// Costs are total costs, expenses included; the expense costs
/// are kept apart too.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Totals {
    budgeted_units: f64,
    actual_units: f64,
    remaining_units: f64,
    pub(super) budgeted_cost: f64,
    pub(super) actual_cost: f64,
    pub(super) remaining_cost: f64,
    budgeted_expense_cost: f64,
    actual_expense_cost: f64,
    remaining_expense_cost: f64,
    /// Whether any assignment holds units, so the units columns aren't empty.
    assigned: bool,
}

/// Each activity's totals, for the activities that have assignments or
/// expenses.
pub(super) fn totals(schedules: &[&Schedule]) -> BTreeMap<ActivityId, Totals> {
    let mut out: BTreeMap<ActivityId, Totals> = BTreeMap::new();
    for schedule in schedules {
        for x in schedule.expenses.values() {
            let held = out.entry(x.activity).or_default();
            let (budget, actual, remaining) = (
                x.budgeted_cost.unwrap_or(0.0),
                x.actual_cost.unwrap_or(0.0),
                x.remaining_cost.unwrap_or(0.0),
            );
            held.budgeted_expense_cost += budget;
            held.actual_expense_cost += actual;
            held.remaining_expense_cost += remaining;
            held.budgeted_cost += budget;
            held.actual_cost += actual;
            held.remaining_cost += remaining;
        }
        for assignment in schedule.assignments.values() {
            let a = &assignment.amounts;
            let held = out.entry(assignment.activity).or_default();
            held.assigned = true;
            // A role alone has no resource type, so its units aren't counted.
            let timed = assignment
                .resource
                .and_then(|r| schedule.resources.get(&r))
                .is_some_and(|r| r.kind != ResourceType::Material);
            if timed {
                held.budgeted_units += a.budgeted_units.unwrap_or(0.0);
                held.actual_units +=
                    a.actual_units.unwrap_or(0.0) + a.actual_overtime_units.unwrap_or(0.0);
                held.remaining_units += a.remaining_units.unwrap_or(0.0);
            }
            held.budgeted_cost += a.budgeted_cost.unwrap_or(0.0);
            held.actual_cost +=
                a.actual_cost.unwrap_or(0.0) + a.actual_overtime_cost.unwrap_or(0.0);
            held.remaining_cost += a.remaining_cost.unwrap_or(0.0);
        }
    }
    out
}

fn rounded(value: f64) -> Value {
    Value::from((value * 100.0).round() / 100.0)
}

/// Original less remaining, over original. Finished work is 100, and work with
/// no duration is 0 until it finishes.
#[allow(clippy::cast_precision_loss, reason = "minutes are far below 2^52")]
fn duration_percent(a: &Activity) -> f64 {
    if a.status == ActivityStatus::Completed {
        return 100.0;
    }
    let original = a.original_duration.get();
    if original <= 0 {
        return 0.0;
    }
    let done = (original - a.remaining_duration.get()) as f64 / original as f64;
    (done * 100.0).clamp(0.0, 100.0)
}

/// Actual units over actual plus remaining. With no units there's nothing to
/// measure, so it's empty rather than a misleading zero, unless finished.
fn units_percent(a: &Activity, totals: Option<&Totals>) -> Option<f64> {
    let held = totals.filter(|t| t.assigned)?;
    let at_completion = held.actual_units + held.remaining_units;
    if at_completion <= 0.0 {
        return (a.status == ActivityStatus::Completed).then_some(100.0);
    }
    Some((held.actual_units / at_completion * 100.0).clamp(0.0, 100.0))
}

const fn type_label(kind: PercentCompleteType) -> &'static str {
    match kind {
        PercentCompleteType::Physical => "physical",
        PercentCompleteType::Duration => "duration",
        PercentCompleteType::Units => "units",
    }
}

/// Percent complete by the activity's type: duration, physical or units.
pub(super) fn complete(a: &Activity, totals: Option<&Totals>) -> Option<f64> {
    match a.percent_complete_type {
        PercentCompleteType::Duration => Some(duration_percent(a)),
        PercentCompleteType::Physical => Some(a.physical_percent_complete.unwrap_or(0.0)),
        PercentCompleteType::Units => units_percent(a, totals),
    }
}

/// One of [`COLUMNS`] for an activity.
pub(super) fn cell(column: &str, a: &Activity, totals: Option<&Totals>) -> Value {
    let units = units_percent(a, totals);
    let money = |pick: fn(&Totals) -> f64| totals.map_or(Value::Null, |t| rounded(pick(t)));
    match column {
        "percent_complete" => complete(a, totals).map_or(Value::Null, rounded),
        "percent_type" => Value::from(type_label(a.percent_complete_type)),
        "duration_percent" => rounded(duration_percent(a)),
        "physical_percent" => a.physical_percent_complete.map_or(Value::Null, rounded),
        "units_percent" => units.map_or(Value::Null, rounded),
        "budgeted_units" | "actual_units" | "remaining_units"
            if totals.is_some_and(|t| !t.assigned) =>
        {
            Value::Null
        }
        "budgeted_units" => money(|t| t.budgeted_units),
        "actual_units" => money(|t| t.actual_units),
        "remaining_units" => money(|t| t.remaining_units),
        "budgeted_cost" => money(|t| t.budgeted_cost),
        "actual_cost" => money(|t| t.actual_cost),
        "remaining_cost" => money(|t| t.remaining_cost),
        "at_completion_cost" => money(|t| t.actual_cost + t.remaining_cost),
        "budgeted_expense_cost" => money(|t| t.budgeted_expense_cost),
        "actual_expense_cost" => money(|t| t.actual_expense_cost),
        "remaining_expense_cost" => money(|t| t.remaining_expense_cost),
        "at_completion_expense_cost" => money(|t| t.actual_expense_cost + t.remaining_expense_cost),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use totalfloat_model::{CalendarId, Minutes, ProjectId, WbsId};

    use super::*;

    fn activity(kind: PercentCompleteType, status: ActivityStatus, hours: (i64, i64)) -> Activity {
        let mut a = Activity::new(
            ActivityId(1),
            ProjectId(1),
            WbsId(1),
            CalendarId(1),
            "A1".to_owned(),
            "Work".to_owned(),
        );
        a.percent_complete_type = kind;
        a.status = status;
        a.original_duration = Minutes::new(hours.0 * 60);
        a.remaining_duration = Minutes::new(hours.1 * 60);
        a
    }

    fn quarter_done() -> Totals {
        Totals {
            budgeted_units: 40.0,
            actual_units: 10.0,
            remaining_units: 30.0,
            budgeted_cost: 4000.0,
            actual_cost: 1000.0,
            remaining_cost: 3000.0,
            assigned: true,
            ..Totals::default()
        }
    }

    #[test]
    fn each_type_picks_its_own_measure() {
        let held = quarter_done();
        let mut a = activity(
            PercentCompleteType::Duration,
            ActivityStatus::InProgress,
            (40, 20),
        );
        a.physical_percent_complete = Some(70.0);
        assert_eq!(cell("percent_complete", &a, Some(&held)), 50.0);
        a.percent_complete_type = PercentCompleteType::Physical;
        assert_eq!(cell("percent_complete", &a, Some(&held)), 70.0);
        a.percent_complete_type = PercentCompleteType::Units;
        assert_eq!(cell("percent_complete", &a, Some(&held)), 25.0);
        assert_eq!(cell("percent_type", &a, None), "units");
        assert_eq!(cell("at_completion_cost", &a, Some(&held)), 4000.0);
    }

    #[test]
    fn a_milestone_is_done_only_when_finished() {
        let waiting = activity(
            PercentCompleteType::Duration,
            ActivityStatus::NotStarted,
            (0, 0),
        );
        assert_eq!(cell("duration_percent", &waiting, None), 0.0);
        let done = activity(
            PercentCompleteType::Duration,
            ActivityStatus::Completed,
            (0, 0),
        );
        assert_eq!(cell("duration_percent", &done, None), 100.0);
    }

    #[test]
    fn no_assignments_means_no_units_or_costs_rather_than_zero() {
        let a = activity(
            PercentCompleteType::Units,
            ActivityStatus::InProgress,
            (40, 20),
        );
        for column in [
            "percent_complete",
            "units_percent",
            "budgeted_units",
            "actual_cost",
        ] {
            assert_eq!(cell(column, &a, None), Value::Null, "{column}");
        }
        // Assignments with no units: nothing to measure until it's finished.
        let empty = Totals {
            assigned: true,
            ..Totals::default()
        };
        assert_eq!(cell("units_percent", &a, Some(&empty)), Value::Null);
        let finished = activity(
            PercentCompleteType::Units,
            ActivityStatus::Completed,
            (40, 0),
        );
        assert_eq!(cell("units_percent", &finished, Some(&empty)), 100.0);
        // Only expenses: no units at all, finished or not.
        let expenses = Totals {
            budgeted_expense_cost: 500.0,
            ..Totals::default()
        };
        assert_eq!(
            cell("units_percent", &finished, Some(&expenses)),
            Value::Null
        );
        assert_eq!(
            cell("budgeted_units", &finished, Some(&expenses)),
            Value::Null
        );
    }

    #[test]
    fn remaining_beyond_original_is_zero_percent_not_negative() {
        let a = activity(
            PercentCompleteType::Duration,
            ActivityStatus::InProgress,
            (40, 60),
        );
        assert_eq!(cell("duration_percent", &a, None), 0.0);
    }
}
