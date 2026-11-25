//! Keeping an assignment's units and costs in step with its activity's
//! durations, by the activity's duration type (TF-038).
//!
//! A duration change moves either the units or the units per hour: the two
//! "units per time" types keep the rate and change the units, and the two
//! "units" types keep the units and change the rate. Work not started keeps its
//! remaining values equal to its budget, as every P6 export in the corpus does.
//! A linked cost scales with its units, so whatever price the file used stays.
//! Actual units and costs are never changed.

use std::fmt::Write as _;

use totalfloat_model::{
    Activity, ActivityId, ActivityStatus, Amounts, DurationType, Minutes, PriceSource, RateType,
    ResourceId, ResourceType, RoleId, Schedule,
};

/// Six decimals for units and rates, and four for costs, as P6 stores them.
pub(crate) fn round(value: f64, places: i32) -> f64 {
    let scale = 10_f64.powi(places);
    (value * scale).round() / scale
}

#[allow(clippy::cast_precision_loss, reason = "minutes are far below 2^52")]
pub(crate) fn hours(minutes: Minutes) -> f64 {
    minutes.get() as f64 / 60.0
}

/// True if a duration change keeps units per hour and moves the units.
const fn keeps_rate(kind: DurationType) -> bool {
    matches!(
        kind,
        DurationType::FixedUnitsPerTime | DurationType::FixedDurationAndUnitsPerTime
    )
}

/// Units and their rate after their span went from `was` to `span`. Units that
/// keep their rate scale with the span, which is the same as rate times hours
/// when the stored values agree, and stays exact when the stored rate was rounded.
fn follow(
    (units, rate): (Option<f64>, Option<f64>),
    (was, span): (Minutes, Minutes),
    rate_stays: bool,
) -> (Option<f64>, Option<f64>) {
    let (before, time) = (hours(was), hours(span));
    if time <= 0.0 {
        // Nothing left to spread over: no units, and the rate stays for later.
        return (units.map(|_| 0.0), rate);
    }
    if rate_stays {
        let scaled = units
            .filter(|_| before > 0.0)
            .map(|u| round(u * time / before, 6));
        (
            scaled
                .or_else(|| rate.map(|r| round(r * time, 6)))
                .or(units),
            rate,
        )
    } else {
        (units, units.map(|u| round(u / time, 6)).or(rate))
    }
}

/// A linked cost after its units moved from `was` to `now`.
fn scaled(
    cost: Option<f64>,
    (was, now): (Option<f64>, Option<f64>),
    price: Option<f64>,
) -> Option<f64> {
    let (Some(was), Some(now)) = (was, now) else {
        return cost;
    };
    // Exactly the same units, so the stored cost stands as written.
    if was.to_bits() == now.to_bits() {
        return cost;
    }
    // The cost per unit the file used, which can differ from today's price.
    let per_unit = cost.filter(|_| was.abs() > 0.0).map(|c| c / was).or(price);
    per_unit.map(|p| round(p * now, 4)).or(cost)
}

/// The amounts after the activity's durations changed.
fn followed(a: Amounts, activity: &Activity, was: (Minutes, Minutes)) -> Amounts {
    let rate_stays = keeps_rate(activity.duration_type);
    let (original, remaining) = (activity.original_duration, activity.remaining_duration);
    let mut next = a;
    if original != was.0 {
        (next.budgeted_units, next.budgeted_units_per_hour) = follow(
            (a.budgeted_units, a.budgeted_units_per_hour),
            (was.0, original),
            rate_stays,
        );
    }
    // Work not started keeps remaining equal to its budget while the two
    // durations agree, as every export does (TF-038).
    let as_budgeted = activity.status == ActivityStatus::NotStarted && remaining == original;
    if as_budgeted {
        next.remaining_units = next.budgeted_units.or(a.remaining_units);
        next.remaining_units_per_hour = next.budgeted_units_per_hour.or(a.remaining_units_per_hour);
    } else if remaining != was.1 {
        (next.remaining_units, next.remaining_units_per_hour) = follow(
            (a.remaining_units, a.remaining_units_per_hour),
            (was.1, remaining),
            rate_stays,
        );
    }
    if a.cost_follows_units {
        let price = a.price_per_unit;
        next.budgeted_cost = scaled(
            a.budgeted_cost,
            (a.budgeted_units, next.budgeted_units),
            price,
        );
        next.remaining_cost = if as_budgeted {
            next.budgeted_cost.or(a.remaining_cost)
        } else {
            scaled(
                a.remaining_cost,
                (a.remaining_units, next.remaining_units),
                price,
            )
        };
    }
    next
}

fn number(value: Option<f64>) -> String {
    value.map_or_else(|| "none".to_owned(), |v| v.to_string())
}

/// What moved on one assignment, or nothing.
fn moved(was: &Amounts, now: &Amounts) -> String {
    let mut said = Vec::new();
    for (name, before, after) in [
        ("budgeted units", was.budgeted_units, now.budgeted_units),
        ("actual units", was.actual_units, now.actual_units),
        ("remaining units", was.remaining_units, now.remaining_units),
        (
            "budgeted units per hour",
            was.budgeted_units_per_hour,
            now.budgeted_units_per_hour,
        ),
        (
            "remaining units per hour",
            was.remaining_units_per_hour,
            now.remaining_units_per_hour,
        ),
        ("budgeted cost", was.budgeted_cost, now.budgeted_cost),
        ("actual cost", was.actual_cost, now.actual_cost),
        ("remaining cost", was.remaining_cost, now.remaining_cost),
    ] {
        if before != after {
            said.push(format!("{name} {} to {}", number(before), number(after)));
        }
    }
    said.join(", ")
}

/// Whether an assignment's units count toward units percent complete: those of
/// a labor or nonlabor resource, which are hours. A material's units are in its
/// own unit, and a role alone has no resource type (docs/rules/TF-004-units-percent-complete.md).
pub(crate) fn counts_hours(schedule: &Schedule, resource: Option<ResourceId>) -> bool {
    resource
        .and_then(|r| schedule.resources.get(&r))
        .is_some_and(|r| r.kind != ResourceType::Material)
}

/// Units percent complete: the actual units of the assignments that count,
/// over their actual and remaining units. With no units to measure, finished
/// work is 100 and other work `None`.
pub(crate) fn units_percent(schedule: &Schedule, id: ActivityId) -> Option<f64> {
    let (done, all) = schedule
        .assignments
        .values()
        .filter(|a| a.activity == id && counts_hours(schedule, a.resource))
        .fold((0.0, 0.0), |(done, all), a| {
            let actual = a.amounts.actual_units.unwrap_or(0.0)
                + a.amounts.actual_overtime_units.unwrap_or(0.0);
            (
                done + actual,
                all + actual + a.amounts.remaining_units.unwrap_or(0.0),
            )
        });
    let finished = schedule
        .activities
        .get(&id)
        .is_some_and(|a| a.status == ActivityStatus::Completed);
    if all > 0.0 {
        Some((done / all * 100.0).clamp(0.0, 100.0))
    } else {
        finished.then_some(100.0)
    }
}

/// Remaining units per hour over the remaining duration, if there is any.
fn per_remaining_hour(units: Option<f64>, remaining: Minutes) -> Option<f64> {
    let time = hours(remaining);
    units.filter(|_| time > 0.0).map(|u| round(u / time, 6))
}

/// One assignment at `percent` complete by units: its units at completion
/// (actual, overtime included, plus remaining) stay, and that share of them is
/// actual. A linked cost at completion splits the same way.
fn at_percent(a: Amounts, percent: f64, remaining: Minutes) -> Amounts {
    let share = percent / 100.0;
    let split = |actual: Option<f64>, overtime: Option<f64>, left: Option<f64>, places| {
        let overtime = overtime.unwrap_or(0.0);
        let total = actual.unwrap_or(0.0) + overtime + left.unwrap_or(0.0);
        let done = round(total * share, places).max(overtime);
        (round(done - overtime, places), round(total - done, places))
    };
    let total_units = a.actual_units.unwrap_or(0.0)
        + a.actual_overtime_units.unwrap_or(0.0)
        + a.remaining_units.unwrap_or(0.0);
    if total_units <= 0.0 {
        return a;
    }
    let mut next = a;
    let (actual, left) = split(
        a.actual_units,
        a.actual_overtime_units,
        a.remaining_units,
        6,
    );
    next.actual_units = Some(actual);
    next.remaining_units = Some(left);
    next.remaining_units_per_hour =
        per_remaining_hour(next.remaining_units, remaining).or(a.remaining_units_per_hour);
    let costed = [a.actual_cost, a.actual_overtime_cost, a.remaining_cost]
        .iter()
        .any(Option::is_some);
    if a.cost_follows_units && costed {
        let (actual, left) = split(a.actual_cost, a.actual_overtime_cost, a.remaining_cost, 4);
        next.actual_cost = Some(actual);
        next.remaining_cost = Some(left);
    }
    next
}

/// Sets each counted assignment of an activity measured by units percent to
/// `percent` complete, and says what moved, one clause per assignment.
pub(crate) fn follow_units_percent(
    schedule: &mut Schedule,
    id: ActivityId,
    percent: f64,
) -> Vec<String> {
    let Some(remaining) = schedule.activities.get(&id).map(|a| a.remaining_duration) else {
        return Vec::new();
    };
    let counted: Vec<_> = schedule
        .assignments
        .values()
        .filter(|a| a.activity == id && counts_hours(schedule, a.resource))
        .map(|a| (a.id, a.resource, a.role))
        .collect();
    let mut said = Vec::new();
    for (assignment, resource, role) in counted {
        let Some(held) = schedule.assignments.get_mut(&assignment) else {
            continue;
        };
        let before = held.amounts;
        held.amounts = at_percent(before, percent, remaining);
        let after = held.amounts;
        said.extend(said_moved(schedule, (resource, role), (&before, &after)));
    }
    said
}

/// An assignment's amounts with new actual or remaining units. The remaining
/// rate follows the remaining units over the remaining duration, and a linked
/// cost follows its units at the cost per unit it had.
pub(crate) fn with_units(
    a: Amounts,
    (actual, left): (Option<f64>, Option<f64>),
    (original, remaining): (Minutes, Minutes),
    not_started: bool,
) -> Amounts {
    let mut next = a;
    if let Some(units) = actual {
        next.actual_units = Some(units);
    }
    if let Some(units) = left {
        next.remaining_units = Some(units);
        next.remaining_units_per_hour =
            per_remaining_hour(Some(units), remaining).or(a.remaining_units_per_hour);
        // Work not started keeps its budget equal to its remaining (TF-038).
        if not_started {
            next.budgeted_units = Some(units);
            next.budgeted_units_per_hour =
                per_remaining_hour(Some(units), original).or(a.budgeted_units_per_hour);
        }
    }
    if a.cost_follows_units {
        // With no actual cost yet, actual units cost what the rest costs.
        let per_unit = |cost: Option<f64>, units: Option<f64>| {
            cost.zip(units)
                .filter(|(_, u)| *u > 0.0)
                .map(|(c, u)| c / u)
        };
        let price = a
            .price_per_unit
            .or_else(|| per_unit(a.remaining_cost, a.remaining_units))
            .or_else(|| per_unit(a.budgeted_cost, a.budgeted_units));
        // Units not held before count as none, so new ones are priced.
        let was = |units: Option<f64>| Some(units.unwrap_or(0.0));
        next.actual_cost = scaled(
            a.actual_cost,
            (was(a.actual_units), next.actual_units),
            price,
        );
        next.remaining_cost = scaled(
            a.remaining_cost,
            (was(a.remaining_units), next.remaining_units),
            price,
        );
        if not_started {
            next.budgeted_cost = scaled(
                a.budgeted_cost,
                (was(a.budgeted_units), next.budgeted_units),
                price,
            );
        }
    }
    next
}

/// What moved on one assignment, said as `NAME: ...`, or nothing.
pub(crate) fn said_moved(
    schedule: &Schedule,
    (resource, role): (Option<ResourceId>, Option<RoleId>),
    (was, now): (&Amounts, &Amounts),
) -> Option<String> {
    let line = moved(was, now);
    (!line.is_empty()).then(|| format!("{}: {line}", assignee(schedule, resource, role)))
}

/// Moves the units and costs of an activity's assignments after its durations
/// changed from `was`, and says what moved, one clause per assignment.
pub(crate) fn follow_durations(
    schedule: &mut Schedule,
    id: ActivityId,
    was: (Minutes, Minutes),
    skip_counted: bool,
) -> Vec<String> {
    let Some(activity) = schedule.activities.get(&id) else {
        return Vec::new();
    };
    if (activity.original_duration, activity.remaining_duration) == was {
        return Vec::new();
    }
    let activity = activity.clone();
    let mut said = Vec::new();
    let names: Vec<_> = schedule
        .assignments
        .values()
        .filter(|a| a.activity == id && !(skip_counted && counts_hours(schedule, a.resource)))
        .map(|a| (a.id, a.resource, a.role))
        .collect();
    for (assignment, resource, role) in names {
        let Some(held) = schedule.assignments.get_mut(&assignment) else {
            continue;
        };
        let before = held.amounts;
        held.amounts = followed(before, &activity, was);
        let line = moved(&before, &held.amounts);
        if !line.is_empty() {
            let name = assignee(schedule, resource, role);
            said.push(format!("{name}: {line}"));
        }
    }
    said
}

/// A new assignment's amounts: `units` if given, otherwise the resource's
/// default units per hour, spread over the activity's durations.
pub(crate) fn new_amounts(
    activity: &Activity,
    units: Option<f64>,
    default_rate: Option<f64>,
    cost_follows_units: bool,
) -> Amounts {
    let (original, remaining) = (
        hours(activity.original_duration),
        hours(activity.remaining_duration),
    );
    let per_hour = |total: f64, span: f64| (span > 0.0).then(|| round(total / span, 6));
    let (budgeted, remaining_units) = match (units, default_rate) {
        (Some(units), _) => (units, units),
        (None, Some(rate)) => (round(rate * original, 6), round(rate * remaining, 6)),
        (None, None) => {
            return Amounts {
                cost_follows_units,
                ..Amounts::default()
            };
        }
    };
    let not_started = activity.status == ActivityStatus::NotStarted;
    let budgeted = if not_started {
        budgeted
    } else {
        budgeted.max(remaining_units)
    };
    Amounts {
        budgeted_units: Some(budgeted),
        remaining_units: Some(remaining_units),
        budgeted_units_per_hour: per_hour(budgeted, original),
        remaining_units_per_hour: per_hour(remaining_units, remaining),
        actual_units: Some(0.0),
        cost_follows_units,
        ..Amounts::default()
    }
}

/// A new assignment at the resource's first price. A cost linked to its units
/// is units times that price; without a price, the cost stays empty.
pub(crate) fn priced(mut amounts: Amounts, price: Option<f64>) -> Amounts {
    let Some(price) = price else {
        return amounts;
    };
    amounts.price_per_unit = Some(price);
    amounts.price_source = Some(PriceSource::Resource);
    amounts.rate_type = Some(RateType::Price1);
    if amounts.cost_follows_units && amounts.remaining_units.is_some() {
        let cost = |units: Option<f64>| units.map(|u| round(u * price, 4));
        amounts.budgeted_cost = cost(amounts.budgeted_units);
        amounts.remaining_cost = cost(amounts.remaining_units);
        amounts.actual_cost = Some(0.0);
    }
    amounts
}

/// Says what a new assignment holds, for the edit's line.
pub(crate) fn describe(amounts: &Amounts) -> String {
    let mut line = String::new();
    match amounts.remaining_units {
        Some(units) => {
            let _ = write!(line, "{units} units");
            if let Some(rate) = amounts.remaining_units_per_hour {
                let _ = write!(line, " at {rate} an hour");
            }
        }
        None => line.push_str("no units"),
    }
    if let Some(price) = amounts.price_per_unit {
        let _ = write!(line, ", at {price} a unit");
    }
    line
}

/// What an assignment is to: its resource's short name, or for a role alone
/// the role's, or the ID where neither is known.
pub(crate) fn assignee(
    schedule: &Schedule,
    resource: Option<ResourceId>,
    role: Option<RoleId>,
) -> String {
    match (resource, role) {
        (Some(id), _) => schedule
            .resources
            .get(&id)
            .map_or_else(|| id.to_string(), |r| r.short_name.clone()),
        (None, Some(id)) => schedule.roles.get(&id).map_or_else(
            || format!("role {id}"),
            |r| format!("role {}", r.short_name),
        ),
        (None, None) => "no one".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use totalfloat_model::{CalendarId, ProjectId, WbsId};

    use super::*;

    fn activity(kind: DurationType, status: ActivityStatus, hours: (i64, i64)) -> Activity {
        let mut a = Activity::new(
            ActivityId(1),
            ProjectId(1),
            WbsId(1),
            CalendarId(1),
            "A1".to_owned(),
            "Work".to_owned(),
        );
        a.duration_type = kind;
        a.status = status;
        a.original_duration = Minutes::new(hours.0 * 60);
        a.remaining_duration = Minutes::new(hours.1 * 60);
        a
    }

    /// 40 hours at 2 an hour, priced at 50.
    fn forty() -> Amounts {
        Amounts {
            budgeted_units: Some(80.0),
            remaining_units: Some(80.0),
            budgeted_units_per_hour: Some(2.0),
            remaining_units_per_hour: Some(2.0),
            actual_units: Some(0.0),
            budgeted_cost: Some(4000.0),
            remaining_cost: Some(4000.0),
            actual_cost: Some(0.0),
            price_per_unit: Some(50.0),
            cost_follows_units: true,
            ..Amounts::default()
        }
    }

    #[test]
    fn units_per_time_types_keep_the_rate_and_move_the_units() {
        for kind in [
            DurationType::FixedUnitsPerTime,
            DurationType::FixedDurationAndUnitsPerTime,
        ] {
            let a = activity(kind, ActivityStatus::NotStarted, (50, 50));
            let next = followed(forty(), &a, (Minutes::new(40 * 60), Minutes::new(40 * 60)));
            assert_eq!(next.budgeted_units, Some(100.0), "{kind:?}");
            assert_eq!(next.remaining_units, Some(100.0));
            assert_eq!(next.remaining_units_per_hour, Some(2.0));
            assert_eq!(next.budgeted_cost, Some(5000.0));
            assert_eq!(next.remaining_cost, Some(5000.0));
        }
    }

    #[test]
    fn units_types_keep_the_units_and_move_the_rate() {
        for kind in [
            DurationType::FixedUnits,
            DurationType::FixedDurationAndUnits,
        ] {
            let a = activity(kind, ActivityStatus::NotStarted, (50, 50));
            let next = followed(forty(), &a, (Minutes::new(40 * 60), Minutes::new(40 * 60)));
            assert_eq!(next.budgeted_units, Some(80.0), "{kind:?}");
            assert_eq!(next.remaining_units_per_hour, Some(1.6));
            assert_eq!(next.budgeted_cost, Some(4000.0));
        }
    }

    #[test]
    fn work_underway_moves_only_what_remains_and_never_its_actuals() {
        let mut held = forty();
        held.actual_units = Some(20.0);
        held.actual_cost = Some(1000.0);
        held.remaining_units = Some(60.0);
        held.remaining_cost = Some(3000.0);
        let a = activity(
            DurationType::FixedUnitsPerTime,
            ActivityStatus::InProgress,
            (40, 20),
        );
        let next = followed(held, &a, (Minutes::new(40 * 60), Minutes::new(30 * 60)));
        assert_eq!(next.remaining_units, Some(40.0));
        assert_eq!(next.remaining_cost, Some(2000.0));
        assert_eq!(
            (next.actual_units, next.actual_cost),
            (Some(20.0), Some(1000.0))
        );
        assert_eq!(
            (next.budgeted_units, next.budgeted_cost),
            (Some(80.0), Some(4000.0))
        );
    }

    /// A file can store a rounded rate: 10 units over 143 hours shows 0.07.
    #[test]
    fn units_scale_with_the_duration_even_when_the_rate_was_rounded() {
        let mut held = forty();
        held.budgeted_units = Some(10.0);
        held.remaining_units = Some(10.0);
        held.budgeted_units_per_hour = Some(0.07);
        held.remaining_units_per_hour = Some(0.07);
        let a = activity(
            DurationType::FixedUnitsPerTime,
            ActivityStatus::NotStarted,
            (286, 286),
        );
        let next = followed(held, &a, (Minutes::new(143 * 60), Minutes::new(143 * 60)));
        assert_eq!(next.budgeted_units, Some(20.0));
        assert_eq!(next.remaining_units, Some(20.0));
    }

    /// Remaining set apart from the original on work not started.
    #[test]
    fn a_remaining_duration_of_its_own_moves_only_what_remains() {
        let a = activity(
            DurationType::FixedUnitsPerTime,
            ActivityStatus::NotStarted,
            (40, 20),
        );
        let next = followed(forty(), &a, (Minutes::new(40 * 60), Minutes::new(40 * 60)));
        assert_eq!(next.budgeted_units, Some(80.0));
        assert_eq!(next.remaining_units, Some(40.0));
        assert_eq!(next.remaining_units_per_hour, Some(2.0));
        assert_eq!(next.remaining_cost, Some(2000.0));
        assert_eq!(next.budgeted_cost, Some(4000.0));
    }

    #[test]
    fn a_budgeted_rate_alone_is_still_reported() {
        let a = activity(
            DurationType::FixedUnits,
            ActivityStatus::InProgress,
            (50, 40),
        );
        let next = followed(forty(), &a, (Minutes::new(40 * 60), Minutes::new(40 * 60)));
        assert_eq!(next.budgeted_units_per_hour, Some(1.6));
        assert!(moved(&forty(), &next).contains("budgeted units per hour 2 to 1.6"));
    }

    #[test]
    fn finishing_leaves_no_remaining_units() {
        let a = activity(DurationType::FixedUnits, ActivityStatus::Completed, (40, 0));
        let next = followed(forty(), &a, (Minutes::new(40 * 60), Minutes::new(40 * 60)));
        assert_eq!(next.remaining_units, Some(0.0));
        assert_eq!(next.remaining_cost, Some(0.0));
        assert_eq!(next.remaining_units_per_hour, Some(2.0));
    }

    /// TF-038: a cost the file worked out at another price keeps that price.
    #[test]
    fn a_cost_keeps_the_price_the_file_used() {
        let mut held = forty();
        held.price_per_unit = Some(10.0);
        let a = activity(
            DurationType::FixedUnitsPerTime,
            ActivityStatus::NotStarted,
            (20, 20),
        );
        let next = followed(held, &a, (Minutes::new(40 * 60), Minutes::new(40 * 60)));
        assert_eq!(next.budgeted_cost, Some(2000.0));
    }

    #[test]
    fn an_unlinked_cost_stays() {
        let mut held = forty();
        held.cost_follows_units = false;
        let a = activity(
            DurationType::FixedUnitsPerTime,
            ActivityStatus::NotStarted,
            (20, 20),
        );
        let next = followed(held, &a, (Minutes::new(40 * 60), Minutes::new(40 * 60)));
        assert_eq!(next.budgeted_units, Some(40.0));
        assert_eq!(next.budgeted_cost, Some(4000.0));
    }

    #[test]
    fn a_new_assignment_spreads_its_units_or_the_default_rate() {
        let a = activity(
            DurationType::FixedDurationAndUnits,
            ActivityStatus::NotStarted,
            (40, 40),
        );
        let given = new_amounts(&a, Some(20.0), Some(1.0), true);
        assert_eq!(given.remaining_units, Some(20.0));
        assert_eq!(given.remaining_units_per_hour, Some(0.5));
        assert!(given.cost_follows_units);
        let by_rate = new_amounts(&a, None, Some(1.0), false);
        assert_eq!(
            (by_rate.budgeted_units, by_rate.remaining_units),
            (Some(40.0), Some(40.0))
        );
        let none = new_amounts(&a, None, None, false);
        assert_eq!(none.remaining_units, None);
        assert_eq!(describe(&given), "20 units at 0.5 an hour");
        assert_eq!(describe(&none), "no units");
    }
}
