//! Earned value for TotalFloat (`docs/rules/TF-041-earned-value.md`).
//!
//! Pure arithmetic on what a caller has already read or worked out: an
//! activity's budget, its actual and remaining cost, its percent complete, and
//! how much of its planned work falls before the data date. The WBS node's
//! settings say how value is earned and how the estimate to complete is made.
//! The formulas follow public earned value guidance, with the sources in the rule.

#![forbid(unsafe_code)]

use totalfloat_model::{ActivityStatus, EarnedValue, EarnedValueTechnique, EstimateMethod};

/// How a user's performance factor (`EE_PF_user`) applies to the budget left:
/// the `user_factor` option of `docs/rules/TF-041-earned-value.md`. A file
/// doesn't say which way its factor reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UserFactor {
    /// The budget left divided by the factor, an efficiency index, as every
    /// public estimate-at-completion formula divides by one.
    #[default]
    Divide,
    /// The budget left times the factor.
    Multiply,
}

/// What an activity's earned value is worked out from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inputs {
    /// Budget at completion: the budgeted cost of its assignments and expenses.
    pub budget: f64,
    /// The budget the plan held, from the baseline if there is one.
    pub planned_budget: f64,
    /// The share of planned work before the data date, from 0 to 1.
    pub planned_fraction: f64,
    /// Actual cost to date.
    pub actual_cost: f64,
    /// Remaining cost.
    pub remaining_cost: f64,
    /// Percent complete by the activity's percent complete type, 0 to 100.
    pub percent_complete: f64,
    /// Progress status.
    pub status: ActivityStatus,
}

/// Earned value measures for an activity, or totals for a group of them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Measures {
    /// Budget at completion.
    pub budget_at_completion: f64,
    /// Planned value: the planned budget for work due by the data date.
    pub planned_value: f64,
    /// Earned value: the budget for work done.
    pub earned_value: f64,
    /// Actual cost.
    pub actual_cost: f64,
    /// Estimate to complete. `None` with an estimate method the model doesn't
    /// name, a cost performance index of 0, or no user factor above zero.
    pub estimate_to_complete: Option<f64>,
    /// Estimate at completion: actual cost plus estimate to complete.
    pub estimate_at_completion: Option<f64>,
}

impl Measures {
    /// Earned less planned value. Negative is behind.
    #[must_use]
    pub fn schedule_variance(&self) -> f64 {
        self.earned_value - self.planned_value
    }

    /// Earned value less actual cost. Negative is over budget.
    #[must_use]
    pub fn cost_variance(&self) -> f64 {
        self.earned_value - self.actual_cost
    }

    /// Earned over planned value. `None` with nothing planned yet.
    #[must_use]
    pub fn schedule_performance_index(&self) -> Option<f64> {
        (self.planned_value > 0.0).then(|| self.earned_value / self.planned_value)
    }

    /// Earned value over actual cost. `None` with nothing spent yet.
    #[must_use]
    pub fn cost_performance_index(&self) -> Option<f64> {
        (self.actual_cost > 0.0).then(|| self.earned_value / self.actual_cost)
    }
}

/// The budget earned for the work done, or `None` for a technique the model
/// doesn't name. With no technique set, value is earned by percent complete.
fn earned(inputs: &Inputs, technique: Option<&EarnedValueTechnique>) -> Option<f64> {
    match technique {
        None | Some(EarnedValueTechnique::ActivityPercentComplete) => {
            let done = if inputs.status == ActivityStatus::Completed {
                100.0
            } else {
                inputs.percent_complete.clamp(0.0, 100.0)
            };
            Some(inputs.budget * done / 100.0)
        }
        Some(EarnedValueTechnique::Other(_)) => None,
    }
}

/// The estimate to complete by the node's method. With no method set, it's
/// the remaining cost. `None` where the method can't give one.
fn to_complete(
    inputs: &Inputs,
    earned: f64,
    settings: &EarnedValue,
    reading: UserFactor,
) -> Option<f64> {
    let left = (inputs.budget - earned).max(0.0);
    match &settings.estimate {
        None | Some(EstimateMethod::RemainingCost) => Some(inputs.remaining_cost),
        Some(EstimateMethod::PerformanceFactorOne) => Some(left),
        // Nothing spent yet: no index, so the factor is 1. Money spent and
        // nothing earned: an index of 0, which gives no estimate.
        Some(EstimateMethod::PerformanceFactorCpi) if inputs.actual_cost <= 0.0 => Some(left),
        Some(EstimateMethod::PerformanceFactorCpi) => {
            let cpi = earned / inputs.actual_cost;
            (cpi > 0.0).then(|| left / cpi)
        }
        // A factor the file doesn't give can't be guessed.
        Some(EstimateMethod::PerformanceFactorUser) => settings
            .user_factor
            .filter(|f| *f > 0.0)
            .map(|f| match reading {
                UserFactor::Divide => left / f,
                UserFactor::Multiply => left * f,
            }),
        Some(EstimateMethod::Other(_)) => None,
    }
}

/// An activity's earned value under its WBS node's settings, with a user's
/// performance factor read as `reading` says. `None` with a technique the model
/// doesn't name, because no value can be said to be earned.
#[must_use]
pub fn activity(inputs: &Inputs, settings: &EarnedValue, reading: UserFactor) -> Option<Measures> {
    let earned_value = earned(inputs, settings.technique.as_ref())?;
    let estimate_to_complete = to_complete(inputs, earned_value, settings, reading);
    Some(Measures {
        budget_at_completion: inputs.budget,
        planned_value: inputs.planned_budget * inputs.planned_fraction.clamp(0.0, 1.0),
        earned_value,
        actual_cost: inputs.actual_cost,
        estimate_to_complete,
        estimate_at_completion: estimate_to_complete.map(|etc| inputs.actual_cost + etc),
    })
}

/// Totals for a group, such as a WBS node: amounts add up, and indices come
/// from the totals. An estimate is only totalled if every member has one.
#[must_use]
pub fn total<'a>(members: impl IntoIterator<Item = &'a Measures>) -> Measures {
    let mut sum = Measures {
        estimate_to_complete: Some(0.0),
        estimate_at_completion: Some(0.0),
        ..Measures::default()
    };
    for m in members {
        sum.budget_at_completion += m.budget_at_completion;
        sum.planned_value += m.planned_value;
        sum.earned_value += m.earned_value;
        sum.actual_cost += m.actual_cost;
        sum.estimate_to_complete = sum
            .estimate_to_complete
            .zip(m.estimate_to_complete)
            .map(|(a, b)| a + b);
        sum.estimate_at_completion = sum
            .estimate_at_completion
            .zip(m.estimate_at_completion)
            .map(|(a, b)| a + b);
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1,000 budget, 40% done, with 500 spent, 700 left, and 60% of its
    /// planned work due by the data date.
    fn underway() -> Inputs {
        Inputs {
            budget: 1000.0,
            planned_budget: 1000.0,
            planned_fraction: 0.6,
            actual_cost: 500.0,
            remaining_cost: 700.0,
            percent_complete: 40.0,
            status: ActivityStatus::InProgress,
        }
    }

    fn settings(estimate: Option<EstimateMethod>) -> EarnedValue {
        EarnedValue {
            technique: Some(EarnedValueTechnique::ActivityPercentComplete),
            estimate,
            ..EarnedValue::default()
        }
    }

    #[test]
    fn percent_complete_earns_its_share_of_the_budget() {
        // EV = 1000 x 40% = 400. PV = 1000 x 0.6 = 600. AC = 500.
        let m = activity(&underway(), &settings(None), UserFactor::Divide).unwrap();
        assert_eq!(
            (m.earned_value, m.planned_value, m.actual_cost),
            (400.0, 600.0, 500.0)
        );
        // SV = 400 - 600 = -200, CV = 400 - 500 = -100.
        assert_eq!((m.schedule_variance(), m.cost_variance()), (-200.0, -100.0));
        // SPI = 400 / 600, CPI = 400 / 500 = 0.8.
        assert!((m.schedule_performance_index().unwrap() - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(m.cost_performance_index(), Some(0.8));
    }

    #[test]
    fn finished_work_has_earned_its_whole_budget() {
        let done = Inputs {
            status: ActivityStatus::Completed,
            percent_complete: 90.0,
            ..underway()
        };
        assert_eq!(
            activity(&done, &settings(None), UserFactor::Divide)
                .unwrap()
                .earned_value,
            1000.0
        );
    }

    #[test]
    fn each_estimate_method() {
        // EV 400, so 600 of budget is left.
        let etc = |method| {
            activity(&underway(), &settings(method), UserFactor::Divide)
                .unwrap()
                .estimate_to_complete
        };
        // Remaining cost, as stored, and the default with no method.
        assert_eq!(etc(Some(EstimateMethod::RemainingCost)), Some(700.0));
        assert_eq!(etc(None), Some(700.0));
        // A factor of 1: the budget left.
        assert_eq!(etc(Some(EstimateMethod::PerformanceFactorOne)), Some(600.0));
        // At CPI 0.8: 600 / 0.8 = 750.
        assert_eq!(etc(Some(EstimateMethod::PerformanceFactorCpi)), Some(750.0));
        // A user factor of 1.5, divided as the CPI is (TF-041 item 7): 600 / 1.5 = 400.
        let user = EarnedValue {
            estimate: Some(EstimateMethod::PerformanceFactorUser),
            user_factor: Some(1.5),
            ..settings(None)
        };
        let m = activity(&underway(), &user, UserFactor::Divide).unwrap();
        assert_eq!(m.estimate_to_complete, Some(400.0));
        // EAC = AC + ETC = 500 + 400.
        assert_eq!(m.estimate_at_completion, Some(900.0));
        // Read as a multiplier, the same factor gives 600 x 1.5 = 900.
        let m = activity(&underway(), &user, UserFactor::Multiply).unwrap();
        assert_eq!(m.estimate_to_complete, Some(900.0));
        assert_eq!(m.estimate_at_completion, Some(1400.0));
        // Only the user factor depends on the reading.
        assert_eq!(
            activity(
                &underway(),
                &settings(Some(EstimateMethod::PerformanceFactorCpi)),
                UserFactor::Multiply
            )
            .unwrap()
            .estimate_to_complete,
            Some(750.0)
        );
        assert_eq!(UserFactor::default(), UserFactor::Divide);
        assert_eq!(etc(Some(EstimateMethod::Other("EE_X".to_owned()))), None);
        // No user factor stored: no estimate, since none can be guessed.
        let missing = EarnedValue {
            estimate: Some(EstimateMethod::PerformanceFactorUser),
            ..settings(None)
        };
        assert_eq!(
            activity(&underway(), &missing, UserFactor::Divide)
                .unwrap()
                .estimate_to_complete,
            None
        );
    }

    /// 400 spent and nothing earned: CPI is 0, so there's no forecast to give.
    #[test]
    fn spending_with_nothing_earned_gives_no_estimate_by_cpi() {
        let stuck = Inputs {
            actual_cost: 400.0,
            percent_complete: 0.0,
            ..underway()
        };
        let m = activity(
            &stuck,
            &settings(Some(EstimateMethod::PerformanceFactorCpi)),
            UserFactor::Divide,
        )
        .unwrap();
        assert_eq!(m.cost_performance_index(), Some(0.0));
        assert_eq!(
            (m.estimate_to_complete, m.estimate_at_completion),
            (None, None)
        );
    }

    #[test]
    fn with_nothing_spent_the_cost_index_waits_and_its_factor_is_one() {
        let fresh = Inputs {
            actual_cost: 0.0,
            percent_complete: 0.0,
            planned_fraction: 0.0,
            status: ActivityStatus::NotStarted,
            ..underway()
        };
        let m = activity(
            &fresh,
            &settings(Some(EstimateMethod::PerformanceFactorCpi)),
            UserFactor::Divide,
        )
        .unwrap();
        assert_eq!(m.cost_performance_index(), None);
        assert_eq!(m.schedule_performance_index(), None);
        assert_eq!(m.estimate_to_complete, Some(1000.0));
    }

    #[test]
    fn an_unnamed_technique_earns_nothing_it_can_name() {
        let other = EarnedValue {
            technique: Some(EarnedValueTechnique::Other("EC_X".to_owned())),
            ..EarnedValue::default()
        };
        assert_eq!(activity(&underway(), &other, UserFactor::Divide), None);
    }

    #[test]
    fn totals_add_amounts_and_recompute_indices() {
        let a = activity(&underway(), &settings(None), UserFactor::Divide).unwrap();
        let b = activity(
            &Inputs {
                status: ActivityStatus::Completed,
                actual_cost: 1000.0,
                remaining_cost: 0.0,
                planned_fraction: 1.0,
                ..underway()
            },
            &settings(None),
            UserFactor::Divide,
        )
        .unwrap();
        // EV 400 + 1000, PV 600 + 1000, AC 500 + 1000.
        let t = total([&a, &b]);
        assert_eq!(
            (t.earned_value, t.planned_value, t.actual_cost),
            (1400.0, 1600.0, 1500.0)
        );
        assert_eq!(t.estimate_at_completion, Some(500.0 + 700.0 + 1000.0));
        assert!((t.cost_performance_index().unwrap() - 1400.0 / 1500.0).abs() < 1e-12);
        let unknown = Measures {
            estimate_to_complete: None,
            ..a
        };
        assert_eq!(total([&a, &unknown]).estimate_to_complete, None);
    }
}
