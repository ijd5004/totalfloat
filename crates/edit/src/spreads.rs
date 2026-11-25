//! How an assignment's units spread over time: evenly, by a resource curve,
//! or by units typed in for each period (TF-040). The three are alternatives,
//! so choosing one clears the others.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use totalfloat_calendar::WorkCalendar;
use totalfloat_model::{
    ActivityStatus, ActivityType, AssignmentId, CalendarId, Date, Minutes, ResourceCurveId,
    Schedule, SpreadSegment, TimeOfDay, TypedSpread,
};

use crate::units::{hours, round};
use crate::{EditError, find, no_resource, parse_day, refuse, resource_named};

/// The periods typed units are given for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SpreadPeriod {
    /// Calendar days.
    Day,
    /// Seven days, from the first period's first day.
    Week,
    /// Calendar months, from the first of a month.
    Month,
}

impl SpreadPeriod {
    /// Every period, with the word flags use for it.
    pub const WORDS: [(&'static str, Self); 3] = [
        ("day", Self::Day),
        ("week", Self::Week),
        ("month", Self::Month),
    ];

    /// The period a word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::WORDS
            .iter()
            .find(|(w, _)| w.eq_ignore_ascii_case(word))
            .map(|(_, p)| *p)
    }

    /// The first day of the period after the one beginning on `first`.
    fn next(self, first: Date) -> Option<Date> {
        match self {
            Self::Day => first.checked_add_days(1),
            Self::Week => first.checked_add_days(7),
            Self::Month => {
                let (year, month, _) = first.ymd();
                if month == 12 {
                    Date::from_ymd(year.checked_add(1)?, 1, 1)
                } else {
                    Date::from_ymd(year, month + 1, 1)
                }
            }
        }
    }
}

/// The resource curve a name means here, ignoring case.
#[must_use]
pub fn curve_named(schedule: &Schedule, wanted: &str) -> Option<ResourceCurveId> {
    let lower = wanted.to_lowercase();
    let mut found = schedule
        .resource_curves
        .values()
        .filter(|c| c.name.to_lowercase() == lower);
    match (found.next(), found.next()) {
        (Some(one), None) => Some(one.id),
        _ => None,
    }
}

/// The one assignment of this resource on this activity.
fn assignment_of(
    schedule: &Schedule,
    (code, wanted): (&str, &str),
) -> Result<AssignmentId, EditError> {
    let activity = find(schedule, code)?;
    let resource = resource_named(schedule, wanted).ok_or_else(|| no_resource(wanted))?;
    let mut held = schedule
        .assignments
        .values()
        .filter(|a| a.activity == activity && a.resource == Some(resource));
    match (held.next(), held.next()) {
        (Some(one), None) => Ok(one.id),
        (None, _) => refuse(
            "not-assigned",
            format!("{code} doesn't have {wanted}"),
            "`query --columns code,resources` lists assignments",
        ),
        (Some(_), Some(_)) => refuse(
            "ambiguous-assignment",
            format!("{code} has {wanted} more than once"),
            "change one of them in P6",
        ),
    }
}

/// Spreads an assignment's units by a curve, or evenly with none, and clears
/// any typed spread.
pub(crate) fn set_curve(
    schedule: &mut Schedule,
    (code, wanted): (&str, &str),
    curve: Option<&str>,
) -> Result<String, EditError> {
    let id = assignment_of(schedule, (code, wanted))?;
    let has_typed_actuals = schedule.assignments[&id]
        .typed_spread
        .as_ref()
        .is_some_and(|t| !t.actual.is_empty());
    if has_typed_actuals {
        return refuse(
            "typed-actuals",
            format!("{code}'s {wanted} has actual units typed by period, which a curve would drop"),
            "type the remaining units with a spread instead",
        );
    }
    let chosen = match curve {
        None => None,
        Some(name) => Some(curve_named(schedule, name).ok_or_else(|| {
            let names: Vec<&str> = schedule
                .resource_curves
                .values()
                .map(|c| c.name.as_str())
                .collect();
            EditError {
                code: "no-such-curve",
                message: format!("no one resource curve is named {name}"),
                hint: if names.is_empty() {
                    "the file has no resource curves; leave the curve out to spread evenly"
                        .to_owned()
                } else {
                    format!(
                        "the file's curves: {}; leave the curve out to spread evenly",
                        names.join(", ")
                    )
                },
            }
        })?),
    };
    let named = chosen.and_then(|c| schedule.resource_curves.get(&c).map(|c| c.name.clone()));
    if let Some(assignment) = schedule.assignments.get_mut(&id) {
        assignment.curve = chosen;
        assignment.typed_spread = None;
    }
    Ok(match named {
        Some(name) => format!("{code}'s {wanted} now spreads by {name}"),
        None => format!("{code}'s {wanted} now spreads evenly"),
    })
}

/// Units typed for consecutive periods from a first day.
pub(crate) struct Typed<'a> {
    pub period: SpreadPeriod,
    pub start: &'a str,
    pub units: &'a [f64],
}

/// Sets an assignment's remaining units per period, in place of a curve.
pub(crate) fn set_spread(
    schedule: &mut Schedule,
    (code, wanted): (&str, &str),
    typed: &Typed<'_>,
) -> Result<String, EditError> {
    let id = assignment_of(schedule, (code, wanted))?;
    let activity = schedule.assignments[&id].activity;
    if schedule.activities[&activity].status == ActivityStatus::Completed {
        return refuse(
            "finished-work",
            format!("{code} is finished, so it has no remaining units to spread"),
            "a curve or an even spread can still be chosen",
        );
    }
    if typed.units.is_empty() {
        return refuse(
            "invalid-spread",
            "a spread needs units for at least one period",
            "give the units for each period, in order",
        );
    }
    if let Some(bad) = typed.units.iter().find(|u| !u.is_finite() || **u < 0.0) {
        return refuse(
            "invalid-units",
            format!("{bad} units can't be spread"),
            "give zero or more units for each period",
        );
    }
    let first = parse_day(typed.start)?;
    if typed.period == SpreadPeriod::Month && first.ymd().2 != 1 {
        return refuse(
            "invalid-spread",
            format!("months start on the first, not on {}", typed.start),
            "start the first month on its first day",
        );
    }
    let segments = segments(schedule, id, (code, wanted), first, typed)?;
    let assignment = &schedule.assignments[&id];
    let remaining = assignment.amounts.remaining_units.unwrap_or(0.0);
    let sum: f64 = typed.units.iter().sum();
    // The file's reader allows this much per stretch it holds.
    #[allow(clippy::cast_precision_loss, reason = "a spread has few periods")]
    let slack = 0.0005 * segments.len() as f64 + 1e-9 * remaining.abs();
    if (sum - remaining).abs() > slack {
        let (more, by) = if sum > remaining {
            ("more", sum - remaining)
        } else {
            ("less", remaining - sum)
        };
        return refuse(
            "spread-units",
            format!(
                "the periods hold {}, {} {more} than {code}'s {} remaining for {wanted}",
                round(sum, 3),
                round(by, 3),
                round(remaining, 3)
            ),
            "make the periods add up to the remaining units; changing units is a separate edit",
        );
    }
    let started = schedule.activities[&assignment.activity].status != ActivityStatus::NotStarted;
    let before = assignment.typed_spread.clone().unwrap_or_default();
    // Every corpus file has the planned spread equal to the remaining one
    // until work starts; after that the plan stays as it was, or absent
    // (TF-039): the remaining units aren't the budgeted ones.
    let planned = if started {
        before.planned
    } else {
        segments.clone()
    };
    if let Some(assignment) = schedule.assignments.get_mut(&id) {
        assignment.curve = None;
        assignment.typed_spread = Some(TypedSpread {
            planned,
            remaining: segments,
            actual: before.actual,
        });
    }
    let count = typed.units.len();
    let word = SpreadPeriod::WORDS
        .iter()
        .find(|(_, p)| *p == typed.period)
        .map_or("period", |(w, _)| w);
    let plural = if count == 1 { "" } else { "s" };
    Ok(format!(
        "{code}'s {wanted} now spreads by {count} typed {word}{plural} from {first}"
    ))
}

/// The typed periods as stretches of the assignment's remaining work: each
/// period's units over the work inside it between the activity's remaining
/// dates, on the assignment's calendar.
fn segments(
    schedule: &Schedule,
    id: AssignmentId,
    (code, wanted): (&str, &str),
    first: Date,
    typed: &Typed<'_>,
) -> Result<Vec<SpreadSegment>, EditError> {
    let assignment = &schedule.assignments[&id];
    let activity = &schedule.activities[&assignment.activity];
    // Remaining dates as a pair, or else early dates, which on started work
    // are the remaining work's too (TF-020), as `query --usage` takes them.
    let stored = &activity.stored;
    let Some((start, finish)) = stored
        .remaining_early_start
        .zip(stored.remaining_early_finish)
        .or_else(|| stored.early_start.zip(stored.early_finish))
    else {
        return refuse(
            "no-remaining-dates",
            format!("{code} has no remaining dates to spread over"),
            "calculate the schedule first; finished work has nothing to spread",
        );
    };
    let calendar = calendar_of(schedule, id)
        .and_then(|c| schedule.calendars.get(&c))
        .and_then(|c| WorkCalendar::new(c).ok());
    let Some(calendar) = calendar else {
        return refuse(
            "no-usable-calendar",
            format!("{code}'s calendar for {wanted} can't be used"),
            "`inspect` lists calendars and their problems",
        );
    };
    let total = calendar.work_between(start, finish).get();
    if total <= 0 {
        return refuse(
            "no-remaining-work",
            format!("{code} has no remaining work between {start} and {finish}"),
            "a spread needs remaining work",
        );
    }
    let out_of_range = || EditError {
        code: "invalid-spread",
        message: "the periods run past the supported dates".to_owned(),
        hint: "use dates between 1900 and 9999".to_owned(),
    };
    let mut segments = Vec::new();
    let mut covered = 0;
    let mut day = first;
    for (n, &units) in typed.units.iter().enumerate() {
        let next = typed.period.next(day).ok_or_else(out_of_range)?;
        let (from, to) = (
            day.at(TimeOfDay::MIDNIGHT).ok_or_else(out_of_range)?,
            next.at(TimeOfDay::MIDNIGHT).ok_or_else(out_of_range)?,
        );
        let (from, to) = (from.max(start), to.min(finish));
        if from >= to {
            return refuse(
                "spread-outside-dates",
                format!(
                    "period {} (from {day}) is outside {code}'s remaining dates, {start} to {finish}",
                    n + 1
                ),
                "type periods from the one holding the remaining start to the one holding the finish",
            );
        }
        let work = calendar.work_between(from, to);
        if work.get() == 0 {
            if units > 0.0 {
                return refuse(
                    "spread-outside-dates",
                    format!(
                        "period {} (from {day}) has no work for {wanted} on {code}, but {units} units",
                        n + 1
                    ),
                    "put units only in periods with work",
                );
            }
        } else {
            covered += work.get();
            segments.push(SpreadSegment { units, work });
        }
        day = next;
    }
    if covered < total {
        let hours = |minutes: i64| round(hours(Minutes::new(minutes)), 3);
        return refuse(
            "spread-outside-dates",
            format!(
                "the periods cover {} of the {} hours of {code}'s remaining work, {start} to {finish}",
                hours(covered),
                hours(total)
            ),
            "type periods from the one holding the remaining start to the one holding the finish",
        );
    }
    Ok(segments)
}

/// The calendar an assignment's work runs on: its resource's on a
/// resource-dependent activity, when the resource has one, and otherwise its
/// activity's (TF-027), as `totalfloat_engine::assignment_calendar` says.
fn calendar_of(schedule: &Schedule, id: AssignmentId) -> Option<CalendarId> {
    let assignment = schedule.assignments.get(&id)?;
    let activity = schedule.activities.get(&assignment.activity)?;
    let resource = assignment
        .resource
        .and_then(|r| schedule.resources.get(&r))
        .and_then(|r| r.calendar);
    match resource {
        Some(calendar) if activity.activity_type == ActivityType::ResourceDependent => {
            Some(calendar)
        }
        _ => Some(activity.calendar),
    }
}
